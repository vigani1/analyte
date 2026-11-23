//! Tests vendor outgoing orders and configured bounds.
//! These assertions do not claim vendor conformance; no vendor evidence level changes.
use analyte_config::{Draft, Validated};
use analyte_fixtures::ConfigEdit as _;
use analyte_mapping::{AppOrder, AppOrderPriority, OrderMappingError};
use analyte_model::{Identifier, NamespacedId, SampleId, SourceText};
type TestResult = Result<(), Box<dyn std::error::Error>>;
const SECTION: &str = "\n[orders]\nsend_ahead=true\npatient_namespace='patients'\npriorities=[{app='routine',machine='R'}]\n";
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
        "{}\n[orders]\nsend_ahead=true\npatient_namespace='patients'\nclone=false\npriorities=[{{app='routine',machine='T'}}]\nspecimen_system='types'\n",
        include_str!("../../../core/config/examples/autolyser.toml")
            .rewrite("download = false", "download = true")
    )
}

fn positioned_order() -> Result<AppOrder, analyte_model::TextError> {
    let mut input = order()?;
    // The work list's four-byte analysis code, padding written by the lab app.
    input.tests = vec![test("synthetic", "SYN ")?];
    input.position = Some(analyte_model::SamplePosition {
        kind: analyte_model::CarrierKind::Rack,
        carrier: None,
        slot: Identifier::new("03")?,
    });
    input.specimen_type = Some(NamespacedId {
        namespace: Identifier::new("types")?,
        value: Identifier::new("S")?,
    });
    Ok(input)
}

