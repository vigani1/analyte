//! Checks LAW query and send-ahead reservations and cancellation eligibility (TF-2b 3.28.4).
use super::{
    Connection, Exchange,
    work::{Command, render},
};
use crate::installation::config::Machine;
use crate::machine::hooks::Operation;
use crate::orders::OrderError as Error;
use crate::orders::outcome::Code;
use analyte_hl7::{Message, Options, law};
use analyte_model::SourceText;
use analyte_store::{Handle, MachineOrder};
use std::time::{Duration, Instant};
impl Connection {
    /// Admits at most one idle exchange. Once reserved, only an outcome may change it;
    /// connection cancellation recovers owned in-flight store calls through `close`.
    pub(crate) async fn idle(
        &mut self,
        store: &Handle,
        machine: &Machine,
        options: Options,
    ) -> Result<Option<Message>, Error> {
        if self.exchange.is_some() {
            return Ok(None);
        }
        let cfg = super::super::settings::law(machine)?;
        let Some(settings) = &cfg.orders() else {
            return Ok(None);
        };
        let until = Instant::now()
            .checked_add(Duration::from_millis(u64::from(
                cfg.settings().timeouts().send_timeout_ms(),
            )))
            .ok_or(Error::LawPreparationDeadlineOverflow)?;
        let cancels = super::super::reservation::cancellations(store, machine, until).await?;
        if let Some(sent) = self
            .idle_cancel(store, machine, options, until, cancels)
            .await?
        {
            return Ok(Some(sent));
        }
        if !settings.send_ahead() {
            return Ok(None);
        }
        self.idle_work(store, machine, options, until).await
    }

    async fn idle_cancel(
        &mut self,
        store: &Handle,
        machine: &Machine,
        options: Options,
        until: Instant,
        cancels: Vec<(MachineOrder, analyte_store::CachedOrder)>,
    ) -> Result<Option<Message>, Error> {
        let freshness = machine.destination_limits.freshness.as_secs();
        for (record, cached) in cancels {
            let app: analyte_contract::Order =
                analyte_contract::decode(cached.document.as_bytes())?;
            let command = super::work::command(machine, &app)?;
            let sent = render(&[(&command, &record)], law::WorkAction::Cancel, options)?;
            let id = record.id;
            let version = app.fields().version;
            let owner = self.owner.clone();
            self.reserving.push(record.key.clone());
            machine
                .hooks
                .operation(Operation::Cancel)
                .await
                .map_err(Error::TestHook)?;
            let admission =
                super::super::reservation::cancel(store, until, id, version, owner, freshness)
                    .await;
            machine
                .hooks
                .operation(Operation::CancelResult)
                .await
                .map_err(Error::TestHook)?;
            match admission {
                Ok(record) => {
                    self.exchange = Some(Exchange {
                        records: vec![record],
                        protocol: analyte_dialog::law::WorkExchange::new(
                            sent.clone(),
                            until.duration_since(self.clock),
                        ),
                        query: None,
                    });
                    return Ok(Some(sent));
                }
                Err(analyte_store::Error::Conflict) => {
                    self.reserving.clear();
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(None)
    }

    async fn idle_work(
        &mut self,
        store: &Handle,
        machine: &Machine,
        options: Options,
        until: Instant,
    ) -> Result<Option<Message>, Error> {
        let freshness = machine.destination_limits.freshness.as_secs();
        let candidates =
            super::super::sending::candidate_page(store, machine, &mut self.cursor).await?;
        for cached in candidates {
            self.cursor = cached.id.as_str().to_owned();
            let app: analyte_contract::Order =
                analyte_contract::decode(cached.document.as_bytes())?;
            if !super::super::eligible(&app, machine) {
                continue;
            }
            let key = super::super::reservation::key(machine, cached.id.clone());
            let version = app.fields().version;
            let prepared = match self
                .reserve(store, machine, vec![cached], until, options)
                .await
            {
                Ok(prepared) => prepared,
                Err(error) => {
                    if let Err(cleanup) = self
                        .close(store, Code::LawSendPreparationFailed.as_str())
                        .await
                    {
                        return Err(crate::machine::custody::effect_failure(
                            Some(error),
                            cleanup,
                        ));
                    }
                    if matches!(
                        &error,
                        Error::Store(
                            analyte_store::Error::DeliveryBuild(_)
                                | analyte_store::Error::ItemIdentityTooLong
                        )
                    ) {
                        let reason = SourceText::new(
                            &crate::orders::outcome::Detail::LawOrderPreflight(error.to_string())
                                .to_string(),
                        )?;
                        super::super::reservation::reject(
                            store, until, key, version, freshness, reason,
                        )
                        .await??;
                        continue;
                    }
                    return Err(error);
                }
            };
            if prepared.is_empty() {
                continue;
            }
            let refs = prepared
                .iter()
                .map(|(command, record)| (command, record))
                .collect::<Vec<_>>();
            let sent = render(&refs, law::WorkAction::New, options)?;
            self.exchange = Some(Exchange {
                records: prepared.into_iter().map(|(_, record)| record).collect(),
                protocol: analyte_dialog::law::WorkExchange::new(
                    sent.clone(),
                    until.duration_since(self.clock),
                ),
                query: None,
            });
            return Ok(Some(sent));
        }
        Ok(None)
    }

    /// The same preflight and per-AWOS reservation admits queried and unsolicited work.
    pub(super) async fn reserve(
        &mut self,
        store: &Handle,
        machine: &Machine,
        cached: Vec<analyte_store::CachedOrder>,
        until: Instant,
        options: Options,
    ) -> Result<Vec<(Command, MachineOrder)>, Error> {
        let freshness = machine.destination_limits.freshness.as_secs();
        let mut prepared = Vec::new();
        for cached in cached {
            let app: analyte_contract::Order =
                analyte_contract::decode(cached.document.as_bytes())?;
            if !super::super::eligible(&app, machine) {
                continue;
            }
            if Instant::now() >= until {
                return Err(Error::LawQueryDeadlineBeforeReservation);
            }
            let command = super::work::command(machine, &app)?;
            let key = super::super::reservation::key(machine, cached.id);
            let version = app.fields().version;
            let identity = key.clone();
            let owner = self.owner.clone();
            let namespace = command.placer_namespace().clone();
            let groups = command
                .tests()
                .iter()
                .cloned()
                .map(|test| vec![test])
                .collect::<Vec<_>>();
            // Keep the existing logical identity budget; rendering independently owns
            // LAW's EI.1-only wire form and validates it against the profile.
            let max_identity_bytes = law::AWOS_ID_MAX_BYTES
                .saturating_sub(namespace.as_str().len())
                .saturating_sub(1);
            self.reserving.push(key);
            let reserved = super::super::reservation::admit(
                store,
                until,
                super::super::reservation::Admission {
                    identity,
                    version,
                    freshness,
                    owner,
                    groups,
                    namespace,
                    max_identity_bytes,
                },
                move |record| {
                    render(&[(&command, record)], law::WorkAction::New, options)
                        .map_err(|error| analyte_store::OrderError::DeliveryBuild(error.into()))?;
                    Ok(command)
                },
            )
            .await?
            .map_err(Error::Refused)?;

            let bytes = render(&[(&reserved.1, &reserved.0)], law::WorkAction::New, options)?
                .raw()
                .to_vec();
            let identity = reserved.0.key.clone();
            let owner = self.owner.clone();
            store
                .request(until, move |store| {
                    store.retain_order_command(&identity, &owner, bytes)
                })
                .await?;
            prepared.push((reserved.1, reserved.0));
        }
        Ok(prepared)
    }
}
