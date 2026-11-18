//! Supplies LAW order reservations and response metadata to the mapper.
use crate::machine::hl7::orders::response::{Profile, response_metadata};
use crate::machine::law::error::OrderError as Error;
use analyte_hl7::{Message, Options, law};
pub(super) use analyte_mapping::{LawCommand as Command, law_address as address};
use analyte_store::MachineOrder;
pub(super) fn command(
    settings: super::Settings<'_>,
    app: &analyte_contract::Order,
) -> Result<Command, Error> {
    Ok(analyte_mapping::map_law_order(settings.stack, app)?)
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
    response_metadata(Profile::Law).build(|response| {
        Ok(analyte_mapping::map_law_work(
            &refs, action, options, response,
        )?)
    })
}
