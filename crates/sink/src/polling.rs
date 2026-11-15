use crate::{Destination, Error, SecretsSource, body, cancelled, deadline};
use analyte_model::{Identifier, SampleId};
use analyte_store::{CachedOrder, Handle, OrderPage};
use tokio::sync::watch;

/// One polling owner per destination, committing every page before advancing.
/// Cancellation may leave the last page committed; restart resumes its stored cursor.
pub struct Poller<'a> {
    store: &'a Handle,
    destination: &'a Destination,
    secrets: &'a dyn SecretsSource,
}

impl<'a> Poller<'a> {
    /// Borrows host-owned resources; creates no background tasks.
    #[must_use]
    pub fn new(
        store: &'a Handle,
        destination: &'a Destination,
        secrets: &'a dyn SecretsSource,
    ) -> Self {
        Self {
            store,
            destination,
            secrets,
        }
    }

    /// Polls one bounded chain, starting with a snapshot if no cursor is stored.
    /// A 410 invalidates trust before returning; the next call starts a snapshot.
    /// # Errors
    /// Returns specific network, document, pagination, status or storage failures.
    pub async fn poll_once(&self) -> Result<(), Error> {
        let id = self.destination.id.clone();
        let mut cursor = self
            .store
            .request(deadline(self.destination.limits.store_timeout)?, move |s| {
                s.cursor(&id)
            })
            .await?;
        for _ in 0..self.destination.limits.max_pages {
            let page = tokio::time::timeout(
                self.destination.limits.request_timeout,
                self.fetch(cursor.as_deref()),
            )
            .await
            .map_err(|_| Error::Timeout)??;
            let Some(page) = page else {
                let id = self.destination.id.clone();
                self.store
                    .request(deadline(self.destination.limits.store_timeout)?, move |s| {
                        s.reset_orders(&id)
                    })
                    .await?;
                return Err(Error::PollStatus(410));
            };
            let fields = page.fields();
            if cursor.as_deref() == Some(fields.next_cursor.as_str()) && !fields.complete {
                return Err(Error::FeedProgress);
            }
            let complete = fields.complete;
            cursor = Some(fields.next_cursor.as_str().to_owned());
            let orders = fields
                .orders
                .iter()
                .map(cached)
                .collect::<Result<Vec<_>, _>>()?;
            let page = OrderPage {
                destination: self.destination.id.clone(),
                cursor: Identifier::new(fields.next_cursor.as_str())
                    .map_err(|_| Error::FeedProgress)?,
                complete,
                polled: jiff::Timestamp::now().as_second(),
                orders,
            };
            self.store
                .request(deadline(self.destination.limits.store_timeout)?, move |s| {
                    s.order_page(&page)
                })
                .await?;
            if complete {
                return Ok(());
            }
        }
        Err(Error::FeedProgress)
    }

    async fn fetch(
        &self,
        cursor: Option<&str>,
    ) -> Result<Option<analyte_contract::OrdersPage>, Error> {
        let credentials = self.destination.credentials(self.secrets)?;
        let mut url = self.destination.url("orders")?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("limit", &self.destination.limits.page_size.to_string());
            if let Some(cursor) = cursor {
                query.append_pair("cursor", cursor);
            }
        }
        let response = self
            .destination
            .client
            .get(url)
            .bearer_auth(&credentials.api_key)
            .send()
            .await?;
        let status = response.status().as_u16();
        if status == 410 {
            return Ok(None);
        }
        if status != 200 {
            return Err(Error::PollStatus(status));
        }
        let page: analyte_contract::OrdersPage = analyte_contract::decode(&body(response).await?)?;
        if page.fields().orders.len() > usize::from(self.destination.limits.page_size) {
            return Err(Error::FeedProgress);
        }
        Ok(Some(page))
    }

    /// Polls repeatedly, with bounded delays after both successful and failed polls.
    /// Cancellation stops active network and storage waits; submitted writes may commit.
    /// # Errors
    /// Store failures stop the owner; transient network/feed errors retry next interval.
    pub async fn run(&self, mut stop: watch::Receiver<bool>) -> Result<(), Error> {
        loop {
            let result = tokio::select! {biased; ()=cancelled(&mut stop)=>return Ok(()), result=self.poll_once()=>result};
            if let Err(error) = &result {
                tracing::warn!(destination=%self.destination.id.as_str(), error=%error, "order poll failed");
            }
            if let Err(Error::Store(error)) = result {
                return Err(Error::Store(error));
            }
            tokio::select! {biased; ()=cancelled(&mut stop)=>return Ok(()), ()=tokio::time::sleep(self.destination.limits.poll_interval)=>{}}
        }
    }
}

fn cached(order: &analyte_contract::Order) -> Result<CachedOrder, Error> {
    let fields = order.fields();
    let expiry = fields
        .expires_at
        .as_ref()
        .map(|s| {
            s.as_str()
                .parse::<jiff::Timestamp>()
                .map(jiff::Timestamp::as_second)
        })
        .transpose()
        .map_err(|_| Error::Document(analyte_contract::DocumentError::Field("expiry")))?
        .unwrap_or(i64::MAX);
    Ok(CachedOrder {
        id: Identifier::new(fields.id.as_str())
            .map_err(|_| Error::Document(analyte_contract::DocumentError::Field("order id")))?,
        sample: SampleId::new(fields.sample_id.as_str())
            .map_err(|_| Error::Document(analyte_contract::DocumentError::Field("sample id")))?,
        document: serde_json::to_string(order)?,
        tombstone: fields.tombstone,
        expiry,
    })
}