// Autolyser manual Rev18 p. 186; Safety rule 5 (ADR 0003)
#[test]
fn autolyser_download_keeps_patient_and_context() -> TestResult {
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

// Safety rule 7 (ADR 0003)
#[test]
fn autolyser_download_refuses_missing_context_or_overflow() -> TestResult {
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
    // ADR 0030: the medium is the order's own S or U, in the machine's system.
    let mut urine = positioned_order()?;
    urine.specimen_type.as_mut().ok_or("specimen")?.value = Identifier::new("U")?;
    let work = map_autolyser(&urine, cfg.effective().stack())?;
    assert_eq!(work.encode().get(16), Some(&b'U'));
    for medium in ["s", "SERUM", "X"] {
        let mut input = positioned_order()?;
        input.specimen_type.as_mut().ok_or("specimen")?.value = Identifier::new(medium)?;
        assert!(matches!(
            map_autolyser(&input, cfg.effective().stack()),
            Err(analyte_mapping::autolyser::OrderError::Shared(
                OrderMappingError::SpecimenTypeUnrepresentable
            ))
        ));
    }
    let mut foreign = positioned_order()?;
    foreign.specimen_type.as_mut().ok_or("specimen")?.namespace = Identifier::new("lab-app")?;
    assert!(matches!(
        map_autolyser(&foreign, cfg.effective().stack()),
        Err(analyte_mapping::autolyser::OrderError::Shared(
            OrderMappingError::ForeignSpecimenType(_)
        ))
    ));
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
fn test(namespace: &str, value: &str) -> Result<NamespacedId, analyte_model::TextError> {
    Ok(NamespacedId {
        namespace: Identifier::new(namespace)?,
        value: Identifier::new(value)?,
    })
}

fn order() -> Result<AppOrder, analyte_model::TextError> {
    Ok(AppOrder {
        patient_name: None,
        patient_birth_date: None,
        patient_sex_code: None,
        sample_origin: None,
        test_dilutions: Vec::new(),
        sample: SampleId::new("000Ab")?,
        tests: vec![test("synthetic", "SYN")?],
        test_names: vec![None],
        priority: AppOrderPriority::Routine,
        position: None,
        patient: Some(NamespacedId {
            namespace: Identifier::new("patients")?,
            value: Identifier::new("000Patient")?,
        }),
        specimen_type: None,
    })
}

// ADR 0004; Safety rule 5 (ADR 0003)
#[test]
fn vidas_download_keeps_exact_patient_and_sample() -> TestResult {
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

// Safety rule 7 (ADR 0003)
#[test]
fn vidas_download_reports_exact_identity_bounds() -> TestResult {
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

// Safety rule 7 (ADR 0003)
#[test]
fn vidas_download_count_and_size_bounds_are_independent() -> TestResult {
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

// VIDAS BCI Link rev A §4; ADR 0029: the order's assay code goes out as
// written, case and leading zeros kept, only in the machine's code system and
// only when it fits the 1 to 4 byte field.
#[test]
fn vidas_assay_codes_are_checked_when_ordered() -> TestResult {
    let cfg = config(&vidas())?;
    for code in ["0aB", "A", "ABCD"] {
        let mut input = order()?;
        input.tests = vec![test("synthetic", code)?];
        let requests = map_vidas(&input, cfg.effective().stack())?;
        assert_eq!(requests.first().ok_or("request")?.get("rt"), Some(code));
    }
    for code in ["ABCDE", "A|B"] {
        let mut input = order()?;
        input.tests = vec![test("synthetic", code)?];
        assert!(
            matches!(
                map_vidas(&input, cfg.effective().stack()),
                Err(analyte_mapping::vidas::OrderError::AssayCode)
            ),
            "{code}"
        );
    }
    let mut foreign = order()?;
    foreign.tests = vec![test("lab-app", "SYN")?];
    assert!(matches!(
        map_vidas(&foreign, cfg.effective().stack()),
        Err(analyte_mapping::vidas::OrderError::Shared(
            OrderMappingError::ForeignTest(_)
        ))
    ));
    Ok(())
}

// Autolyser manual Rev18 chapter 23; ADR 0029: the analysis code is exactly
// four bytes; Analyte never pads or trims what the lab app wrote.
#[test]
fn autolyser_analysis_code_is_exactly_four_bytes() -> TestResult {
    let cfg = config(&autolyser())?;
    for code in ["SYN", "SYN  "] {
        let mut input = positioned_order()?;
        input.tests = vec![test("synthetic", code)?];
        assert!(
            matches!(
                map_autolyser(&input, cfg.effective().stack()),
                Err(analyte_mapping::autolyser::OrderError::AnalysisCode)
            ),
            "{code:?}"
        );
    }
    let mut foreign = positioned_order()?;
    foreign.tests = vec![test("Synthetic", "SYN ")?];
    assert!(matches!(
        map_autolyser(&foreign, cfg.effective().stack()),
        Err(analyte_mapping::autolyser::OrderError::Shared(
            OrderMappingError::ForeignTest(_)
        ))
    ));
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

const VIDAS_RECIPE: &str = "[codes.vidas_identity]\nsample_namespace='samples'\nevidence={manual='BCI Link 161150-486 rev C', section='p. 4-6'}\n";

// BCI Link 161150-486 rev C, p. 4-6: use both source completion fields.
#[test]
fn vidas_completion_evidence_distinguishes_measurements() -> TestResult {
    let definition = include_str!("../../../core/config/examples/vidas_literal.toml");
    let cfg = config(&format!("{definition}\n{VIDAS_RECIPE}"))?;
    let analyte_config::Stack::Vidas(machine) = cfg.effective().stack() else {
        return Err("expected VIDAS stack".into());
    };
    let map = |time| -> Result<analyte_model::IdentityStatus, Box<dyn std::error::Error>> {
        let message = analyte_vidas::syntax::Message::parse(
            &format!("mtrsl|pi|si|ci000a|rtSYN|tt{time}|td2026/11/15|qlPOS|qn1|"),
            analyte_vidas::syntax::SampleLimit::Twelve,
        )?;
        let output = analyte_mapping::vidas::map(
            machine,
            &message,
            analyte_mapping::Input {
                device: None,
                receipt: analyte_model::ReceiptId::new(),
                received_at: jiff::Timestamp::UNIX_EPOCH,
                revision: analyte_model::Revision(0),
            },
        )?;
        Ok(output
            .results
            .into_iter()
            .next()
            .ok_or("missing result")?
            .content
            .identity)
    };
    let first = map("10:30")?;
    let analyte_model::IdentityStatus::Resolved(identity) = &first else {
        return Err("expected completion identity".into());
    };
    assert_eq!(identity.observation.order.value.as_str(), "000a");
    assert_eq!(first, map("10:30")?);
    assert_ne!(first, map("10:31")?);
    assert!(matches!(
        map("")?,
        analyte_model::IdentityStatus::Unresolved(_)
    ));

    let incompatible = format!(
        "{}\n{VIDAS_RECIPE}",
        include_str!("../../../core/config/examples/autolyser.toml")
    );
    assert!(
        matches!(config(&incompatible), Err(analyte_config::Error::Invalid { key, .. })
        if key == "codes.vidas_identity")
    );
    Ok(())
}

// BCI Link rev C pp. 4-5 and B-7; VIDAS PC Manual F ch. 10.
#[test]
fn vidas_control_labels_keep_composite_and_enforce_qc_width() -> TestResult {
    use analyte_model::{QcClass, TimePrecision};

    let config = config(include_str!(
        "../../../core/config/examples/vidas_literal.toml"
    ))?;
    let analyte_config::Stack::Vidas(machine) = config.effective().stack() else {
        return Err("expected VIDAS stack".into());
    };
    let result = analyte_mapping::vidas::map(
        machine,
        &vidas_control_message("KitX 260203-2", "conthnor")?,
        result_input(),
    )?;
    let kit = &result.results.first().ok_or("kit result")?.content;
    let part = kit.parts.as_slice().first().ok_or("part")?;
    assert!(kit.sample.is_none());
    assert_eq!(part.qc_class, QcClass::Control);
    let details = part.details.as_ref().ok_or("result details")?;
    assert_eq!(
        details.instrument_id.as_ref().map(SourceText::as_str),
        Some("Box 01")
    );
    assert_eq!(
        details.instrument_serial.as_ref().map(SourceText::as_str),
        Some("000 a")
    );
    assert_eq!(
        details.technologist.as_ref().map(SourceText::as_str),
        Some("Tech 7")
    );
    assert_eq!(
        details.specimen_label.as_ref().map(SourceText::as_str),
        Some("KitX 260203-2")
    );
    assert_eq!(
        details.control_name.as_ref().map(SourceText::as_str),
        Some("KitX")
    );
    assert_eq!(
        details.strip_lot.as_ref().map(SourceText::as_str),
        Some("260203-2")
    );
    let completion = part.times.as_slice().first().ok_or("completion")?;
    assert_eq!(completion.role.as_str(), "completion");
    assert_eq!(completion.time.original.as_str(), "2026/02/03 09:05");
    assert_eq!(completion.time.precision, TimePrecision::Minute);
    assert!(completion.time.offset.is_none());
    for (id, flag) in [("QC6", ""), ("Qc0000000001", "conthwst")] {
        let result = analyte_mapping::vidas::map(
            machine,
            &vidas_control_message(id, flag)?,
            result_input(),
        )?;
        let qc = &result.results.first().ok_or("QC result")?.content;
        let part = qc.parts.as_slice().first().ok_or("part")?;
        assert!(qc.sample.is_none());
        assert_eq!(part.qc_class, QcClass::Control);
        let details = part.details.as_ref().ok_or("result details")?;
        assert_eq!(
            details.control_name.as_ref().map(SourceText::as_str),
            Some(id)
        );
        assert!(details.strip_lot.is_none());
    }
    Ok(())
}

// BCI Link rev C pp. 4-5 and B-7; VIDAS PC Manual F pp. 6-8 and 10-8.
#[test]
fn vidas_rejects_ambiguous_control_labels() -> TestResult {
    assert!(matches!(
        vidas_control_message("C2 260203-0", "")?.subject(),
        analyte_vidas::syntax::Subject::KitControl { name, lot }
            if name.as_str() == "C2" && lot.as_str() == "260203-0"
    ));
    assert!(
        analyte_vidas::syntax::Message::parse(
            vidas_control_message("KitX 260203-2", "conthnor")?.text(),
            analyte_vidas::syntax::SampleLimit::Twelve,
        )
        .is_err()
    );
    assert!(vidas_control_message("C2 260203-1", "").is_err());
    assert!(vidas_control_message("C3", "").is_err());
    assert!(vidas_control_message("QC1", "conthnor").is_err());
    assert!(vidas_control_message("Kit 260203-2", "").is_err());
    assert!(vidas_control_message("Qc00000000012", "conthwst").is_err());
    Ok(())
}

fn vidas_control_message(
    ci: &str,
    flag: &str,
) -> Result<analyte_vidas::syntax::Message, analyte_vidas::syntax::Error> {
    use analyte_vidas::syntax::{ControlLabels, Message, SampleLimit};
    Message::parse_with_control_labels(
        &format!(
            "mtrsl|pi|si|ci{ci}|rtSYN|tt09:05|td2026/02/03|ql|qn1|nc{flag}|idBox 01|sn000 a|m4Tech 7|"
        ),
        SampleLimit::Twelve,
        ControlLabels::AppendixB,
    )
}

// BCI Link rev C p. 4-9, Figure 4-3. The two separate doses use the same short assay code.
#[test]
fn vidas_cmva_doses_remain_separate_even_with_completion_recipe() -> TestResult {
    let definition = include_str!("../../../core/config/examples/vidas_literal.toml");
    let config = config(&format!("{definition}\n{VIDAS_RECIPE}"))?;
    let analyte_config::Stack::Vidas(machine) = config.effective().stack() else {
        return Err("expected VIDAS stack".into());
    };
    let mut receipts = Vec::new();
    for dose in ["< 2", "41"] {
        let message = analyte_vidas::syntax::Message::parse(
            &format!("mtrsl|pi|si|ci000a|rtCMVA|tt09:05|td2026/02/03|ql|qn{dose}|"),
            analyte_vidas::syntax::SampleLimit::Twelve,
        )?;
        let output = analyte_mapping::vidas::map(machine, &message, result_input())?;
        let result = &output.results.first().ok_or("dose")?.content;
        let part = result.parts.as_slice().first().ok_or("part")?;
        assert!(result.requires_parking());
        assert_eq!(
            part.values
                .as_slice()
                .first()
                .ok_or("value")?
                .original
                .as_str(),
            dose
        );
        assert!(part.units.is_none());
        assert!(part.dilution.is_none());
        receipts.push(result.receipt);
    }
    assert_ne!(receipts.first(), receipts.last());
    Ok(())
}

// Autolyser Rev18 pp. 175, 177–178. The association preserves the documented
// padding, while an empty patient code or the analyzer's fallback stays unresolved.
#[test]
fn autolyser_keeps_report_code_without_inventing_sample_or_run() -> TestResult {
    let definition = include_str!("../../../core/config/examples/autolyser.toml");
    let config = config(definition)?;
    let analyte_config::Stack::Autolyser(machine) = config.effective().stack() else {
        return Err("expected Autolyser stack".into());
    };
    for (code, id) in [
        ("00aBc          ", Some("00aBc          ")),
        ("               ", None),
        ("{0}            ", None),
    ] {
        let raw = format!("{code}TS001SYN   <0.1 ");
        let report = analyte_autolyser::syntax::Report::parse(raw.as_bytes(), false)?;
        let output = analyte_mapping::autolyser::map(machine, &report, result_input())?;
        let result = &output.results.first().ok_or("result")?.content;
        assert_eq!(result.sample.as_ref().map(|s| s.id.as_str()), id);
        assert!(
            result
                .sample
                .as_ref()
                .and_then(|s| s.position.as_ref())
                .is_none()
        );
        assert!(result.requires_parking());
        let part = result.parts.as_slice().first().ok_or("part")?;
        assert_eq!(
            part.values
                .as_slice()
                .first()
                .ok_or("value")?
                .original
                .as_str(),
            "  <0.1 "
        );
    }
    Ok(())
}

// Autolyser Rev18 §23.2 pp. 177–178. Follow the field widths, including the
// original level and lot, in place of the inconsistent printed example.
#[test]
fn autolyser_projects_controls_layout_and_leaves_normal_placeholders_raw() -> TestResult {
    use analyte_model::QcClass;
    let config = config(include_str!("../../../core/config/examples/autolyser.toml"))?;
    let analyte_config::Stack::Autolyser(machine) = config.effective().stack() else {
        return Err("expected Autolyser stack".into());
    };
    let lot = " 00lot              ";
    for (control, expected_class) in [
        (format!("YU007{lot}"), QcClass::Control),
        (format!("NK001{:20}", ""), QcClass::Unknown),
    ] {
        let raw = format!("00aBc          RU{control}001SYN   <0.1 ");
        let report = analyte_autolyser::syntax::Report::parse(raw.as_bytes(), true)?;
        let output = analyte_mapping::autolyser::map(machine, &report, result_input())?;
        let result = &output.results.first().ok_or("result")?.content;
        let part = result.parts.as_slice().first().ok_or("part")?;
        assert_eq!(part.qc_class, expected_class);
        assert!(result.requires_parking());
        let details = part.details.as_ref().ok_or("result details")?;
        assert_eq!(
            details.report_list.as_ref().map(SourceText::as_str),
            Some("R")
        );
        assert_eq!(
            details.specimen_descriptor.as_ref().map(SourceText::as_str),
            Some("U")
        );
        let control = expected_class == QcClass::Control;
        if control {
            assert!(result.sample.is_none());
        }
        assert_eq!(
            details.control_knowledge.as_ref().map(SourceText::as_str),
            control.then_some("U")
        );
        assert_eq!(
            details.control_level.as_ref().map(SourceText::as_str),
            control.then_some("007")
        );
        assert_eq!(
            details.control_lot.as_ref().map(SourceText::as_str),
            control.then_some(lot)
        );
        assert_eq!(report.raw, raw.as_bytes());
    }
    Ok(())
}

fn result_input() -> analyte_mapping::Input<'static> {
    analyte_mapping::Input {
        device: None,
        receipt: analyte_model::ReceiptId::new(),
        received_at: jiff::Timestamp::UNIX_EPOCH,
        revision: analyte_model::Revision(0),
    }
}
