//! LAW field and component constraints, from TF-2b Appendix C and section 2.2.
use super::{Error, content, raw};
use crate::{Message, Segment};

/// Negotiated content options and externally supplied authority bindings.
/// The profile validates content; it never resolves identities by guessing.
#[derive(Debug, Clone, Copy)]
pub struct Profile<'a> {
    pub(super) mass_spectrometry: bool,
    pub(super) contributing_substances: bool,
    pub(super) dilution: bool,
    pub(super) dilution_required: bool,
    pub(super) manager_repeat: bool,
    pub(super) repeat_control: bool,
    pub(super) priority: bool,
    pub(super) priority_required: bool,
    pub(super) analyzer_reflex: bool,
    pub(super) external_results: bool,
    pub(super) pooling_analyzer: bool,
    pub(super) dilution_bindings: &'a [super::DilutionBinding<'a>],
    pub(super) result_adjustment: super::ResultAdjustment,
    pub(super) patient: bool,
    pub(super) specimen: bool,
    pub(super) container: bool,
    pub(super) pooling_outside: bool,
    pub(super) negative_query: bool,
    pub(super) results_message: bool,
    pub(super) authorities: &'a [super::AuthorityBinding<'a>],
}

impl<'a> Profile<'a> {
    /// Supports receiving enhanced content without inferring what the peer can send.
    /// Pooling outside the analyzer remains unselected unless explicitly enabled.
    #[must_use]
    pub fn receive() -> Self {
        Self {
            mass_spectrometry: true,
            contributing_substances: true,
            dilution: true,
            dilution_required: false,
            manager_repeat: false,
            repeat_control: false,
            priority: true,
            priority_required: false,
            analyzer_reflex: true,
            external_results: true,
            pooling_analyzer: false,
            dilution_bindings: &[],
            result_adjustment: super::ResultAdjustment::Unknown,
            patient: true,
            specimen: true,
            container: true,
            pooling_outside: false,
            negative_query: false,
            results_message: true,
            authorities: &[],
        }
    }

    /// Selects received ORL field predicates for independent segment validation.
    /// Whole-message validation derives this context from MSH-9 automatically.
    #[must_use]
    pub fn receive_work_response() -> Self {
        Self {
            results_message: false,
            ..Self::receive()
        }
    }

    /// Selects outgoing content from the connected peer's declared capabilities.
    #[must_use]
    pub fn for_peer(capabilities: super::Capabilities) -> Self {
        Self {
            dilution: capabilities.dilution,
            dilution_required: capabilities.dilution,
            manager_repeat: capabilities.manager_repeat,
            repeat_control: capabilities.repeat_control,
            priority: capabilities.priority,
            priority_required: capabilities.priority,
            patient: capabilities.patient,
            specimen: capabilities.specimen,
            container: capabilities.container,
            ..Self::receive()
        }
    }

