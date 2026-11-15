//! Outbound mapping never searches or reverses the incoming result table.
mod display;
pub use display::map_display;

use analyte_config::Validated;
use analyte_model::{NamespacedId, SampleId, SamplePosition};

/// Explicit app priority, translated only by the outbound priority table.
#[derive(Debug, Clone, Copy)]
pub enum AppOrderPriority {
    /// Routine work.
    Routine,
    /// Urgent work.
    Urgent,
    /// Immediate work.
    Stat,
}

impl AppOrderPriority {
    fn code(self) -> &'static str {
        match self {
            Self::Routine => "routine",
            Self::Urgent => "urgent",
            Self::Stat => "stat",
        }
    }
}

/// Host-adapted app order; identities and clinical strings are copied exactly.
pub struct AppOrder {
    /// Exact sample identifier, including case and leading zeros.
    pub sample: SampleId,
    /// Requested tests, each with its app assigning system.
    pub tests: Vec<NamespacedId>,
    /// Explicit priority from the app.
    pub priority: AppOrderPriority,
    /// Position supplied by the app, never invented by the bridge.
    pub position: Option<SamplePosition>,
    /// Optional exact patient identifier; the namespace is checked before encoding.
    pub patient: Option<NamespacedId>,
    /// Specimen type evidence, never substituted for origin metadata.
    pub specimen_type: Option<NamespacedId>,
}

/// Complete mapped commands; mapping has no transmission or reservation side effects.
pub enum MachineOrder {
    /// A standard O record, including CR.
    Astm(Vec<u8>),
    /// One validated request per assay, in the app's order.
    Vidas(Vec<analyte_vidas::syntax::Message>),
    /// A complete validated host-initiated insertion.
    Autolyser(analyte_autolyser::records::Worklist),
}

impl MachineOrder {
    /// Borrows an ASTM record, refusing another protocol's payload.
    /// # Errors
    /// Returns Disabled when this is not an ASTM command.
    pub fn record(&self) -> Result<&[u8], OrderMappingError> {
        match self {
            Self::Astm(record) => Ok(record),
            Self::Vidas(_) | Self::Autolyser(_) => Err(OrderMappingError::Disabled),
        }
    }
}

