//! Shares one parsed retained HL7 message across mapping, custody, diagnostics and departure evidence.
use crate::installation::config::Machine;
use crate::intake::{Error, InterpretationOutcome, Mapping, ParkReason};
use crate::machine::interpretation::ReadError;

use analyte_hl7::{Message, WireCompatibility, law};
use analyte_store::{Handle, InterpretationCommit, Receipt, Reprocess};

pub(crate) struct Context<'a> {
    pub receipt: &'a Receipt,
    pub results: Option<&'a law::Results<'a>>,
    pub wire: Option<WireCompatibility>,
    payload: Option<&'a [u8]>,
}

impl Context<'_> {
    pub(crate) async fn policy(
        &self,
        store: &Handle,
        machine: &Machine,
    ) -> Result<super::policy::Policy, Error> {
        use super::policy::Policy;
        let stack = machine.snapshot.effective().stack();
        if stack.law().is_some() {
            Ok(Policy::Execution(
                crate::machine::law::policy::Policy::prepare(store, machine, self.results).await?,
            ))
        } else if stack.choice().connection_context() {
            Ok(crate::machine::poct::policy())
        } else {
            Ok(Policy::standard())
        }
    }
}

pub(crate) type Outcome = (
    InterpretationOutcome,
    InterpretationCommit,
    Option<crate::machine::hl7::location::Location>,
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
    let stack = machine.snapshot.effective().stack();
    let profile = stack.law().map(Profile::Law).or_else(|| {
        stack
            .hl7_record_settings()
            .map(|(syntax, mappings)| Profile::Hl7(syntax, mappings))
    });
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
    let payload = match crate::machine::protocols::payload(&machine.snapshot, &receipt.raw) {
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
        Profile::Law(settings) => law::parse_message(
            &payload,
            crate::machine::law::settings::options(settings, machine.snapshot.effective().site()),
        ),
        Profile::Hl7(syntax, _) => {
            Message::parse(&payload, crate::machine::hl7::settings::options(syntax))
        }
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
        device: None,
        receipt: receipt.metadata.id,
        received_at: receipt.metadata.received_at,
        revision: analyte_model::Revision(0),
    };
    match profile {
        Profile::Hl7(syntax, mappings) => {
            let mapped = analyte_mapping::hl7::map(
                &analyte_mapping::hl7::Settings {
                    syntax,
                    mappings,
                    declarations: &machine.snapshot.effective().machine().deviations,
                },
                message,
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
            let mapped = analyte_mapping::law::map(settings, &results, input)
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
        let options =
            crate::machine::hl7::settings::effective_options(machine.snapshot.effective())?;
        crate::machine::hl7::location::mapping_location(&mapped, payload, options.terminator)
    });
    let mapping = supplied.unwrap_or_else(|| match mapped {
        Ok(output) => Mapping::Mapped(output),
        Err(error) => {
            let reason = format!(
                "{}{error}",
                machine.snapshot.effective().stack().interpretation_prefix()
            );
            Mapping::Parked(ParkReason::Diagnostic(reason))
        }
    });
    let (outcome, commit) =
        crate::intake::persist_prepared(store, machine, &context, mapping, action).await?;
    Ok((outcome, commit, location))
}