    /// Supplies trusted namespace/universal-ID associations from the host's configuration.
    #[must_use]
    pub fn with_authorities(mut self, authorities: &'a [super::AuthorityBinding<'a>]) -> Self {
        self.authorities = authorities;
        self
    }

    /// Selects validation of the outside-analyzer pooling fields and predicates.
    /// This does not authorize pooling or add pooling to the work-order builder.
    #[must_use]
    pub fn with_pooling_outside(mut self, selected: bool) -> Self {
        self.pooling_outside = selected;
        self
    }

    /// Selects `LAW_DILUTIONS`; selection requires a factor or a type in each TCD.
    #[must_use]
    pub fn with_dilution(mut self, selected: bool) -> Self {
        self.dilution = selected;
        self.dilution_required = selected;
        self
    }

    /// Selects received mass-spectrometry probability; manager output always excludes it.
    #[must_use]
    pub fn with_mass_spectrometry(mut self, selected: bool) -> Self {
        self.mass_spectrometry = selected;
        self
    }

    /// Selects contributing inventory support; unsupported input remains available as raw bytes.
    #[must_use]
    pub fn with_contributing_substances(mut self, selected: bool) -> Self {
        self.contributing_substances = selected;
        self
    }

    /// Selects analyzer reflex parent/action support.
    #[must_use]
    pub fn with_analyzer_reflex(mut self, selected: bool) -> Self {
        self.analyzer_reflex = selected;
        self
    }

    /// Selects externally persisted result handling on analyzer messages.
    #[must_use]
    pub fn with_external_results(mut self, selected: bool) -> Self {
        self.external_results = selected;
        self
    }

    /// Selects priority segment support.
    #[must_use]
    pub fn with_priority(mut self, selected: bool) -> Self {
        self.priority = selected;
        self.priority_required = selected;
        self
    }

    /// Selects the required manager repeat-status and permission fields independently.
    #[must_use]
    pub fn with_repeat_options(mut self, status: bool, permissions: bool) -> Self {
        self.manager_repeat = status;
        self.repeat_control = permissions;
        self
    }

    /// Selects field validation of on-analyzer pooling; no pooling workflow is authorized.
    #[must_use]
    pub fn with_pooling_analyzer(mut self, selected: bool) -> Self {
        self.pooling_analyzer = selected;
        self
    }

    /// Supplies host-asserted test/type/factor definitions; an unknown pair is unresolved.
    #[must_use]
    pub fn with_dilution_bindings(mut self, bindings: &'a [super::DilutionBinding<'a>]) -> Self {
        self.dilution_bindings = bindings;
        self
    }

    /// Records trusted knowledge about whether reported values already include dilution.
    /// Unknown is preserved by default; this crate never adjusts an observation value.
    #[must_use]
    pub fn with_result_adjustment(mut self, adjustment: super::ResultAdjustment) -> Self {
        self.result_adjustment = adjustment;
        self
    }

    /// Validates all implemented segment profiles and SPM sequence numbers.
    /// Negative-query exceptions apply only to the restricted LAB-28 form.
    /// # Errors
    /// Returns a located failure; this is not a complete message-group validator.
    pub fn validate_message(
        mut self,
        message: &Message,
        direction: super::ControlDirection,
    ) -> Result<(), Error> {
        super::validate_control(message, direction)?;
        let mut segments = message.segments();
        let msh = segments.next().ok_or(content("MSH", 0))?;
        self.results_message = super::control::part(raw(msh, 9), 1) == b"OUL";
        self.negative_query = super::control::part(raw(msh, 9), 1) == b"OML"
            && segments
                .map(Segment::id)
                .eq([b"SPM".as_slice(), b"SAC", b"ORC"])
            && message
                .segments()
                .find(|s| s.id() == b"ORC")
                .is_some_and(|s| {
                    super::OrderControl::parse(raw(s, 1)) == Ok(super::OrderControl::Discontinue)
                })
            && message
                .segments()
                .find(|s| s.id() == b"SPM")
                .is_some_and(|s| {
                    super::control::part(raw(s, 11), 1) == b"U"
                        && super::control::part(raw(s, 11), 3) == b"IHELAW"
                });
        super::order::validate_links(message, self, direction)?;
        let mut ordinal = 0usize;
        let mut note_sequence = 0usize;
        let mut occurrences = std::collections::BTreeMap::<&[u8], usize>::new();
        for segment in message.segments() {
            let occurrence = occurrences.entry(segment.id()).or_default();
            *occurrence = occurrence.checked_add(1).ok_or(content("MSH", 0))?;
            self.validate_segment(segment, direction)
                .map_err(|error| error.at_segment(segment, *occurrence))?;
            if segment.id() == b"NTE" {
                note_sequence = note_sequence.checked_add(1).ok_or(content("NTE", 1))?;
                if raw(segment, 1) != note_sequence.to_string().as_bytes() {
                    return Err(content("NTE", 1).at_segment(segment, *occurrence));
                }
            } else {
                note_sequence = 0;
            }
            if segment.id() == b"SPM" {
                ordinal = ordinal.checked_add(1).ok_or(content("SPM", 1))?;
                if super::specimen::set_id(raw(segment, 1)) != Some(ordinal) {
                    return Err(content("SPM", 1).at_segment(segment, *occurrence));
                }
            }
        }
        Ok(())
    }

    /// Checks a segment with explicit send/receive handling of unsupported content.
    /// RE availability stays with the app; absent optional values are never invented.
    /// # Errors
    /// Returns the field/component/subcomponent that violates the selected profile.
    pub fn validate_segment(
        self,
        segment: Segment<'_>,
        direction: super::ControlDirection,
    ) -> Result<(), Error> {
        match segment.id() {
            b"MSH" | b"MSA" | b"ERR" => super::control::validate_segment(segment, direction),
            b"PID" | b"PV1" => super::patient::validate(segment, self, direction),
            b"SPM" | b"SAC" => super::specimen::validate(segment, self, direction),
            b"ORC" | b"OBR" | b"TQ1" => super::order::validate(segment, self, direction),
            b"TCD" => super::tcd::validate(segment, self, direction),
            b"OBX" | b"NTE" | b"INV" => super::observation::validate(segment, self, direction),
            _ => validate_segment(segment),
        }
    }
}

/// LAW usage, including the two availability-dependent receiver obligations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Usage {
    /// Mandatory: present and valued, including explicit unknown where permitted.
    Mandatory,
    /// Required: sender supports it; null represents an unavailable required value.
    Required,
    /// Required if available at the sending manager.
    RequiredIfAvailable,
    /// Required if available from the analyzer.
    RequiredFromAnalyzer,
    /// Not supported in this context.
    Excluded,
    /// Conditional usage resolved by the owning segment predicate.
    Conditional,
}

/// A field constraint with an already-resolved conditional usage.
#[derive(Debug, Clone, Copy)]
pub struct FieldRule {
    /// Segment owning the rule.
    pub segment: &'static str,
    /// One-based field position.
    pub field: usize,
    /// Effective usage; unresolved Conditional is rejected.
    pub usage: Usage,
    /// Minimum repetitions when the field is present.
    pub minimum: usize,
    /// Maximum repetitions.
    pub maximum: usize,
    /// Maximum encoded bytes per repetition.
    pub length: usize,
}

