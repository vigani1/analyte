//! ADR 0011; `PaLM` TF-2b Rev 11 C.7 Table C.7-18 p.110 and Figure C.7-1 p.111.
/// Permitted changes to retained content within one observation identity.
pub const STATUS_TRANSITIONS: &[(&str, &str)] = &[
    ("P", "P"),
    ("P", "R"),
    ("P", "F"),
    ("R", "R"),
    ("R", "F"),
    ("F", "C"),
    ("C", "C"),
];
/// Source statuses retained without result delivery.
pub const WITHHELD_STATUSES: &[&str] = &["R"];
/// At most one run of an AWOS/test may produce these statuses.
pub const SINGLE_RUN_STATUSES: &[&str] = &["F", "C"];
/// Extracts the explicit run identifier from OG.1 (C.7, pp.102-103).
/// No run is inferred from group or sequence components.
#[must_use]
pub fn observation_run(sub_id: &str) -> Option<&str> {
    sub_id.split('^').next().filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    #[test]
    fn adr0011_law_c7_figure_1_every_permitted_and_forbidden_move() {
        for from in ["P", "R", "F", "C", "X"] {
            for to in ["P", "R", "F", "C", "X"] {
                let expected = matches!(
                    (from, to),
                    ("P", "P" | "R" | "F") | ("R", "R" | "F") | ("F", "C") | ("C", "C")
                );
                assert_eq!(
                    super::STATUS_TRANSITIONS.contains(&(from, to)),
                    expected,
                    "{from}->{to}"
                );
            }
        }
        assert_eq!(super::observation_run("2^1^3"), Some("2"));
        assert_eq!(super::observation_run(""), None);
    }
}
