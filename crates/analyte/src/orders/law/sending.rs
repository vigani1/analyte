//! Shared query/send-ahead reservation and cancellation admission; TF-2b 3.28.4.
use super::{
    Connection, Exchange,
    work::{Command, render},
};
use crate::{Error, installation::config::Machine, invalid};
use analyte_hl7::{Message, Options, law};
use analyte_model::SourceText;
use analyte_store::{Handle, MachineOrder, MachineOrderKey, OrderItems, OrderReservation};
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
                cfg.timeouts().send_timeout_ms(),
            )))
            .ok_or("LAW preparation deadline overflow")?;
        let freshness = machine.destination_limits.freshness.as_secs();
        let destination = super::super::destination(machine);
        let target = machine.id.clone();
        let cancels = store
            .request(until, move |store| {
                if !store
                    .order_trust(&destination, jiff::Timestamp::now().as_second(), freshness)?
                    .is_trusted()
                {
                    return Ok(None);
                }
                store
                    .cancel_candidates(
                        &destination,
                        &target,
                        jiff::Timestamp::now().as_second(),
                        freshness,
                    )
                    .map(Some)
            })
            .await?;
        let Some(cancels) = cancels else {
            return Ok(None);
        };
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
            match super::super::reservation::cancel(store, until, id, version, owner, freshness)
                .await
            {
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
            let key = MachineOrderKey {
                destination: super::super::destination(machine),
                order: cached.id.clone(),
                machine: machine.id.clone(),
            };
            let version = app.fields().version;
            let prepared = match self
                .reserve(store, machine, vec![cached], until, options)
                .await
            {
                Ok(prepared) => prepared,
                Err(error) => {
                    self.close(store, "LAW-send-preparation-failed").await?;
                    if matches!(
                        error.downcast_ref::<analyte_store::Error>(),
                        Some(
                            analyte_store::Error::DeliveryBuild(_)
                                | analyte_store::Error::ItemIdentityTooLong
                        )
                    ) {
                        let reason = SourceText::new(&format!("LAW-order-preflight:{error}"))?;
                        store
                            .request(until, move |store| {
                                store.reject_unsent(
                                    &OrderReservation {
                                        identity: &key,
                                        version,
                                        now: jiff::Timestamp::now().as_second(),
                                        freshness,
                                    },
                                    reason,
                                    super::super::event,
                                )
                            })
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
                return Err(invalid("LAW query deadline before reservation"));
            }
            let command = super::work::command(machine, &app)?;
            let key = MachineOrderKey {
                destination: super::super::destination(machine),
                order: cached.id,
                machine: machine.id.clone(),
            };
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
            let reserved = store
                .request(until, move |store| {
                    store
                        .reserve_send(
                            &super::super::reservation::candidate(&identity, version, freshness),
                            analyte_store::ReserveOptions {
                                items: OrderItems::Groups {
                                    groups: &groups,
                                    namespace: Some(&namespace),
                                    max_identity_bytes,
                                },
                                owner: Some(&owner),
                                prepare: Some(&mut |record| {
                                    render(&[(&command, record)], law::WorkAction::New, options)
                                        .map_err(analyte_store::Error::DeliveryBuild)?;
                                    Ok(())
                                }),
                            },
                        )
                        .map(|reserved| reserved.map(|record| (record, command)))
                })
                .await?
                .map_err(std::io::Error::other)?;
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