/// Specific mapping refusal; diagnostic details belong in protected order events.
#[derive(Debug, thiserror::Error)]
pub enum OrderMappingError {
    /// This machine has no outbound capability.
    #[error("outbound orders are disabled")]
    Disabled,
    /// VIDAS requires an explicit patient identifier.
    #[error("missing-patient")]
    MissingPatient,
    /// Patient assigning system does not match the configured system.
    #[error("patient-namespace-mismatch")]
    PatientNamespace,
    /// The supplied specimen type has no wire mapping.
    #[error("unmapped-specimen-type")]
    UnmappedSpecimenType,
    /// The machine requires a specimen medium supplied by the app.
    #[error("missing-specimen-type")]
    MissingSpecimenType,
    /// No explicitly addressed cup can be encoded; zero is never invented.
    #[error("invalid-cup-position")]
    InvalidCupPosition,
    /// Autolyser requires an explicit addressed cup.
    #[error("missing-cup-position")]
    MissingCupPosition,
    /// An Autolyser field fails the documented fixed-width rules.
    #[error("Autolyser request is not representable: {0}")]
    Autolyser(#[from] analyte_autolyser::records::Error),
    /// A field exceeds its documented or installed ceiling.
    #[error("overlong-{0}")]
    Overlong(&'static str),
    /// Vendor syntax refuses the mapped request, without exposing patient content.
    #[error("VIDAS request is not representable: {0}")]
    Vidas(#[from] analyte_vidas::syntax::Error),
    /// No exact app-system and code match exists.
    #[error("unmapped test {0:?}")]
    UnmappedTest(NamespacedId),
    /// The app did not provide the required position.
    #[error("required specimen position was not supplied")]
    MissingPosition,
    /// Priority has no explicit translation.
    #[error("unmapped order priority")]
    UnmappedPriority,
    /// Input cannot be represented under the selected protocol repertoire and bounds.
    #[error("order cannot be represented within the configured protocol bounds")]
    Representation,
}

/// Maps every requested test before constructing complete protocol commands.
/// No wire output is returned for partial or ambiguous mappings.
/// # Errors
/// Refuses disabled capability, unmapped values, missing required context and wire bounds.
pub fn map_order(order: &AppOrder, config: &Validated) -> Result<MachineOrder, OrderMappingError> {
    use analyte_config::Stack;
    match config.effective().stack() {
        Stack::Astm(s) => astm(
            order,
            &s.syntax.limits,
            s.orders.as_ref().ok_or(OrderMappingError::Disabled)?,
        ),
        Stack::FileAstm(s) => astm(
            order,
            &s.syntax.limits,
            s.orders.as_ref().ok_or(OrderMappingError::Disabled)?,
        ),
        Stack::VidasLiteral(s) => vidas(
            order,
            s,
            s.orders.as_ref().ok_or(OrderMappingError::Disabled)?,
        ),
        Stack::Autolyser(s) => autolyser(
            order,
            s,
            s.orders.as_ref().ok_or(OrderMappingError::Disabled)?,
        ),
        Stack::Hl7(_) | Stack::FileHl7(_) | Stack::Law(_) | Stack::Poct(_) => {
            Err(OrderMappingError::Disabled)
        }
    }
}

fn astm(
    order: &AppOrder,
    limits: &analyte_config::SyntaxLimits,
    settings: &analyte_config::AstmOrders,
) -> Result<MachineOrder, OrderMappingError> {
    let astm = &settings.layout;
    let settings = &settings.tables;
    if order.tests.is_empty() || order.tests.len() > limits.repetitions {
        return Err(OrderMappingError::Representation);
    }
    let mut tests = Vec::new();
    for requested in &order.tests {
        let mapped = settings
            .tests
            .iter()
            .find(|t| t.app == *requested)
            .ok_or_else(|| OrderMappingError::UnmappedTest(requested.clone()))?;
        for part in &mapped.machine_components {
            wire_text(part)?;
        }
        tests.push(mapped.machine_components.join("^"));
    }
    let priority = settings
        .priorities
        .iter()
        .find(|p| p.app.as_str() == order.priority.code())
        .ok_or(OrderMappingError::UnmappedPriority)?;
    let mut fields: [String; 16] = std::array::from_fn(|_| String::new());
    fields[0] = "O".into();
    fields[1] = "1".into();
    set(
        &mut fields,
        &astm.specimen,
        order.sample.as_str(),
        limits.components,
    )?;
    if let Some(position) = &astm.position {
        let value = order
            .position
            .as_ref()
            .ok_or(OrderMappingError::MissingPosition)?;
        if value.slot.as_str().is_empty() {
            return Err(OrderMappingError::MissingPosition);
        }
        set(
            &mut fields,
            position,
            value.slot.as_str(),
            limits.components,
        )?;
    }
    if let Some(destination) = &astm.specimen_type {
        let code = specimen_type(order, settings)?;
        set(&mut fields, destination, code, limits.components)?;
    }
    let prefix = "^".repeat(usize::from(astm.test.component).saturating_sub(1));
    if settings.tests.iter().any(|t| {
        t.machine_components
            .len()
            .checked_add(prefix.len())
            .is_none_or(|n| n > limits.components)
    }) {
        return Err(OrderMappingError::Representation);
    }
    fields[4] = tests
        .iter()
        .map(|t| format!("{prefix}{t}"))
        .collect::<Vec<_>>()
        .join("\\");
    set(
        &mut fields,
        &astm.priority,
        priority.machine.as_str(),
        limits.components,
    )?;
    set(
        &mut fields,
        &astm.action,
        astm.action_code.as_str(),
        limits.components,
    )?;
    let last_field = if astm.specimen_type.is_some() { 16 } else { 12 };
    let mut record = fields
        .get(..last_field)
        .ok_or(OrderMappingError::Representation)?
        .join("|")
        .into_bytes();
    if record.len() > limits.record_bytes
        || record.len().checked_add(1).is_none_or(|n| n > limits.bytes)
    {
        return Err(OrderMappingError::Representation);
    }
    record.push(b'\r');
    Ok(MachineOrder::Astm(record))
}

fn set(
    fields: &mut [String],
    selected: &analyte_config::Field,
    value: &str,
    limit: usize,
) -> Result<(), OrderMappingError> {
    wire_text(value)?;
    let component = usize::from(selected.component);
    if component == 0 || component > limit {
        return Err(OrderMappingError::Representation);
    }
    let field = fields
        .get_mut(usize::from(selected.field).saturating_sub(1))
        .ok_or(OrderMappingError::Representation)?;
    *field = format!("{}{value}", "^".repeat(component.saturating_sub(1)));
    Ok(())
}

fn wire_text(value: &str) -> Result<(), OrderMappingError> {
    // This initial outbound repertoire refuses reserved characters rather than
    // changing clinical identifiers or guessing a vendor escape convention.
    if value.len() > 4096
        || !value.is_ascii()
        || value
            .bytes()
            .any(|b| !(32..127).contains(&b) || b"|\\^&".contains(&b))
    {
        return Err(OrderMappingError::Representation);
    }
    Ok(())
}

// One shared patient boundary for all outbound stacks with an unqualified field.
fn patient<'a>(
    order: &'a AppOrder,
    namespace: &analyte_model::Identifier,
) -> Result<&'a str, OrderMappingError> {
    let patient = order
        .patient
        .as_ref()
        .ok_or(OrderMappingError::MissingPatient)?;
    if namespace != &patient.namespace {
        return Err(OrderMappingError::PatientNamespace);
    }
    Ok(patient.value.as_str())
}

fn vidas(
    order: &AppOrder,
    machine: &analyte_config::VidasStack,
    settings: &analyte_config::VidasOrders,
) -> Result<MachineOrder, OrderMappingError> {
    let installed = &machine.policy;
    if !installed.download {
        return Err(OrderMappingError::Disabled);
    }
    let patient = patient(order, &settings.patient_namespace)?;
    if patient.len() > 17 {
        return Err(OrderMappingError::Overlong("patient-id"));
    }
    if order.sample.as_str().len() > usize::from(u8::from(installed.sample_limit)) {
        return Err(OrderMappingError::Overlong("sample-id"));
    }
    // BCI A section 4 has no specimen-type destination; so is origin and si is empty.
    if order.specimen_type.is_some() {
        return Err(OrderMappingError::UnmappedSpecimenType);
    }
    if order.tests.is_empty()
        || order.tests.len() > 100
        || order.tests.len() > machine.limits.records
        || order.tests.len() > machine.limits.repetitions
    {
        return Err(OrderMappingError::Representation);
    }
    if !settings
        .priorities
        .iter()
        .any(|p| p.app.as_str() == order.priority.code())
    {
        return Err(OrderMappingError::UnmappedPriority);
    }
    let limit = match installed.sample_limit {
        analyte_config::VidasSampleLimit::Twelve => analyte_vidas::syntax::SampleLimit::Twelve,
        analyte_config::VidasSampleLimit::Seventeen => {
            analyte_vidas::syntax::SampleLimit::Seventeen
        }
    };
    let mut requests = Vec::new();
    let mut total_bytes = 0usize;
    for test in &order.tests {
        let mapped = settings
            .tests
            .iter()
            .find(|t| t.app == *test)
            .ok_or_else(|| OrderMappingError::UnmappedTest(test.clone()))?;
        let [code] = mapped.machine_components.as_slice() else {
            return Err(OrderMappingError::Representation);
        };
        let request = analyte_vidas::syntax::Message::build(
            &[
                ("mt", "mpr"),
                ("pi", patient),
                ("si", ""),
                ("ci", order.sample.as_str()),
                ("rt", code),
            ],
            limit,
        )?;
        if request.text().len() > machine.frame_limit {
            return Err(OrderMappingError::Representation);
        }
        total_bytes = total_bytes
            .checked_add(request.text().len())
            .ok_or(OrderMappingError::Representation)?;
        if total_bytes > machine.limits.bytes {
            return Err(OrderMappingError::Representation);
        }
        requests.push(request);
    }
    Ok(MachineOrder::Vidas(requests))
}

fn autolyser(
    order: &AppOrder,
    machine: &analyte_config::AutolyserStack,
    settings: &analyte_config::AutolyserOrders,
) -> Result<MachineOrder, OrderMappingError> {
    use analyte_autolyser::records::{Field, List, PatientCode, Sample, Worklist};
    if !machine.policy.download {
        return Err(OrderMappingError::Disabled);
    }
    let patient = patient(order, &settings.patient_namespace)?;
    let clone = settings.clone;
    let settings = &settings.tables;
    if patient.len() > 15 {
        return Err(OrderMappingError::Overlong("patient-id"));
    }
    let mut padded = [b' '; 15];
    padded
        .get_mut(..patient.len())
        .ok_or(OrderMappingError::Overlong("patient-id"))?
        .copy_from_slice(patient.as_bytes());
    let patient = PatientCode::new(&padded)?;
    let sample = match specimen_type(order, settings)? {
        "S" => Sample::Serum,
        "U" => Sample::Urine,
        _ => return Err(OrderMappingError::UnmappedSpecimenType),
    };
    let priority = settings
        .priorities
        .iter()
        .find(|p| p.app.as_str() == order.priority.code())
        .ok_or(OrderMappingError::UnmappedPriority)?;
    let list = match &priority.machine {
        analyte_config::MachinePriority::Code(analyte_config::PriorityCode::T) => List::Routine,
        analyte_config::MachinePriority::Code(analyte_config::PriorityCode::R) => List::Stat,
        analyte_config::MachinePriority::Code(
            analyte_config::PriorityCode::S
            | analyte_config::PriorityCode::A
            | analyte_config::PriorityCode::P
            | analyte_config::PriorityCode::C
            | analyte_config::PriorityCode::Prn,
        )
        | analyte_config::MachinePriority::Custom(_) => {
            return Err(OrderMappingError::UnmappedPriority);
        }
    };
    let position = order
        .position
        .as_ref()
        .ok_or(OrderMappingError::MissingCupPosition)?;
    let slot = position.slot.as_str();
    if slot.is_empty() || slot.len() > 2 || !slot.bytes().all(|b| b.is_ascii_digit()) {
        return Err(OrderMappingError::InvalidCupPosition);
    }
    let position = slot
        .parse::<u8>()
        .map_err(|_| OrderMappingError::InvalidCupPosition)?;
    if !(1..=99).contains(&position) {
        return Err(OrderMappingError::InvalidCupPosition);
    }
    if order.tests.is_empty()
        || order.tests.len() > 99
        || order.tests.len() > machine.limits.repetitions
    {
        return Err(OrderMappingError::Representation);
    }
    let mut tests = Vec::new();
    for test in &order.tests {
        let mapped = settings
            .tests
            .iter()
            .find(|t| t.app == *test)
            .ok_or_else(|| OrderMappingError::UnmappedTest(test.clone()))?;
        let [code] = mapped.machine_components.as_slice() else {
            return Err(OrderMappingError::Representation);
        };
        tests.push(Field::<4>::new(code.as_bytes())?);
    }
    let worklist = Worklist::new(patient, list, sample, clone, position, tests)?;
    if worklist.encode().len() > machine.limits.bytes {
        return Err(OrderMappingError::Representation);
    }
    Ok(MachineOrder::Autolyser(worklist))
}

fn specimen_type<'a>(
    order: &AppOrder,
    settings: &'a analyte_config::OrderTables,
) -> Result<&'a str, OrderMappingError> {
    let requested = order
        .specimen_type
        .as_ref()
        .ok_or(OrderMappingError::MissingSpecimenType)?;
    settings
        .specimen_types
        .as_ref()
        .and_then(|types| types.iter().find(|t| t.app == *requested))
        .map(|mapping| mapping.machine.as_str())
        .ok_or(OrderMappingError::UnmappedSpecimenType)
}
