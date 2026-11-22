//! Tests the published schema, OpenAPI document and catalog against the committed files, the contract vectors and the machines folder.
#![forbid(unsafe_code)]
use analyte_contract::document::{OrderState, ProcessingMode, QcClass, ResultStatus};
use serde_json::{Value, json};
use std::path::Path;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn repository_file(path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(analyte_publish::repository().join(path))?;
    Ok(serde_json::from_str(&text)?)
}

fn vector(name: &str) -> Result<Value, Box<dyn std::error::Error>> {
    repository_file(&format!("docs/contract/vectors/{name}"))
}

/// A validator for the definition `name` of the committed contract schema,
/// checking `format` too.
fn body(name: &str) -> Result<jsonschema::Validator, Box<dyn std::error::Error>> {
    let mut schema = repository_file("docs/contract/contract-v1.schema.json")?;
    let root = schema.as_object_mut().ok_or("schema is not an object")?;
    root.insert("$ref".into(), format!("#/$defs/{name}").into());
    Ok(jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema)?)
}

fn errors(validator: &jsonschema::Validator, document: &Value) -> Vec<String> {
    validator
        .iter_errors(document)
        .map(|error| format!("{} at {}", error, error.instance_path()))
        .collect()
}

// ADR 0032: the committed files are exactly what the generator writes from the
// contract types and the machines folder; `cargo run -p analyte-publish`
// rewrites them.
#[test]
fn committed_files_match_the_generator() -> TestResult {
    let repository = analyte_publish::repository();
    let files = analyte_publish::files(&repository.join("machines"))?;
    let names: Vec<_> = files.iter().map(|file| file.name).collect();
    assert_eq!(names, analyte_publish::NAMES);
    for file in files {
        let path = repository.join(analyte_publish::FOLDER).join(file.name);
        let committed = std::fs::read_to_string(&path)?;
        assert!(
            committed == file.text,
            "{} is stale; run `cargo run -p analyte-publish` and review the diff",
            path.display()
        );
    }
    Ok(())
}

// Contract v1 (docs/contract/v1.md): every valid published document validates
// against the schema, so a lab app checking with it accepts what Analyte sends.
#[test]
fn valid_vectors_match_the_schema() -> TestResult {
    let delivery = body("Delivery")?;
    for name in [
        "delivery.json",
        "decoded-text.json",
        "missing-sample.json",
        "multi-part-coded.json",
        "source-path-occurrence.json",
        "unknown-fields.json",
        "unresolved-identity.json",
        "unsupported-formatting.json",
        "withdrawn.json",
    ] {
        assert_eq!(
            errors(&delivery, &vector(name)?),
            Vec::<String>::new(),
            "{name}"
        );
    }
    let signed = vector("signature.json")?;
    let signed: Value = serde_json::from_str(signed["body"].as_str().ok_or("body")?)?;
    assert!(delivery.is_valid(&signed), "signature.json body");
    assert_eq!(
        errors(&body("OrderEvent")?, &vector("event.json")?),
        Vec::<String>::new()
    );

    let status = body("StatusBody")?;
    let error = body("ErrorBody")?;
    for name in ["accepted.json", "duplicate.json"] {
        assert!(status.is_valid(&vector(name)?), "{name}");
    }
    // An answer Analyte marks done is a status body; a 200 it parks is not.
    for case in vector("responses.json")?.as_array().ok_or("responses")? {
        let (schema, valid) = match (case["status"].as_u64(), case["expected"].as_str()) {
            (Some(200), Some("done")) => (&status, true),
            (Some(200), _) => (&status, false),
            _ => (&error, true),
        };
        assert_eq!(
            schema.is_valid(&case["body"]),
            valid,
            "responses.json {} {}",
            case["status"],
            case["expected"]
        );
    }
    for step in vector("idempotency.json")?.as_array().ok_or("steps")? {
        if !step["expected_body"].is_null() {
            assert!(status.is_valid(&step["expected_body"]), "{}", step["step"]);
        }
    }

    let order = body("Order")?;
    let seed = vector("seed-orders.json")?;
    let mut orders = vec![vector("order.json")?, vector("order-context.json")?];
    orders.extend(seed.as_array().ok_or("seed")?.iter().cloned());
    for step in vector("order-versions.json")?.as_array().ok_or("steps")? {
        orders.push(step["offer"].clone());
    }
    for document in &orders {
        assert_eq!(
            errors(&order, document),
            Vec::<String>::new(),
            "{}",
            document["id"]
        );
    }
    let page =
        json!({ "contract_version": 1, "orders": seed, "next_cursor": "c1", "complete": true });
    assert_eq!(errors(&body("OrdersPage")?, &page), Vec::<String>::new());
    Ok(())
}

/// Refusals JSON Schema cannot express: each compares two values in one
/// document. The Rust validators and the reference receiver refuse them.
const BEYOND_SCHEMA: [&str; 2] = ["receipt-mismatch", "unit-mapping-mismatch"];

