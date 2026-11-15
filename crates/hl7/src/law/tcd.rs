//! Test details from TF-2b C.13; no observation value is calculated here.
use super::control::{component_error, excluded_fields, field, part, primitive, scalar};
use super::order::{limit, test_code};
use super::patient::{coded, excluded, extras};
use super::{ControlDirection, Error, Profile, content, raw};
use crate::Segment;

/// Whether a reported observation value already accounts for dilution.
/// This comes from an explicit analyzer contract, never from a numeric guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultAdjustment {
    /// The original observation already includes the dilution adjustment.
    AlreadyApplied,
    /// The original observation has not been adjusted; this crate still never adjusts it.
    NotApplied,
    /// No trusted contract establishes the value's meaning; reconciliation is required.
    Unknown,
}

/// The meaning of an original TCD value, distinct from its numeric spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DilutionMeaning {
    /// App instruction for this run, including analyzer-selected dilution/concentration.
    RequestedForRun,
    /// App instruction for a possible analyzer-initiated rerun.
    RequestedForRerun,
    /// Endogenous content supplied for the analyzer, never used to compute a result here.
    PrediluentContent,
    /// Analyzer-reported factor plus the host's explicit observation-adjustment knowledge.
    ReportedForRun {
        /// Whether the original observation value includes the factor.
        adjustment: ResultAdjustment,
    },
}

/// An original encoded dilution value with explicit meaning; no conversion is performed.
#[derive(Debug, Clone, Copy)]
pub struct Dilution<'a> {
    raw: &'a [u8],
    meaning: DilutionMeaning,
}

impl<'a> Dilution<'a> {
    /// Returns every original byte, including zeros, decimal spelling and ignored components.
    #[must_use]
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }

    /// Returns request/report meaning and explicit adjustment knowledge.
    #[must_use]
    pub fn meaning(self) -> DilutionMeaning {
        self.meaning
    }

    /// Flags unknown reported adjustment so the host can park it for reconciliation.
    #[must_use]
    pub fn requires_reconciliation(self) -> bool {
        match self.meaning {
            DilutionMeaning::RequestedForRun
            | DilutionMeaning::RequestedForRerun
            | DilutionMeaning::PrediluentContent => false,
            DilutionMeaning::ReportedForRun { adjustment } => match adjustment {
                ResultAdjustment::Unknown => true,
                ResultAdjustment::AlreadyApplied | ResultAdjustment::NotApplied => false,
            },
        }
    }
}

/// A host-asserted test/type/factor association for a configured analyzer.
/// It proves only what the host configured, not a clinical interpretation of the code.
#[derive(Debug, Clone, Copy)]
pub struct DilutionBinding<'a> {
    test: &'a [u8],
    kind: &'a [u8],
    factor: &'a [u8],
}

impl<'a> DilutionBinding<'a> {
    /// Validates a trusted test/type/factor association without deriving a factor from a code.
    /// # Errors
    /// Returns a TCD location when any supplied encoded value is invalid.
    pub fn new(test: &'a [u8], kind: &'a [u8], factor: &'a [u8]) -> Result<Self, Error> {
        test_code(test, "TCD", 1, ControlDirection::Send)?;
        coded(kind, "TCD", 11, b"", ControlDirection::Send)?;
        factor_value(factor, 2, ControlDirection::Send)?;
        Ok(Self { test, kind, factor })
    }
}

/// A validated TCD segment, retaining all original fields for custody and reprocessing.
#[derive(Debug, Clone, Copy)]
pub struct TestDetail<'a> {
    segment: Segment<'a>,
    direction: ControlDirection,
    adjustment: ResultAdjustment,
    dilution_supported: bool,
}

impl<'a> TestDetail<'a> {
    /// Validates a TCD using explicit option/binding context.
    /// # Errors
    /// Returns a located profile error or an unresolved type/factor definition.
    pub fn new(
        segment: Segment<'a>,
        profile: Profile<'_>,
        direction: ControlDirection,
    ) -> Result<Self, Error> {
        if segment.id() != b"TCD" {
            return Err(content("TCD", 0));
        }
        validate(segment, profile, direction)?;
        Ok(Self::from_validated(segment, profile, direction))
    }

    /// Borrows a segment already checked at the owning message boundary.
    pub(super) fn from_validated(
        segment: Segment<'a>,
        profile: Profile<'_>,
        direction: ControlDirection,
    ) -> Self {
        Self {
            segment,
            direction,
            adjustment: profile.result_adjustment,
            dilution_supported: profile.dilution,
        }
    }

