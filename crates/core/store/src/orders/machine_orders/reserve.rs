//! Reserves machine orders with durable identities.
use super::RESERVATION_OWNER_PREFIX;
use super::{
    CachedFields, CachedOrder, Error, Identifier, MachineOrder, MachineOrderKey,
    OrderEventDelivery, OrderOwner, OrderReservation, OrderState, OrderTransition, ReserveOptions,
    SendRefusal, SourceText, SqlOrderState, Store, TransactionBehavior, append, create_items,
    params,
};
use rusqlite::OptionalExtension as _;

impl Store {
    /// Atomically rechecks the current cache and persists Sending and history without an app event.
    /// Once committed, cancellation or a lost reply requires reconciliation,
    /// never a second automatic reservation. Only a later real outcome builds
    /// contract bytes; an internal reservation must not be reported as sent.
    /// # Errors
    /// Returns storage failures; expected eligibility refusals are values.
    pub fn reserve_send(
        &mut self,
        candidate: &OrderReservation<'_>,
        options: ReserveOptions<'_>,
    ) -> Result<Result<MachineOrder, SendRefusal>, Error> {
        let code = match options.owner {
            Some(owner) => format!("{RESERVATION_OWNER_PREFIX}{}", owner.as_str()),
            None => "host-reserved".to_owned(),
        };
        self.start_order(
            candidate,
            OrderState::Sending,
            SourceText::new(&code).map_err(crate::StorageError::Text)?,
            options,
            None::<fn(&MachineOrder) -> Result<OrderEventDelivery, Error>>,
        )
    }

    /// Records a mapping rejection and app event without claiming anything was sent.
    /// # Errors
    /// Reports cache refusals, builder errors and storage failures.
    pub fn reject_unsent(
        &mut self,
        candidate: &OrderReservation<'_>,
        reason: SourceText,
        build: impl FnOnce(&MachineOrder) -> Result<OrderEventDelivery, Error>,
    ) -> Result<Result<MachineOrder, SendRefusal>, Error> {
        self.start_order(
            candidate,
            OrderState::Rejected,
            reason,
            ReserveOptions::default(),
            Some(build),
        )
    }

    pub(super) fn start_order(
        &mut self,
        candidate: &OrderReservation<'_>,
        initial: OrderState,
        code: SourceText,
        options: ReserveOptions<'_>,
        build: Option<impl FnOnce(&MachineOrder) -> Result<OrderEventDelivery, Error>>,
    ) -> Result<Result<MachineOrder, SendRefusal>, Error> {
        let OrderReservation {
            identity,
            version,
            now,
            freshness,
        } = *candidate;
        self.capacity(self.limits.record_bytes)?;
        let limit = self.limits.record_bytes;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if crate::upkeep::retention::orders::was_purged(&tx, self.retention_key.as_ref(), identity)?
        {
            return Err(Error::Purged);
        }
        let trust = Self::order_trust_on(&tx, &identity.destination, now, freshness)?;
        if trust == crate::OrderTrust::RestorePaused {
            return Ok(Err(SendRefusal::RestoreUnreconciled));
        }
        let existing = tx
            .query_row(
                "SELECT id FROM machine_orders WHERE destination=?1 AND order_id=?2 AND machine=?3",
                params![
                    identity.destination.as_str(),
                    identity.order.as_str(),
                    identity.machine.as_str()
                ],
                |r| r.get::<_, i64>(0),
            )
            .optional()?;
        if existing.is_some() {
            return Ok(Err(SendRefusal::AlreadyReserved));
        }
        if !trust.is_trusted() {
            return Ok(Err(SendRefusal::Untrusted));
        }
        let cached = tx.query_row("SELECT evidence FROM orders WHERE destination=?1 AND id=?2 AND generation=(SELECT generation FROM feed WHERE destination=?1)",params![identity.destination.as_str(),identity.order.as_str()], |r| r.get::<_,String>(0)).optional()?;
        let Some(cached) = cached else {
            return Ok(Err(SendRefusal::Missing));
        };
        let cached: CachedOrder = serde_json::from_str(&cached)?;
        let fields: CachedFields = serde_json::from_str(&cached.document)?;
        if fields.version != version {
            return Ok(Err(SendRefusal::VersionChanged));
        }
        if cached.tombstone {
            return Ok(Err(SendRefusal::Withdrawn));
        }
        if cached.expiry <= now {
            return Ok(Err(SendRefusal::Expired));
        }
        match fields.state.eligible_for(
            OrderState::Requested,
            fields.tombstone,
            fields
                .target_analyzers
                .as_ref()
                .map(|targets| targets.iter().map(Identifier::as_str)),
            identity.machine.as_str(),
        ) {
            Ok(()) => {}
            Err(analyte_model::OrderIneligible::Withdrawn) => {
                return Ok(Err(SendRefusal::Withdrawn));
            }
            Err(analyte_model::OrderIneligible::Phase) => {
                return Ok(Err(SendRefusal::NotRequested));
            }
            Err(analyte_model::OrderIneligible::Target) => {
                return Ok(Err(SendRefusal::WrongMachine));
            }
        }
        let state = OrderState::Requested.transition(OrderTransition::Advance(initial))?;
        tx.execute("INSERT INTO machine_orders(destination,order_id,machine,version,state,created_at,updated_at,source_code) VALUES (?1,?2,?3,?4,?5,?6,?6,?7)",params![identity.destination.as_str(),identity.order.as_str(),identity.machine.as_str(),version.to_string(),SqlOrderState(state),now,code.as_str()])?;
        let mut record = MachineOrder {
            id: tx.last_insert_rowid(),
            key: identity.clone(),
            version,
            state,
            created_at: now,
            updated_at: now,
            source_code: code,
            owner: options.owner.map_or(OrderOwner::None, |owner| {
                OrderOwner::Reservation(owner.clone())
            }),
            items: Vec::new(),
            original_command: None,
        };
        record.items = create_items(&tx, &record, fields.tests, options.items, limit)?;
        if let Some(prepare) = options.prepare {
            prepare(&record)?;
        }
        let event = build.map(|build| build(&record)).transpose()?;
        append(&tx, &record, event.as_ref(), limit)?;
        tx.commit()?;
        Ok(Ok(record))
    }
}

impl Store {
    /// Retains immutable prepared bytes before I/O, only for an owned sending reservation.
    /// Cancellation after this commit never grants permission to resend.
    /// # Errors
    /// Reports capacity, ownership, changed evidence or database failures.
    pub fn retain_order_command(
        &mut self,
        identity: &MachineOrderKey,
        owner: &Identifier,
        bytes: Vec<u8>,
    ) -> Result<(), Error> {
        self.capacity(self.limits.record_bytes)?;
        let limit = self.limits.record_bytes;
        if bytes.is_empty() || bytes.len() > limit {
            return Err(Error::InvalidOrderInput);
        }
        let record = self.machine_order(identity)?.ok_or(Error::Conflict)?;
        if !record.reserved_by(owner) || record.original_command.is_some() {
            return Err(Error::Conflict);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut record = record;
        record.original_command = Some(bytes);
        append(&tx, &record, None, limit)?;
        tx.commit()?;
        Ok(())
    }
}
