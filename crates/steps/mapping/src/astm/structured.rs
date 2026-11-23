//! Maps the documented six-component ASTM query and download layout.
use super::{AppOrder, OrderError, map_priority, map_specimen, map_test, order_error};
use analyte_astm::{OrderProfile, OrderRecord, StructuredQuery};
use analyte_config::{AstmOrders, SyntaxLimits};
use std::collections::BTreeSet;

/// Maps an order against the original query coordinates.
/// # Errors
/// Refuses inconsistent or unrepresentable work before reservation.
pub fn map_structured_query(
    order: &AppOrder,
    limits: &SyntaxLimits,
    settings: &AstmOrders,
    profile: OrderProfile,
    query: &StructuredQuery,
) -> Result<OrderRecord, OrderError> {
    map(Some(order), limits, settings, profile, Some(query))
}

/// Builds the documented empty test list for an identified sample.
/// # Errors
/// Refuses an unresolved specimen or unknown mandatory specimen code.
pub fn no_structured_orders(
    limits: &SyntaxLimits,
    settings: &AstmOrders,
    profile: OrderProfile,
    query: &StructuredQuery,
) -> Result<OrderRecord, OrderError> {
    map(None, limits, settings, profile, Some(query))
}

pub(super) fn map(
    order: Option<&AppOrder>,
    limits: &SyntaxLimits,
    settings: &AstmOrders,
    profile: OrderProfile,
    query: Option<&StructuredQuery>,
) -> Result<OrderRecord, OrderError> {
    let sample = match (order, query) {
        (Some(order), Some(query)) if query.sample() == Some(order.sample.as_str()) => {
            order.sample.as_str()
        }
        (Some(order), None) if profile == OrderProfile::Cobas => order.sample.as_str(),
        (None, Some(query)) => query.sample().ok_or(OrderError::SpecimenMismatch)?,
        _ => return Err(OrderError::SpecimenMismatch),
    };
    if sample.len() > 22 || sample.starts_with('@') {
        return Err(OrderError::SpecimenMismatch);
    }
    let mut fields: [Vec<Vec<String>>; 24] = std::array::from_fn(|_| Vec::new());
    fields[0] = scalar(sample);
    let mut instrument = if let Some(query) = query {
        query.instrument_id().clone()
    } else {
        let container = settings
            .layout()
            .batch_container
            .as_ref()
            .ok_or(OrderError::RecordLayout)?;
        [
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            container.as_str().into(),
        ]
    };
    if profile == OrderProfile::Cobas {
        let code = match order {
            Some(order) => map_specimen(order, settings.tables())?,
            None => match instrument[4].as_str() {
                "S1" => "1",
                "S2" => "2",
                "S5" => "5",
                _ => return Err(OrderError::SpecimenMismatch),
            },
        };
        // Table 16 note 2: O.16 is authoritative. The O.4 sample type agrees
        // with it, independently of the analyzer's provisional Q.3 type.
        instrument[4] = match code {
            "1" => "S1",
            "2" => "S2",
            "5" => "S5",
            _ => return Err(crate::OrderMappingError::SpecimenTypeUnrepresentable.into()),
        }
        .into();
        fields[13] = scalar(code);
    }
    fields[1] = vec![Vec::from(instrument)];
    fields[3] = scalar("R");
    if let Some(order) = order {
        fields[2] = tests(order, settings, limits, profile)?;
        fields[3] = scalar(
            map_priority(settings.tables().priorities(), order.priority)?
                .machine
                .as_str(),
        );
    }
    fields[9] = scalar(settings.layout().action_code.as_str());
    fields[23] = scalar(match (profile, order.is_some()) {
        (OrderProfile::Elecsys, true) => "Q",
        (OrderProfile::Elecsys, false) => "Z",
        (OrderProfile::Cobas, _) => "O",
    });
    OrderRecord::new(Vec::from(fields), limits.record_bytes(), limits.bytes()).map_err(order_error)
}

fn scalar(value: &str) -> Vec<Vec<String>> {
    vec![vec![value.into()]]
}