    /// Returns the source segment without dropping unsupported or absent positions.
    #[must_use]
    pub fn segment(self) -> Segment<'a> {
        self.segment
    }

    /// Returns the original current-run factor and its explicit requested/reported meaning.
    #[must_use]
    pub fn auto_dilution(self) -> Option<Dilution<'a>> {
        let value = raw(self.segment, 2);
        if value.is_empty() || !self.dilution_supported {
            return None;
        }
        let meaning = match self.direction {
            ControlDirection::Send => DilutionMeaning::RequestedForRun,
            ControlDirection::Receive => DilutionMeaning::ReportedForRun {
                adjustment: self.adjustment,
            },
        };
        Some(Dilution {
            raw: value,
            meaning,
        })
    }

    /// Returns the original supported dilution-type code, without interpreting it numerically.
    #[must_use]
    pub fn dilution_type(self) -> Option<&'a [u8]> {
        let value = raw(self.segment, 11);
        (self.dilution_supported && !value.is_empty()).then_some(value)
    }

    /// Records adjustment knowledge for numeric or type-only reported dilution.
    /// Requests and ignored dilution content have no reported adjustment meaning.
    #[must_use]
    pub fn result_adjustment(self) -> Option<ResultAdjustment> {
        let reported = self.direction == ControlDirection::Receive
            && (self.auto_dilution().is_some() || self.dilution_type().is_some());
        reported.then_some(self.adjustment)
    }

    /// Flags reported dilution whose adjustment meaning still needs host reconciliation.
    #[must_use]
    pub fn requires_reconciliation(self) -> bool {
        match self.result_adjustment() {
            Some(ResultAdjustment::Unknown) => true,
            Some(ResultAdjustment::AlreadyApplied | ResultAdjustment::NotApplied) | None => false,
        }
    }

    /// Returns a manager's original rerun instruction; analyzer X content remains only raw.
    #[must_use]
    pub fn rerun_dilution(self) -> Option<Dilution<'a>> {
        self.request_value(3, DilutionMeaning::RequestedForRerun)
    }

    /// Returns original manager-supplied diluent content without applying it to a value.
    #[must_use]
    pub fn prediluent_content(self) -> Option<Dilution<'a>> {
        self.request_value(5, DilutionMeaning::PrediluentContent)
    }

    fn request_value(self, number: usize, meaning: DilutionMeaning) -> Option<Dilution<'a>> {
        let value = raw(self.segment, number);
        (self.direction == ControlDirection::Send && !value.is_empty()).then_some(Dilution {
            raw: value,
            meaning,
        })
    }
}

fn enabled(
    segment: Segment<'_>,
    number: usize,
    supported: bool,
    required: bool,
    direction: ControlDirection,
) -> Result<bool, Error> {
    let value = raw(segment, number);
    if !supported {
        excluded(value, direction, content("TCD", number))?;
        return Ok(false);
    }
    field(segment, "TCD", number, required, false)?;
    Ok(!value.is_empty())
}

fn factor_value(value: &[u8], number: usize, direction: ControlDirection) -> Result<(), Error> {
    excluded(part(value, 1), direction, component_error("TCD", number, 1))?;
    if part(value, 2) != b"1" {
        return Err(component_error("TCD", number, 2));
    }
    let suffix = part(value, 3);
    match suffix {
        b":" => {
            let denominator = part(value, 4);
            primitive(denominator, true, 15, component_error("TCD", number, 4))?;
            if !super::specimen::numeric(denominator)
                || denominator.first() == Some(&b'-')
                || !denominator.iter().any(|b| matches!(b, b'1'..=b'9'))
            {
                return Err(component_error("TCD", number, 4));
            }
        }
        b"+" | b"-" if number == 2 && direction == ControlDirection::Send => {
            excluded(part(value, 4), direction, component_error("TCD", number, 4))?;
        }
        _ => return Err(component_error("TCD", number, 3)),
    }
    extras(value, b'^', &[2, 3, 4], direction, |position| {
        component_error("TCD", number, position)
    })?;
    limit(value, "TCD", number, 20, direction)
}

fn consumption(value: &[u8], direction: ControlDirection) -> Result<(), Error> {
    let quantity = part(value, 1);
    primitive(quantity, true, 16, component_error("TCD", 9, 1))?;
    if !super::specimen::numeric(quantity) {
        return Err(component_error("TCD", 9, 1));
    }
    let units = part(value, 2);
    let mut parts = units.split(|b| *b == b'&');
    for (position, required, maximum) in [(1, true, 20), (2, false, 199), (3, true, 4)] {
        let field = parts.next().unwrap_or_default();
        let error = Error::Subcomponent {
            segment: "TCD",
            field: 9,
            component: 2,
            subcomponent: position,
        };
        primitive(field, required, maximum, error)?;
        if position == 3 && field != b"UCUM" {
            return Err(Error::Subcomponent {
                segment: "TCD",
                field: 9,
                component: 2,
                subcomponent: 3,
            });
        }
    }
    extras(units, b'&', &[1, 2, 3], direction, |subcomponent| {
        Error::Subcomponent {
            segment: "TCD",
            field: 9,
            component: 2,
            subcomponent,
        }
    })?;
    extras(value, b'^', &[1, 2], direction, |position| {
        component_error("TCD", 9, position)
    })?;
    if direction == ControlDirection::Send && super::control::characters(units) > 225 {
        return Err(component_error("TCD", 9, 2));
    }
    limit(value, "TCD", 9, 242, direction)
}

