//! Tests vendor outgoing orders and configured bounds.
//! These assertions do not claim vendor conformance; no vendor evidence level changes.
use analyte_config::{Draft, Validated};
use analyte_fixtures::ConfigEdit as _;
use analyte_mapping::{AppOrder, AppOrderPriority, OrderMappingError};
use analyte_model::{Identifier, NamespacedId, SampleId};
type TestResult = Result<(), Box<dyn std::error::Error>>;
const SECTION: &str = "\n[orders]\nsend_ahead=true\npatient_namespace='patients'\ntests=[{app={namespace='demo',value='DEMO'},machine_components=['SYN']}]\npriorities=[{app='routine',machine='R'}]\n";
fn config(definition: &str) -> Result<Validated, analyte_config::Error> {
    Draft::load(
        ("machine", definition),
        (
            "site",
            include_str!("../../../core/config/examples/site.toml"),
        ),
        ("local", "schema_version=1"),
    )?
    .validate()
}
fn vidas() -> String {
    format!(
        "{}{SECTION}",
        include_str!("../../../core/config/examples/vidas_literal.toml")
            .rewrite("download = false", "download = true")
    )
}

fn autolyser() -> String {
    format!(
        "{}\n[orders]\nsend_ahead=true\npatient_namespace='patients'\nclone=false\ntests=[{{app={{namespace='demo',value='DEMO'}},machine_components=['SYN ']}}]\npriorities=[{{app='routine',machine='T'}}]\nspecimen_types=[{{app={{namespace='types',value='SERUM'}},machine='S'}}]\n",
        include_str!("../../../core/config/examples/autolyser.toml")
            .rewrite("download = false", "download = true")
    )
}

fn positioned_order() -> Result<AppOrder, analyte_model::TextError> {
    let mut input = order()?;
    input.position = Some(analyte_model::SamplePosition {
        kind: analyte_model::CarrierKind::Rack,
        carrier: None,
        slot: Identifier::new("03")?,
    });
    input.specimen_type = Some(NamespacedId {
        namespace: Identifier::new("types")?,
        value: Identifier::new("SERUM")?,
    });
    Ok(input)
}

#[test]
fn autolyser_rev18_p186_rule5_explicit_patient_code_and_download_context_are_preserved()
-> TestResult {
    let cfg = config(&autolyser())?;
    let work = map_autolyser(&positioned_order()?, cfg.effective().stack())?;
    let wire = work.encode();
    assert_eq!(wire.get(..15), Some(b"000Patient     ".as_slice()));
    assert_eq!(wire.get(15..22), Some(b"TSN0301".as_slice()));
    assert_eq!(wire.get(22..26), Some(b"SYN ".as_slice()));
    for (length, accepted) in [(14, true), (15, true), (16, false)] {
        let mut input = positioned_order()?;
        input.patient.as_mut().ok_or("patient")?.value = Identifier::new(&"X".repeat(length))?;
        let mapped = map_autolyser(&input, cfg.effective().stack());
        if accepted {
            assert!(mapped.is_ok());
        } else {
            assert!(matches!(
                mapped,
                Err(analyte_mapping::autolyser::OrderError::Shared(
                    OrderMappingError::PatientIdTooLong
                ))
            ));
        }
    }
    Ok(())
}

#[test]
fn rule7_autolyser_download_refuses_missing_context_and_bounds_before_output() -> TestResult {
    let cfg = config(&autolyser())?;
    // The manual permits 00 (automatic positioning); its current refusal by
    // Analyte is reported separately rather than asserted as a wire rule here.
    for slot in ["100", "ABC", "003"] {
        let mut input = positioned_order()?;
        input.position.as_mut().ok_or("position")?.slot = Identifier::new(slot)?;
        assert!(matches!(
            map_autolyser(&input, cfg.effective().stack()),
            Err(analyte_mapping::autolyser::OrderError::InvalidCupPosition)
        ));
    }
    let mut one_digit = positioned_order()?;
    one_digit.position.as_mut().ok_or("position")?.slot = Identifier::new("3")?;
    let one_digit = map_autolyser(&one_digit, cfg.effective().stack())?;
    assert_eq!(one_digit.encode().get(18..20), Some(b"03".as_slice()));
    let urine = config(&autolyser().rewrite("machine='S'", "machine='U'"))?;
    let work = map_autolyser(&positioned_order()?, urine.effective().stack())?;
    assert_eq!(work.encode().get(16), Some(&b'U'));
    let repeated = config(&autolyser().rewrite("repetitions = 128", "repetitions = 2"))?;
    for (count, accepted) in [(1, true), (2, true), (3, false)] {
        let mut input = positioned_order()?;
        input.tests = vec![input.tests.first().ok_or("test")?.clone(); count];
        assert_eq!(
            map_autolyser(&input, repeated.effective().stack()).is_ok(),
            accepted
        );
    }
    for (count, accepted) in [(0, false), (1, true), (99, true), (100, false)] {
        let mut input = positioned_order()?;
        input.tests = vec![input.tests.first().ok_or("test")?.clone(); count];
        let mapped = map_autolyser(&input, cfg.effective().stack());
        assert_eq!(mapped.is_ok(), accepted, "count {count}");
        if !accepted {
            assert!(matches!(
                mapped,
                Err(analyte_mapping::autolyser::OrderError::TestCount)
            ));
        }
    }
    let input = positioned_order()?;
    let work = map_autolyser(&input, cfg.effective().stack())?;
    let size = work.encode().len();
    for (bound, accepted) in [(size - 1, false), (size, true), (size + 1, true)] {
        let cfg = config(&autolyser().rewrite("bytes = 1048576", &format!("bytes = {bound}")))?;
        assert_eq!(
            map_autolyser(&input, cfg.effective().stack()).is_ok(),
            accepted
        );
    }
    Ok(())
}
fn order() -> Result<AppOrder, analyte_model::TextError> {
    Ok(AppOrder {
        sample: SampleId::new("000Ab")?,
        tests: vec![NamespacedId {
            namespace: Identifier::new("demo")?,
            value: Identifier::new("DEMO")?,
        }],
        priority: AppOrderPriority::Routine,
        position: None,
        patient: Some(NamespacedId {
            namespace: Identifier::new("patients")?,
            value: Identifier::new("000Patient")?,
        }),
        specimen_type: None,
    })
}

