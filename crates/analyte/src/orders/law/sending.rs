//! Checks LAW query and send-ahead reservations and cancellation eligibility (TF-2b 3.28.4).
use super::{
    Exchange, LawOrderLink,
    work::{Command, render},
};
use crate::installation::config::Machine;
use crate::orders::OrderError as Error;
use crate::orders::outcome::Code;
use crate::runtime::hooks::Operation;
use analyte_hl7::{Message, Options, law};
use analyte_store::{Handle, MachineOrder};
use std::time::{Duration, Instant};
impl LawOrderLink {
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
                Err(analyte_store::OrderError::Conflict) => {
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
        let candidates =
            super::super::sending::candidate_page(store, machine, &mut self.cursor).await?;
        for cached in candidates {
            self.cursor = cached.id.as_str().to_owned();
            let prepared = match self
                .reserve(store, machine, vec![cached], until, options, true)
                .await
            {
                Ok(prepared) => prepared,
                Err(error) => {
                    if let Err(cleanup) = self
                        .close(store, Code::LawSendPreparationFailed.as_str())
                        .await
                    {
                        return Err(crate::cleanup::effect_failure(Some(error), cleanup));
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
        unsolicited: bool,
    ) -> Result<Vec<(Command, MachineOrder)>, Error> {
        let freshness = machine.destination_limits.freshness.as_secs();
        let mut prepared = Vec::new();
        for cached in cached {
            if Instant::now() >= until {
                return Err(Error::LawQueryDeadlineBeforeReservation);
            }
            let (candidate, command) = match super::super::admission::prepare(
                store,
                machine,
                cached,
                until,
                |error| format!("rejected: {error}"),
                |app| super::work::command(machine, app),
            )
            .await?
            {
                super::super::admission::Prepared::Mapped(candidate, command) => {
                    (candidate, command)
                }
                super::super::admission::Prepared::Skipped => continue,
                super::super::admission::Prepared::Rejected(error) => {
                    if unsolicited {
                        continue;
                    }
                    return Err(error);
                }
            };
            let key = candidate.identity;
            let version = candidate.version;
            let identity = key.clone();
            let owner = self.owner.clone();
            let namespace = command.placer_namespace().clone();
            let groups = groups(&command);
            // Keep the existing logical identity budget; rendering independently owns
            // LAW's EI.1-only wire form and validates it against the profile.
            let max_identity_bytes = law::AWOS_ID_MAX_BYTES
                .saturating_sub(namespace.as_str().len())
                .saturating_sub(1);
            self.reserving.push(key.clone());
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
            .await;
            let reserved = match reserved {
                Ok(Ok(reserved)) => reserved,
                Ok(Err(refusal)) => {
                    if !unsolicited {
                        return Err(Error::Refused(refusal));
                    }
                    tracing::info!(reason=%refusal, "LAW order reservation refused");
                    self.reserving.clear();
                    continue;
                }
                Err(
                    error @ (analyte_store::OrderError::DeliveryBuild(_)
                    | analyte_store::OrderError::ItemIdentityTooLong),
                ) => {
                    self.reserving.clear();
                    if !unsolicited {
                        return Err(error.into());
                    }
                    super::super::admission::reject(
                        store,
                        machine,
                        until,
                        crate::orders::Candidate {
                            identity: key,
                            version,
                        },
                        &crate::orders::outcome::Detail::LawOrderPreflight(error.to_string())
                            .to_string(),
                    )
                    .await?;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };

            self.retain_command(store, until, options, &reserved)
                .await?;
            prepared.push((reserved.1, reserved.0));
        }
        Ok(prepared)
    }
    async fn retain_command(
        &self,
        store: &Handle,
        until: Instant,
        options: Options,
        reserved: &(MachineOrder, Command),
    ) -> Result<(), Error> {
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
        Ok(())
    }
}

fn groups(command: &Command) -> Vec<Vec<analyte_model::NamespacedId>> {
    command
        .tests()
        .iter()
        .cloned()
        .map(|test| vec![test])
        .collect()
}
