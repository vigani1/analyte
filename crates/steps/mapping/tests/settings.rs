//! Selects protocol payloads from synthetic test snapshots.
use analyte_config::{
    AstmSyntax, Hl7Orders, Hl7Syntax, LawStack, Mappings, RecordTransport, Stack, Validated,
};
#[expect(
    clippy::panic,
    reason = "Synthetic fixture family is fixed by each test; a mismatch must fail the test"
)]
pub(crate) fn astm(config: &Validated) -> (&AstmSyntax, &Mappings) {
    match config.effective().stack() {
        Stack::Astm(RecordTransport::Stream(s)) => (s.settings().syntax(), s.settings().codes()),
        Stack::Astm(RecordTransport::Folder(s)) => (s.settings().syntax(), s.settings().codes()),
        stack @ (Stack::Hl7(_)
        | Stack::Law(_)
        | Stack::Poct(_)
        | Stack::Vidas(_)
        | Stack::Autolyser(_)) => panic!("expected ASTM fixture, got {stack:?}"),
    }
}
#[expect(
    clippy::panic,
    reason = "Synthetic fixture family is fixed by each test; a mismatch must fail the test"
)]
pub(crate) fn hl7(config: &Validated) -> (&Hl7Syntax, &Mappings) {
    match config.effective().stack() {
        Stack::Hl7(RecordTransport::Stream(s)) => (s.settings().syntax(), s.settings().codes()),
        Stack::Hl7(RecordTransport::Folder(s)) => (s.settings().syntax(), s.settings().codes()),
        stack @ (Stack::Astm(_)
        | Stack::Law(_)
        | Stack::Poct(_)
        | Stack::Vidas(_)
        | Stack::Autolyser(_)) => panic!("expected HL7 fixture, got {stack:?}"),
    }
}
#[expect(
    clippy::expect_used,
    reason = "Synthetic fixtures explicitly enable this capability; absence must fail the test"
)]
pub(crate) fn orders(config: &Validated) -> &Hl7Orders {
    config
        .effective()
        .stack()
        .hl7_orders()
        .expect("HL7 order fixture")
}
#[expect(
    clippy::expect_used,
    reason = "Synthetic fixtures explicitly enable this capability; absence must fail the test"
)]
pub(crate) fn law(config: &Validated) -> &LawStack {
    config.effective().stack().law().expect("LAW fixture")
}
