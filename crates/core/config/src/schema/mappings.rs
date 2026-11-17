//! Holds raw TOML field selectors and mapping tables.
use super::{
    Classification, Deserialize, DilutionSource, Group, Identifier, SampleSource, Serialize,
    ValueKind,
};

/// Group-relative one-based field selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    /// Owning group.
    pub group: Group,
    /// Record or segment name.
    pub record: Identifier,
    /// One-based field.
    pub field: u16,
    /// One-based component.
    pub component: u16,
    /// One-based repetition.
    pub repetition: u16,
}

/// Explicit decomposition of one ASTM test-ID component; positions are one-based.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentParts {
    /// Component selected from R.3.
    pub component: u16,
    /// Single non-protocol separator.
    pub separator: char,
    /// Exact number of parts, including empty parts.
    pub count: u16,
    /// Part used for test lookup.
    pub code: u16,
    /// Optional part retained as dilution evidence.
    pub dilution: Option<u16>,
    /// Source evidence for whether the dilution was applied.
    pub applied: Option<DilutionSource>,
}

/// Sample identity copied without normalization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SampleSelection {
    /// Source location.
    pub field: Field,
    /// Acquisition evidence.
    pub source: SampleSource,
}

/// Exact code translation with assigning systems.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestMapping {
    /// Expected interpretation; omission preserves permissive mapping.
    #[serde(default)]
    pub value_kind: ValueKind,
    /// Code as received.
    pub machine_code: Identifier,
    /// Source assigning system.
    pub machine_system: Identifier,
    /// Destination code.
    pub app_code: Identifier,
    /// Destination assigning system.
    pub app_system: Identifier,
}

/// Explicit spelling translation; no implicit numeric conversion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitMapping {
    /// Exact source spelling.
    pub original: Identifier,
    /// Explicit destination UCUM spelling.
    pub ucum: Identifier,
}

/// Exact field equality classifies evidence only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QcRule {
    /// Source location.
    pub field: Field,
    /// Exact value.
    pub value: Identifier,
    /// Classification to carry.
    pub classification: Classification,
}

/// Assigning namespace, explicitly fixed or selected from the source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum OrderNamespace {
    /// A site-declared assigning system.
    Fixed {
        /// Exact assigning system.
        fixed: Identifier,
    },
    /// An assigning system reported by the source.
    Field {
        /// Group-relative source location.
        field: Field,
    },
}

/// Explicit absence of a sub-ID concept.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NoSubId {
    /// Deliberately use an empty sub-ID.
    #[serde(rename = "none")]
    None,
}

/// Sub-ID evidence or a deliberate declaration that it does not exist.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SubId {
    /// Source-provided sub-ID; missing values remain unresolved.
    Field(Field),
    /// The protocol profile has no sub-ID concept.
    None(NoSubId),
}

/// Explicit identity evidence; source status never assigns a revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityEvidence {
    /// Order identity within its assigning namespace.
    pub order_id: Field,
    /// Explicit assigning namespace.
    pub order_namespace: OrderNamespace,
    /// Source sub-ID or deliberate absence.
    pub sub_id: SubId,
    /// Ordered run evidence; empty means reruns cannot be distinguished.
    pub run: Vec<Field>,
    /// Exact source codes declaring a correction of the same observation.
    pub correction_status_codes: Vec<Identifier>,
}

/// Bounded declarative mapping tables without expressions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mappings {
    /// Optional ASTM test-code component decomposition.
    pub component_parts: Option<ComponentParts>,
    /// Sample selection.
    pub sample: SampleSelection,
    /// Test code table.
    pub tests: Vec<TestMapping>,
    /// Unit spelling table.
    pub units: Vec<UnitMapping>,
    /// Classification rules.
    pub qc: Vec<QcRule>,
    /// Identity evidence.
    pub identity: IdentityEvidence,
}