fn bound_pair(segment: Segment<'_>, profile: Profile<'_>) -> Result<(), Error> {
    let test = raw(segment, 1);
    let kind = raw(segment, 11);
    let factor = raw(segment, 2);
    if factor.is_empty() || kind.is_empty() {
        return Ok(());
    }
    let same_kind = |binding: &&DilutionBinding<'_>| {
        part(binding.test, 1) == part(test, 1)
            && part(binding.test, 3) == part(test, 3)
            && part(binding.kind, 1) == part(kind, 1)
            && part(binding.kind, 3) == part(kind, 3)
    };
    let same_factor = |binding: &DilutionBinding<'_>| {
        [2, 3, 4]
            .into_iter()
            .all(|position| part(binding.factor, position) == part(factor, position))
    };
    let mut definitions = profile.dilution_bindings.iter().filter(same_kind);
    let Some(first) = definitions.next() else {
        return Err(Error::DilutionDefinition {
            segment: "TCD",
            field: 11,
            component: 1,
        });
    };
    if !same_factor(first) || definitions.any(|binding| !same_factor(binding)) {
        return Err(Error::Inconsistent {
            segment: "TCD",
            field: 11,
        });
    }
    Ok(())
}

/// Checks every C.13 field and resolves option predicates without calculating results.
pub(super) fn validate(
    segment: Segment<'_>,
    profile: Profile<'_>,
    direction: ControlDirection,
) -> Result<(), Error> {
    excluded_fields(segment, "TCD", &[1, 2, 3, 5, 6, 7, 8, 9, 10, 11], direction)?;
    field(segment, "TCD", 1, true, false)?;
    test_code(raw(segment, 1), "TCD", 1, direction)?;
    for number in [2, 3, 5] {
        let supported = profile.dilution && (number == 2 || direction == ControlDirection::Send);
        if enabled(segment, number, supported, false, direction)? {
            factor_value(raw(segment, number), number, direction)?;
        }
    }
    let manager = direction == ControlDirection::Send;
    for number in [6, 7] {
        if enabled(
            segment,
            number,
            manager && profile.repeat_control,
            true,
            direction,
        )? {
            scalar(segment, "TCD", number, true, 1)?;
            if !matches!(raw(segment, number), b"Y" | b"N") {
                return Err(content("TCD", number));
            }
        }
    }
    if enabled(
        segment,
        8,
        manager && profile.manager_repeat,
        true,
        direction,
    )? {
        let value = raw(segment, 8);
        for (position, maximum) in [(1, 20), (2, 199), (3, 7)] {
            primitive(
                part(value, position),
                true,
                maximum,
                component_error("TCD", 8, position),
            )?;
        }
        extras(value, b'^', &[1, 2, 3], direction, |position| {
            component_error("TCD", 8, position)
        })?;
        limit(value, "TCD", 8, 250, direction)?;
        if !matches!(part(value, 1), b"O" | b"R" | b"D" | b"F") {
            return Err(component_error("TCD", 8, 1));
        }
        if part(value, 3) != b"HL70389" {
            return Err(component_error("TCD", 8, 3));
        }
    }
    if enabled(segment, 9, manager, false, direction)? {
        consumption(raw(segment, 9), direction)?;
    }
    if enabled(segment, 10, profile.pooling_analyzer, true, direction)? {
        scalar(segment, "TCD", 10, true, 16)?;
        let value = raw(segment, 10);
        if !value.iter().all(u8::is_ascii_digit) || value.iter().all(|b| *b == b'0') {
            return Err(content("TCD", 10));
        }
    }
    if enabled(segment, 11, profile.dilution, false, direction)? {
        coded(raw(segment, 11), "TCD", 11, b"", direction)?;
    }
    if profile.dilution_required && raw(segment, 2).is_empty() && raw(segment, 11).is_empty() {
        return Err(content("TCD", 2));
    }
    if profile.dilution {
        bound_pair(segment, profile)?;
    }
    Ok(())
}