// Contract v1 "POST /results": the schema refuses every invalid vector it can
// express; the two it cannot are named, so a new gap fails this test.
#[test]
fn invalid_vectors_fail_the_schema_where_it_can_express_them() -> TestResult {
    let delivery = body("Delivery")?;
    assert!(!delivery.is_valid(&vector("invalid-version.json")?));
    let entries = vector("invalid-documents.json")?;
    let entries = entries.as_array().ok_or("entries")?;
    assert!(entries.len() >= 9);
    for entry in entries {
        let name = entry["name"].as_str().ok_or("name")?;
        let valid = delivery.is_valid(&entry["document"]);
        assert_eq!(
            valid,
            BEYOND_SCHEMA.contains(&name),
            "{name}: schema {}",
            if valid { "accepts it" } else { "refuses it" }
        );
    }
    // Envelope text has no NUL; identifiers have no control characters (v1
    // "HTTP and authentication"), checked by the patterns.
    let mut nul = vector("delivery.json")?;
    nul["source"]["analyzer"] = json!("synthetic\u{0}1");
    assert!(!delivery.is_valid(&nul));
    let mut control = vector("delivery.json")?;
    control["receipt"]["config_version"] = json!("1\u{85}");
    assert!(!delivery.is_valid(&control));
    control["receipt"]["config_version"] = json!("1\u{e9}");
    assert!(delivery.is_valid(&control));
    // A tombstone may have no tests; a live order may not (v1 "GET /orders").
    let order = body("Order")?;
    let mut live = vector("order.json")?;
    live["tests"] = json!([]);
    assert!(!order.is_valid(&live));
    live["tombstone"] = json!(true);
    assert!(order.is_valid(&live));
    Ok(())
}

// ADR 0032: enums are closed and spelled exactly as the Rust types write them.
#[test]
fn enums_are_closed_and_match_the_rust_types() -> TestResult {
    let schema = repository_file("docs/contract/contract-v1.schema.json")?;
    let constants = |name: &str| -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let definition = &schema["$defs"][name];
        let variants = definition["oneOf"]
            .as_array()
            .or_else(|| definition["enum"].as_array())
            .ok_or(format!("{name} has no variants"))?;
        variants
            .iter()
            .map(|variant| {
                variant
                    .get("const")
                    .or(Some(variant))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| format!("{name}: {variant}").into())
            })
            .collect()
    };
    assert_eq!(
        constants("OrderState")?,
        spelled(&[
            OrderState::Requested,
            OrderState::Sent,
            OrderState::Accepted,
            OrderState::Done,
            OrderState::Rejected,
            OrderState::CancelRequested,
            OrderState::CancelConfirmed,
            OrderState::Uncertain,
        ])
    );
    assert_eq!(
        constants("ProcessingMode")?,
        spelled(&[
            ProcessingMode::Unknown,
            ProcessingMode::Production,
            ProcessingMode::Training,
            ProcessingMode::Debug,
            ProcessingMode::QualityControl,
        ])
    );
    assert_eq!(
        constants("QcClass")?,
        spelled(&[
            QcClass::Patient,
            QcClass::Control,
            QcClass::Calibration,
            QcClass::Unknown,
        ])
    );
    assert_eq!(
        constants("ResultStatus")?,
        spelled(&[
            ResultStatus::Pending,
            ResultStatus::Preliminary,
            ResultStatus::Final,
            ResultStatus::Corrected,
            ResultStatus::Withdrawn,
            ResultStatus::NotPerformed,
            ResultStatus::Unknown,
        ])
    );
    assert_eq!(
        constants("Priority")?,
        spelled(&[
            analyte_contract::Priority::Routine,
            analyte_contract::Priority::Urgent,
            analyte_contract::Priority::Stat,
        ])
    );
    assert_eq!(constants("DeliveryStatus")?, ["accepted", "duplicate"]);
    Ok(())
}

/// Each value as serde spells it.
fn spelled<T: serde::Serialize>(values: &[T]) -> Vec<String> {
    values
        .iter()
        .filter_map(|value| serde_json::to_value(value).ok())
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect()
}