#[test]
fn adr0004_rule5_vendor_download_keeps_exact_explicit_patient_and_sample() -> TestResult {
    let cfg = config(&vidas())?;
    let input = order()?;
    let requests = map_vidas(&input, cfg.effective().stack())?;
    assert_eq!(requests.len(), 1);
    let request = requests.first().ok_or("request")?;
    assert_eq!(request.get("pi"), Some("000Patient"));
    assert_eq!(request.get("ci"), Some("000Ab"));
    assert_eq!(request.get("rt"), Some("SYN"));
    let mut missing = order()?;
    missing.patient = None;
    assert!(matches!(
        map_vidas(&missing, cfg.effective().stack()),
        Err(analyte_mapping::vidas::OrderError::Shared(
            OrderMappingError::MissingPatient
        ))
    ));
    let mut wrong = order()?;
    wrong.patient.as_mut().ok_or("patient")?.namespace = Identifier::new("other")?;
    assert!(matches!(
        map_vidas(&wrong, cfg.effective().stack()),
        Err(analyte_mapping::vidas::OrderError::Shared(
            OrderMappingError::PatientNamespace
        ))
    ));
    Ok(())
}

#[test]
fn rule7_vendor_download_reports_exact_host_identity_bounds() -> TestResult {
    let cfg = config(&vidas())?;
    for (length, accepted) in [(16, true), (17, true), (18, false)] {
        let mut input = order()?;
        input.patient.as_mut().ok_or("patient")?.value = Identifier::new(&"X".repeat(length))?;
        let result = map_vidas(&input, cfg.effective().stack());
        if accepted {
            assert!(result.is_ok());
        } else {
            assert!(matches!(
                result,
                Err(analyte_mapping::vidas::OrderError::Shared(
                    OrderMappingError::PatientIdTooLong
                ))
            ));
        }
    }
    for (length, accepted) in [(11, true), (12, true), (13, false)] {
        let mut input = order()?;
        input.sample = SampleId::new(&"X".repeat(length))?;
        let result = map_vidas(&input, cfg.effective().stack());
        if accepted {
            assert!(result.is_ok());
        } else {
            assert!(matches!(
                result,
                Err(analyte_mapping::vidas::OrderError::SampleIdTooLong)
            ));
        }
    }
    Ok(())
}

#[test]
fn rule7_vendor_download_count_and_aggregate_bounds_are_independent() -> TestResult {
    let cfg = config(&vidas())?;
    for (count, accepted) in [(99, true), (100, true), (101, false)] {
        let mut input = order()?;
        input.tests = vec![input.tests.first().ok_or("test")?.clone(); count];
        assert_eq!(
            map_vidas(&input, cfg.effective().stack()).is_ok(),
            accepted,
            "count={count}"
        );
    }
    for (key, old) in [("records", 4096), ("repetitions", 128)] {
        let cfg = config(&vidas().rewrite(&format!("{key} = {old}"), &format!("{key} = 2")))?;
        for (count, accepted) in [(0, false), (1, true), (2, true), (3, false)] {
            let mut input = order()?;
            input.tests = vec![input.tests.first().ok_or("test")?.clone(); count];
            assert_eq!(
                map_vidas(&input, cfg.effective().stack()).is_ok(),
                accepted,
                "{key}:{count}"
            );
        }
    }
    let input = order()?;
    let requests = map_vidas(&input, config(&vidas())?.effective().stack())?;
    let size = requests.first().ok_or("request")?.text().len();
    for (key, old) in [("bytes", 1048576), ("limit", 1920)] {
        for (bound, accepted) in [(size - 1, false), (size, true), (size + 1, true)] {
            let cfg =
                config(&vidas().rewrite(&format!("{key} = {old}"), &format!("{key} = {bound}")))?;
            assert_eq!(
                map_vidas(&input, cfg.effective().stack()).is_ok(),
                accepted,
                "{key}:{bound}"
            );
        }
    }
    Ok(())
}

fn map_autolyser(
    order: &AppOrder,
    stack: &analyte_config::Stack,
) -> Result<analyte_autolyser::syntax::Worklist, analyte_mapping::autolyser::OrderError> {
    let analyte_config::Stack::Autolyser(settings) = stack else {
        return Err(OrderMappingError::Disabled.into());
    };
    analyte_mapping::autolyser::map_order(
        order,
        settings,
        settings.orders().ok_or(OrderMappingError::Disabled)?,
    )
}

fn map_vidas(
    order: &AppOrder,
    stack: &analyte_config::Stack,
) -> Result<Vec<analyte_vidas::syntax::Message>, analyte_mapping::vidas::OrderError> {
    let analyte_config::Stack::Vidas(settings) = stack else {
        return Err(OrderMappingError::Disabled.into());
    };
    analyte_mapping::vidas::map_order(
        order,
        settings,
        settings.orders().ok_or(OrderMappingError::Disabled)?,
    )
}
