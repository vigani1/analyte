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

/// Explicit decomposition of the ASTM R.3 component named by `test_component`;
/// positions are one-based.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentParts {
    /// Single non-protocol separator.
    pub separator: char,
    /// Exact number of parts, including empty parts.
    pub count: u16,
    /// Part that holds the machine's test code.
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

/// LIS02 section 5.6.1 component of the universal test ID (R.3) that holds
/// the machine's own test code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AstmTestComponent {
    /// Component 1, the universal test ID.
    Universal,
    /// Component 4, the manufacturer's or local code.
    Manufacturer,
}

impl AstmTestComponent {
    /// The one-based R.3 component number.
    #[must_use]
    pub const fn number(self) -> u16 {
        match self {
            Self::Universal => 1,
            Self::Manufacturer => 4,
        }
    }
}

/// What the machine's message says for one of its own test codes: the kind
/// of value it sends. It never names or translates the test (ADR 0029).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValueExpectation {
    /// The machine's test code, exactly as sent.
    pub code: Identifier,
    /// The code system the message names for it; absent means `test_system`.
    pub system: Option<Identifier>,
    /// Required interpretation: `numeric` or `coded`.
    pub kind: ValueKind,
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
    /// The machine's own test code system: the namespace of every test code
    /// whose message does not name one, and the only one its orders accept.
    pub test_system: Identifier,
    /// ASTM only: the R.3 component that holds the machine's test code.
    pub test_component: Option<AstmTestComponent>,
    /// Optional decomposition of the ASTM test component.
    pub component_parts: Option<ComponentParts>,
    /// Sample selection.
    pub sample: SampleSelection,
    /// Kinds of value the machine sends for some of its codes; absent means none.
    #[serde(default)]
    pub value_kinds: Vec<ValueExpectation>,
    /// Unit spelling table.
    pub units: Vec<UnitMapping>,
    /// Classification rules.
    pub qc: Vec<QcRule>,
    /// Identity evidence.
    pub identity: IdentityEvidence,
}