// ADR 0032: the OpenAPI document covers the three calls, with bearer
// authentication and the Standard Webhooks headers, and each body reference
// names a definition of the schema file.
#[test]
fn openapi_covers_the_three_calls_and_its_references_resolve() -> TestResult {
    let openapi = repository_file("docs/contract/openapi.json")?;
    let schema = repository_file("docs/contract/contract-v1.schema.json")?;
    assert_eq!(openapi["openapi"], "3.1.1");
    let paths: Vec<_> = openapi["paths"]
        .as_object()
        .ok_or("paths")?
        .iter()
        .flat_map(|(path, item)| {
            item.as_object()
                .into_iter()
                .flat_map(move |item| item.keys().map(move |method| format!("{method} {path}")))
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        ["get /orders", "post /order-events", "post /results"]
    );
    assert_eq!(
        openapi["components"]["securitySchemes"]["bearer"]["scheme"],
        "bearer"
    );
    let headers: Vec<_> = ["WebhookId", "WebhookTimestamp", "WebhookSignature"]
        .iter()
        .map(|name| openapi["components"]["parameters"][name]["name"].clone())
        .collect();
    assert_eq!(
        headers,
        ["webhook-id", "webhook-timestamp", "webhook-signature"]
    );
    for post in ["/results", "/order-events"] {
        assert_eq!(
            openapi["paths"][post]["post"]["parameters"]
                .as_array()
                .map(Vec::len),
            Some(3),
            "{post}"
        );
    }
    let mut references = Vec::new();
    collect_references(&openapi, &mut references);
    assert!(references.len() >= 8);
    for reference in references {
        if let Some(local) = reference.strip_prefix("#/") {
            let pointer = format!("/{local}");
            assert!(openapi.pointer(&pointer).is_some(), "{reference}");
        } else {
            let definition = reference
                .strip_prefix("contract-v1.schema.json#/$defs/")
                .ok_or(format!("unexpected reference {reference}"))?;
            assert!(schema["$defs"].get(definition).is_some(), "{reference}");
        }
    }
    for name in analyte_contract::schema::BODIES {
        assert!(schema["$defs"].get(name).is_some(), "{name}");
    }
    Ok(())
}

fn collect_references(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(reference)) = map.get("$ref") {
                found.push(reference.clone());
            }
            map.values()
                .for_each(|value| collect_references(value, found));
        }
        Value::Array(items) => items
            .iter()
            .for_each(|value| collect_references(value, found)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

// ADR 0032: one catalog entry per model folder, keyed by the model id that
// source.model carries; a blocked definition is listed as not ready, with no
// protocol or code systems guessed for it.
#[test]
fn catalog_lists_every_model_folder() -> TestResult {
    let root = analyte_publish::repository().join("machines");
    let catalog = repository_file("docs/contract/machines.json")?;
    let machines = catalog["machines"].as_array().ok_or("machines")?;
    let mut folders = Vec::new();
    for vendor in std::fs::read_dir(&root)? {
        let vendor = vendor?;
        if vendor.file_type()?.is_dir() {
            for model in std::fs::read_dir(vendor.path())? {
                let model = model?;
                if model.file_type()?.is_dir() {
                    folders.push(format!(
                        "{}/{}",
                        vendor.file_name().to_string_lossy(),
                        model.file_name().to_string_lossy()
                    ));
                }
            }
        }
    }
    folders.sort();
    let models: Vec<_> = machines
        .iter()
        .filter_map(|m| m["model"].as_str())
        .collect();
    assert_eq!(models, folders);
    let entry = |model: &str| machines.iter().find(|m| m["model"] == model).cloned();
    assert_eq!(
        entry("example/synthetic-astm"),
        Some(json!({
            "model": "example/synthetic-astm",
            "vendor": "example",
            "name": "Synthetic ASTM example",
            "ready": true,
            "evidence_level": "documented",
            "protocol": "astm",
            "test_system": "synthetic",
            "specimen_system": null,
        }))
    );
    assert_eq!(
        entry("roche/cobas-e411"),
        Some(json!({
            "model": "roche/cobas-e411",
            "vendor": "roche",
            "name": "Roche cobas e 411",
            "ready": false,
            "evidence_level": "documented",
            "protocol": null,
            "test_system": null,
            "specimen_system": null,
        }))
    );
    let schema = repository_file("docs/contract/machines.schema.json")?;
    let validator = jsonschema::draft202012::new(&schema)?;
    assert_eq!(errors(&validator, &catalog), Vec::<String>::new());
    Ok(())
}

// ADR 0032 and machines/README.md: a model folder without its README title or
// evidence level, or with a name that is not a model key, stops the catalog
// with the folder named instead of being guessed.
#[test]
fn catalog_refuses_a_folder_it_cannot_describe() -> TestResult {
    let temp = tempfile::tempdir()?;
    let folder = temp.path().join("acme").join("model-1");
    std::fs::create_dir_all(&folder)?;
    let readme = folder.join("README.md");
    let write = |text: &str| std::fs::write(&readme, text);
    write("## Machine\n\nLevel: documented\n")?;
    assert!(matches!(
        analyte_publish::files(temp.path()),
        Err(analyte_publish::Error::Catalog(
            analyte_publish::CatalogError::Title(_)
        ))
    ));
    write("# Acme Model 1\n\n## Evidence\n")?;
    assert!(matches!(
        analyte_publish::files(temp.path()),
        Err(analyte_publish::Error::Catalog(
            analyte_publish::CatalogError::Level(_)
        ))
    ));
    write("# Acme Model 1\n\nLevel: documented\n")?;
    let files = analyte_publish::files(temp.path())?;
    let catalog: Value = serde_json::from_str(&files.get(2).ok_or("catalog")?.text)?;
    assert_eq!(catalog["machines"][0]["model"], "acme/model-1");
    assert_eq!(catalog["machines"][0]["ready"], false);
    std::fs::create_dir_all(temp.path().join("acme_corp").join("x"))?;
    assert!(matches!(
        analyte_publish::files(temp.path()),
        Err(analyte_publish::Error::Catalog(
            analyte_publish::CatalogError::Key { .. }
        ))
    ));
    assert!(Path::new(&readme).is_file());
    Ok(())
}
