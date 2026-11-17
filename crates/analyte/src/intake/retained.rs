//! Shares one parsed retained HL7 message across mapping, custody, diagnostics and departure evidence.
use super::{Error, InterpretationOutcome, Machine, Mapping, ParkReason};
use crate::machine::interpretation::ReadError;
use analyte_config::{RecordTransport, Stack};
use analyte_hl7::{Message, WireCompatibility, law};
use analyte_store::{Handle, InterpretationCommit, Receipt, Reprocess};

pub(super) struct Context<'a> {
    pub receipt: &'a Receipt,
    pub results: Option<&'a law::Results<'a>>,
    pub wire: Option<WireCompatibility>,
    payload: Option<&'a [u8]>,
}

pub(crate) type Outcome = (
    InterpretationOutcome,
    InterpretationCommit,
    Option<crate::machine::hl7::error_location::Location>,
);

enum Profile<'a> {
    Hl7(&'a analyte_config::Hl7Syntax, &'a analyte_config::Mappings),
    Law(&'a analyte_config::LawStack),
}
// Cancellation before persistence changes no durable state. Once a store request
// is submitted, its owner must reconcile the same receipt/interpretation identity.
pub(crate) async fn process(
    store: &Handle,
    machine: &Machine,
    receipt: &Receipt,
    supplied: Option<Mapping>,
    action: Option<Reprocess>,
) -> Result<Outcome, Error> {
    let profile = match machine.snapshot.effective().stack() {
        Stack::Hl7(RecordTransport::Stream(s)) => {
            Some(Profile::Hl7(s.settings().syntax(), s.settings().codes()))
        }
        Stack::Hl7(RecordTransport::Folder(s)) => {
            Some(Profile::Hl7(s.settings().syntax(), s.settings().codes()))
        }
        Stack::Law(s) => Some(Profile::Law(s)),
        Stack::Astm(_) | Stack::Poct(_) | Stack::Vidas(_) | Stack::Autolyser(_) => None,
    };
    let Some(profile) = profile else {
        let mapping = if supplied.is_some() {
            Ok(analyte_mapping::Output::default())
        } else {
            crate::machine::interpretation::map_retained(store, machine, receipt).await
        };
        return finish(
            store,
            machine,
            Context {
                receipt,
                results: None,
                wire: None,
                payload: None,
            },
            mapping,
            supplied,
            action,
        )
        .await;
    };
    let payload = match crate::reading::payload(&machine.snapshot, &receipt.raw) {
        Ok(payload) => payload,
        Err(error) => {
            return finish(
                store,
                machine,
                Context {
                    receipt,
                    results: None,
                    wire: None,
                    payload: None,
                },
                Err(error.into()),
                supplied,
                action,
            )
            .await;
        }
    };
    let parsed = match &profile {
        Profile::Law(settings) => law::parse_message(&payload, settings.options()),
        Profile::Hl7(syntax, _) => Message::parse(&payload, syntax.options()),
    };
    let context = Context {
        receipt,
        results: None,
        wire: None,
        payload: Some(&payload),
    };
    let message = match parsed {
        Ok(message) => message,
        Err(error) => {
            return finish(
                store,
                machine,
                context,
                Err(crate::reading::Error::from(error).into()),
                supplied,
                action,
            )
            .await;
        }
    };
    let context = Context {
        wire: Some(analyte_hl7::wire_compatibility(&message)),
        ..context
    };
    map_parsed(store, machine, profile, &message, context, supplied, action).await
}

async fn map_parsed(
    store: &Handle,
    machine: &Machine,
    profile: Profile<'_>,
    message: &Message,
    context: Context<'_>,
    supplied: Option<Mapping>,
    action: Option<Reprocess>,
) -> Result<Outcome, Error> {
    let receipt = context.receipt;
    let input = analyte_mapping::Input {
        receipt: receipt.metadata.id,
        received_at: receipt.metadata.received_at,
        revision: analyte_model::Revision(0),
    };
    match profile {
        Profile::Hl7(syntax, mappings) => {
            let mapped = analyte_mapping::map_hl7(
                message,
                syntax,
                mappings,
                &machine.snapshot.effective().machine().deviations,
                input,
            )
            .map_err(crate::reading::Error::from)
            .map_err(ReadError::from);
            finish(store, machine, context, mapped, supplied, action).await
        }
        Profile::Law(settings) => {
            let results = match law::receive_results(message) {
                Ok(results) => results,
                Err(error) => {
                    return finish(
                        store,
                        machine,
                        context,
                        Err(crate::reading::Error::from(error).into()),
                        supplied,
                        action,
                    )
                    .await;
                }
            };
            let context = Context {
                results: Some(&results),
                ..context
            };
            let mapped = analyte_mapping::map_law(&results, settings, input)
                .map_err(crate::reading::Error::from)
                .map_err(ReadError::from);
            finish(store, machine, context, mapped, supplied, action).await
        }
    }
}

async fn finish(
    store: &Handle,
    machine: &Machine,
    context: Context<'_>,
    mapped: Result<analyte_mapping::Output, ReadError>,
    supplied: Option<Mapping>,
    action: Option<Reprocess>,
) -> Result<Outcome, Error> {
    let location = context.payload.and_then(|payload| {
        let options = machine.snapshot.effective().hl7_options()?;
        crate::machine::hl7::error_location::mapping_location(&mapped, payload, options.terminator)
    });
    let mapping = supplied.unwrap_or_else(|| match mapped {
        Ok(output) => Mapping::Mapped(output),
        Err(error) => {
            let reason = match machine.snapshot.effective().stack() {
                Stack::Vidas(_) => format!("VIDAS interpretation: {error}"),
                Stack::Autolyser(_) => format!("Autolyser interpretation: {error}"),
                Stack::Poct(_) => format!("POCT interpretation: {error}"),
                Stack::Astm(_) | Stack::Hl7(_) | Stack::Law(_) => error.to_string(),
            };
            Mapping::Parked(ParkReason::Diagnostic(reason))
        }
    });
    let (outcome, commit) =
        super::persist_prepared(store, machine, &context, mapping, action).await?;
    Ok((outcome, commit, location))
}