impl FieldRule {
    /// Checks presence, exclusion, repetition count and length without changing input.
    /// Availability cannot be inferred by a receiver; absent RE/RE.AN remains valid.
    /// # Errors
    /// Returns the owning field for a violated constraint or unresolved predicate.
    pub fn check(self, segment: Segment<'_>) -> Result<(), Error> {
        let value = raw(segment, self.field);
        match self.usage {
            Usage::Mandatory | Usage::Required => {
                if value.is_empty() {
                    return Err(content(self.segment, self.field));
                }
            }
            Usage::Excluded => {
                if !value.is_empty() {
                    return Err(content(self.segment, self.field));
                }
                return Ok(());
            }
            Usage::Conditional => return Err(content(self.segment, self.field)),
            Usage::RequiredIfAvailable | Usage::RequiredFromAnalyzer => {
                if value.is_empty() {
                    return Ok(());
                }
            }
        }
        let field = segment
            .field(self.field)
            .ok_or(content(self.segment, self.field))?;
        let mut count = 0usize;
        for repetition in field.repetitions() {
            if repetition.raw().len() > self.length {
                return Err(content(self.segment, self.field));
            }
            count = count
                .checked_add(1)
                .ok_or(content(self.segment, self.field))?;
        }
        if count < self.minimum || count > self.maximum {
            return Err(content(self.segment, self.field));
        }
        Ok(())
    }
}

/// Checks shared LAW segment constraints without interpreting clinical content.
/// This entry point checks Appendix C fields independently of message grouping.
/// Control segments use the strict send policy; use `validate_control` for
/// directional control validation and cross-segment predicates.
/// # Errors
/// Returns a segment/field location for missing fields, excess repetitions or invalid codes.
pub fn validate_segment(segment: Segment<'_>) -> Result<(), Error> {
    if matches!(segment.id(), b"ORC" | b"OBR" | b"TQ1") {
        return super::order::validate(segment, Profile::receive(), super::ControlDirection::Send);
    }
    if segment.id() == b"TCD" {
        return super::tcd::validate(segment, Profile::receive(), super::ControlDirection::Send);
    }

    if matches!(segment.id(), b"SPM" | b"SAC") {
        return super::specimen::validate(
            segment,
            Profile::receive(),
            super::ControlDirection::Send,
        );
    }
    if matches!(segment.id(), b"PID" | b"PV1") {
        return super::patient::validate(
            segment,
            Profile::receive(),
            super::ControlDirection::Send,
        );
    }
    if matches!(segment.id(), b"MSH" | b"MSA" | b"ERR") {
        return super::control::validate_segment(segment, super::ControlDirection::Send);
    }
    match segment.id() {
        b"OBX" | b"NTE" | b"INV" => {
            super::observation::validate(segment, Profile::receive(), super::ControlDirection::Send)
        }
        b"QPD" | b"RCP" | b"QAK" => Ok(()),
        _ => Err(content("LAW", 0)),
    }
}

pub(super) fn header(message: &Message, transaction: &[u8], triggered: bool) -> Result<(), Error> {
    super::validate_control(message, super::ControlDirection::Receive)?;
    let msh = message.segments().next().ok_or(content("MSH", 0))?;
    let first = raw(msh, 21)
        .split(|b| *b == message.delimiters().repetition)
        .next()
        .unwrap_or_default();
    let mut components = first.split(|b| *b == message.delimiters().component);
    if components.next() != Some(transaction) || components.next() != Some(b"IHE".as_slice()) {
        return Err(content("MSH", 21));
    }
    let actual_trigger = matches!(
        raw(msh, 9)
            .split(|b| *b == message.delimiters().component)
            .next(),
        Some(b"QBP" | b"OML" | b"OUL")
    );
    if actual_trigger != triggered {
        return Err(content("MSH", 9));
    }
    Ok(())
}

fn digits(raw: &[u8]) -> Option<u16> {
    raw.iter().try_fold(0u16, |value, byte| {
        if !byte.is_ascii_digit() {
            return None;
        }
        value
            .checked_mul(10)?
            .checked_add(u16::from(byte.checked_sub(b'0')?))
    })
}

pub(super) fn timestamp(value: &[u8], zone: bool) -> bool {
    if value.len() != if zone { 19 } else { 14 } {
        return false;
    }
    let part = |start, end| value.get(start..end).and_then(digits);
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        part(0, 4),
        part(4, 6),
        part(6, 8),
        part(8, 10),
        part(10, 12),
        part(12, 14),
    ) else {
        return false;
    };
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(400) || (year.is_multiple_of(4) && !year.is_multiple_of(100)) => {
            29
        }
        2 => 28,
        _ => return false,
    };
    if year == 0 || day == 0 || day > days || hour > 23 || minute > 59 || second > 59 {
        return false;
    }
    if zone {
        if !matches!(value.get(14), Some(b'+' | b'-')) {
            return false;
        }
        if part(15, 17).is_none_or(|v| v > 23) || part(17, 19).is_none_or(|v| v > 59) {
            return false;
        }
    }
    true
}
