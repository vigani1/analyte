//! Translates Roche ASTM dilution selectors and ratios in both directions.
use analyte_astm::OrderProfile;

// Roche cobas e 411 HIM v2.3, Tables 16?18: Elecsys uses selector codes
// while cobas uses the ratio text on the wire.
const SELECTORS: &[(&str, &str)] = &[
    ("0", "1"),
    ("1", "2"),
    ("2", "5"),
    ("3", "10"),
    ("5", "20"),
    ("7", "50"),
    ("9", "100"),
];

fn wire(profile: OrderProfile, selector: &'static str, ratio: &'static str) -> &'static str {
    match profile {
        OrderProfile::Elecsys => selector,
        OrderProfile::Cobas => ratio,
    }
}

pub(super) fn decode(profile: OrderProfile, code: &str) -> Option<&'static str> {
    SELECTORS
        .iter()
        .find_map(|&(selector, ratio)| (wire(profile, selector, ratio) == code).then_some(ratio))
}

pub(super) fn encode(profile: OrderProfile, ratio: u16) -> Option<&'static str> {
    let ratio = ratio.to_string();
    SELECTORS.iter().find_map(|&(selector, factor)| {
        (factor == ratio).then_some(wire(profile, selector, factor))
    })
}
