//! Supplies LAW order reservations and response metadata to the mapper.
use crate::installation::config::Machine;
use crate::orders::OrderError as Error;
use analyte_hl7::{Message, Options, law};
pub(super) use analyte_mapping::{LawCommand as Command, law_address as address};
use analyte_store::MachineOrder;
pub(super) fn command(machine: &Machine, app: &analyte_contract::Order) -> Result<Command, Error> {
    Ok(Command::new(&machine.snapshot, app)?)
}
pub(super) fn render(
    commands: &[(&Command, &MachineOrder)],
    action: law::WorkAction,
    options: Options,
) -> Result<Message, Error> {
    let items = commands
        .iter()
        .map(|(_, record)| {
            record
                .items
                .iter()
                .map(|item| analyte_mapping::OutboundItem {
                    placer: &item.placer,
                    namespace: item.placer_namespace.as_ref(),
                    tests: &item.tests,
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let refs = commands
        .iter()
        .zip(&items)
        .map(|((command, _), items)| (*command, items.as_slice()))
        .collect::<Vec<_>>();
    crate::machine::hl7_response::response_metadata(crate::machine::hl7_response::Profile::Law)
        .build(|response| {
            Ok(analyte_mapping::map_law_work(
                &refs, action, options, response,
            )?)
        })
}