fn tests(
    order: &AppOrder,
    settings: &AstmOrders,
    limits: &SyntaxLimits,
    profile: OrderProfile,
) -> Result<Vec<Vec<String>>, OrderError> {
    if order.tests.is_empty() || order.tests.len() > limits.repetitions().min(18) {
        return Err(OrderError::TestCount);
    }
    let mut seen = BTreeSet::new();
    order
        .tests
        .iter()
        .map(|requested| {
            let code = map_test(settings.tables().test_system(), &requested.code)?;
            if code.len() > 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
                return Err(OrderError::StructuredCode);
            }
            if !seen.insert(test_identity(profile, code)) {
                return Err(OrderError::DuplicateTest);
            }
            let ratio = requested.dilution.map(std::num::NonZeroU16::get);
            let dilution = dilution(profile, ratio)?;
            Ok(vec![
                String::new(),
                String::new(),
                String::new(),
                code.into(),
                dilution.into(),
            ])
        })
        .collect()
}

fn test_identity(profile: OrderProfile, code: &str) -> &str {
    // Table 16: the final Elecsys digit is generation. Sending another
    // generation of the same application requests the same test again.
    let identity = match profile {
        OrderProfile::Elecsys => code.get(..code.len().saturating_sub(1)).unwrap_or(code),
        OrderProfile::Cobas => code,
    };
    identity.trim_start_matches('0')
}

fn dilution(profile: OrderProfile, ratio: Option<u16>) -> Result<&'static str, OrderError> {
    let wire = match (profile, ratio) {
        (_, None) => "",
        (OrderProfile::Elecsys, Some(1)) => "0",
        (OrderProfile::Elecsys, Some(2)) | (OrderProfile::Cobas, Some(1)) => "1",
        (OrderProfile::Elecsys, Some(5)) | (OrderProfile::Cobas, Some(2)) => "2",
        (OrderProfile::Elecsys, Some(10)) => "3",
        (OrderProfile::Elecsys, Some(20)) | (OrderProfile::Cobas, Some(5)) => "5",
        (OrderProfile::Elecsys, Some(50)) => "7",
        (OrderProfile::Elecsys, Some(100)) => "9",
        (OrderProfile::Cobas, Some(10)) => "10",
        (OrderProfile::Cobas, Some(20)) => "20",
        (OrderProfile::Cobas, Some(50)) => "50",
        (OrderProfile::Cobas, Some(100)) => "100",
        _ => return Err(OrderError::UnsupportedDilution),
    };
    Ok(wire)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Roche cobas e 411 HIM v2.3 Table 16, note 6 and application generation.
    #[test]
    fn duplicate_identity_ignores_elecsys_generation_and_numeric_padding() {
        assert_eq!(
            test_identity(OrderProfile::Elecsys, "072"),
            test_identity(OrderProfile::Elecsys, "73")
        );
        assert_ne!(
            test_identity(OrderProfile::Elecsys, "72"),
            test_identity(OrderProfile::Elecsys, "82")
        );
        assert_eq!(
            test_identity(OrderProfile::Cobas, "072"),
            test_identity(OrderProfile::Cobas, "72")
        );
        assert_ne!(
            test_identity(OrderProfile::Cobas, "72"),
            test_identity(OrderProfile::Cobas, "73")
        );
    }

    // Roche cobas e 411 HIM v2.3 Table 16 and Table 17.
    #[test]
    fn dilution_table_uses_only_documented_ratios() {
        for (ratio, elecsys, cobas) in [
            (None, "", ""),
            (Some(1), "0", "1"),
            (Some(2), "1", "2"),
            (Some(5), "2", "5"),
            (Some(10), "3", "10"),
            (Some(20), "5", "20"),
            (Some(50), "7", "50"),
            (Some(100), "9", "100"),
        ] {
            assert_eq!(dilution(OrderProfile::Elecsys, ratio).unwrap(), elecsys);
            assert_eq!(dilution(OrderProfile::Cobas, ratio).unwrap(), cobas);
        }
        for mode in [OrderProfile::Elecsys, OrderProfile::Cobas] {
            assert!(matches!(
                dilution(mode, Some(3)),
                Err(OrderError::UnsupportedDilution)
            ));
        }
    }
}
