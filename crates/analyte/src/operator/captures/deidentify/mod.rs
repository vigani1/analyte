//! Makes a de-identified copy of one recording: same-shape synthetic patient fields and recomputed checksums (ADR 0037).
//!
//! Each supported protocol finds its patient fields from the standard's own
//! positions. Every replacement keeps its byte length, so framing lengths,
//! frame numbers, event boundaries and times stay exactly as recorded; only the
//! checksums that cover a changed byte are recomputed. A checksum that was wrong
//! in the original stays wrong by the same amount, so a recorded fault still
//! replays as a fault. Anything the scanner cannot place refuses the whole file:
//! a half-cleaned copy is never produced. Errors name an event index and a
//! reason, never a value.
mod astm;
mod autolyser;
mod fields;
mod hl7;
mod lanes;
#[cfg(test)]
mod tests;
mod values;
mod vidas;

use analyte_transport::recording::{Event, Header};
use lanes::{Lane, Lanes};
use std::{collections::HashSet, ops::Range, time::Instant};
use values::{Kind, Pseudonyms};

/// The opening words of the privacy statement in every de-identified copy's header (ADR 0037).
pub const MARK: &str = "De-identified by Analyte captures de-identify";

/// What the command prints after writing a copy.
#[must_use]
pub fn checklist() -> &'static str {
    analyte_i18n::text("cli-deidentify-checklist")
}

/// Why no copy was made.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The protocol has no de-identification rules.
    #[error("{}", analyte_i18n::message("cli-deidentify-unsupported", [("protocol", (.0).to_string().into())]))]
    Unsupported(&'static str),
    /// The header names a protocol stack Analyte does not know.
    #[error("{}", analyte_i18n::text("cli-deidentify-unknown-stack"))]
    UnknownStack,
    /// A message could not be read well enough to find every patient field.
    #[error("{}", analyte_i18n::message("cli-deidentify-unparsed", [("event", (event).to_string().into()), ("reason", (reason).to_string().into())]))]
    Unparsed {
        /// Zero-based event index (the file's line is this plus two).
        event: usize,
        /// What was not understood; never the bytes.
        reason: &'static str,
    },
    /// A checksum covering a replaced field cannot be recomputed.
    #[error("{}", analyte_i18n::message("cli-deidentify-checksum", [("event", (event).to_string().into()), ("reason", (reason).to_string().into())]))]
    Checksum {
        /// Zero-based event index.
        event: usize,
        /// Why; never the bytes.
        reason: &'static str,
    },
    /// The work took longer than its bound.
    #[error("{}", analyte_i18n::message("cli-deidentify-deadline", [("seconds", (.0).to_string().into())]))]
    Deadline(u64),
    /// The system random source failed.
    #[error("{}", analyte_i18n::message("cli-deidentify-random", [("error", (.0).to_string().into())]))]
    Random(getrandom::Error),
    /// An internal bound was exceeded.
    #[error("{}", analyte_i18n::text("cli-deidentify-limit"))]
    Limit,
}

/// The checksum letter case the operator states when the recording cannot show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum LetterCase {
    /// A to F.
    Upper,
    /// a to f.
    Lower,
}

/// The de-identified recording and what was replaced.
#[derive(Debug)]
pub struct Copy {
    /// The marked header.
    pub header: Header,
    /// The events, same count, directions, times and lengths as the original.
    pub events: Vec<Event>,
    /// Values replaced, counting every occurrence.
    pub replaced: usize,
    /// Distinct real values replaced.
    pub distinct: usize,
}

/// The longest the work may take; a recording is at most 16 MiB.
pub const TIME_LIMIT_SECONDS: u64 = 120;

/// The supported protocols, from the recording's own stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Protocol {
    Astm,
    Hl7,
    Vidas,
    Autolyser,
}

impl Protocol {
    fn of(stack: &[String; 4]) -> Result<Self, Error> {
        let [_, framing, dialog, syntax] = stack;
        match (framing.as_str(), dialog.as_str(), syntax.as_str()) {
            ("astm_lis01", _, "astm_lis02") => Ok(Self::Astm),
            // IHE LAW is HL7 v2.5.1; the same segment rules apply.
            ("mllp", "hl7_receiver" | "law_manager", "hl7_v2") => Ok(Self::Hl7),
            ("vidas_literal", "vidas_literal", "vidas_literal") => Ok(Self::Vidas),
            ("autolyser", "autolyser", "autolyser") => Ok(Self::Autolyser),
            // POCT1-A carries patient data in many optional XML elements and
            // vendor extensions; a partial clean is worse than none.
            ("poct", _, _) => Err(Error::Unsupported("POCT1-A")),
            ("none", _, _) => Err(Error::Unsupported("watched-folder")),
            _ => Err(Error::UnknownStack),
        }
    }
}

/// A bound on the work, checked as it goes.
struct Clock {
    deadline: Instant,
}

impl Clock {
    fn check(&self) -> Result<(), Error> {
        if Instant::now() > self.deadline {
            return Err(Error::Deadline(TIME_LIMIT_SECONDS));
        }
        Ok(())
    }
}

/// Consecutive bytes of a value that sit together in the lane, at every place
/// they were sent (a frame sent twice holds the same bytes twice).
struct Run {
    /// Where these bytes start within the value.
    at: usize,
    len: usize,
    /// Lane start of each copy; the first is the one read.
    starts: Vec<usize>,
}

/// One patient value to replace: its bytes in one lane, in runs split only
/// by frame or chunk boundaries, so memory follows the framing, not the bytes.
struct Target {
    lane: usize,
    runs: Vec<Run>,
    keep: Vec<bool>,
    kind: Kind,
}

/// How a checksum is written.
#[derive(Debug, Clone, Copy)]
enum Format {
    /// Two hex digits; `standard_upper` when the standard fixes upper case.
    Hex { standard_upper: bool },
    /// Three decimal digits of a sum modulo 256.
    Decimal,
}

/// A checksum over lane bytes.
struct Sum {
    lane: usize,
    /// Lane bytes the sum covers that a replacement may change.
    covered: Vec<Range<usize>>,
    /// The checksum digits.
    digits: Range<usize>,
    format: Format,
}

/// What a protocol scanner found.
#[derive(Default)]
struct Plan {
    targets: Vec<Target>,
    sums: Vec<Sum>,
}

impl Plan {
    /// Adds a value whose bytes are contiguous in the lane.
    fn value(&mut self, lane: usize, range: Range<usize>, keep: Vec<bool>, kind: Kind) {
        self.targets.push(Target {
            lane,
            runs: vec![Run {
                at: 0,
                len: range.len(),
                starts: vec![range.start],
            }],
            keep,
            kind,
        });
    }
}

fn unparsed(lane: &Lane, index: usize, reason: &'static str) -> Error {
    Error::Unparsed {
        event: lane.event_at(index),
        reason,
    }
}

/// Makes the de-identified copy, or refuses the whole file.
/// # Errors
/// Refuses unsupported protocols, unreadable messages, checksums that cannot
/// be recomputed, work over its time bound and random source failures.
///
/// `case` is the checksum letter case from the machine's settings; it is used
/// only when neither the checksum nor the others in its direction show one and
/// the standard fixes none (VIDAS).
pub fn de_identify(
    header: &Header,
    events: &[Event],
    case: Option<LetterCase>,
) -> Result<Copy, Error> {
    let clock = Clock {
        deadline: Instant::now()
            .checked_add(std::time::Duration::from_secs(TIME_LIMIT_SECONDS))
            .ok_or(Error::Limit)?,
    };
    let protocol = Protocol::of(&header.stack)?;
    let mut lanes = Lanes::join(events);
    let mut plan = Plan::default();
    match protocol {
        Protocol::Astm => astm::scan(&lanes, &mut plan, &clock)?,
        Protocol::Hl7 => hl7::scan(&lanes, &mut plan, &clock)?,
        Protocol::Vidas => vidas::scan(&lanes, &mut plan, &clock)?,
        Protocol::Autolyser => autolyser::scan(&lanes, &mut plan, &clock)?,
    }
    let original: Vec<Vec<u8>> = lanes.lanes.iter().map(|lane| lane.bytes.clone()).collect();
    let real_of = |target: &Target| -> Vec<u8> {
        let bytes = original
            .get(target.lane)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut value = Vec::new();
        for run in &target.runs {
            let start = run.starts.first().copied().unwrap_or_default();
            value.extend_from_slice(
                bytes
                    .get(start..start.saturating_add(run.len))
                    .unwrap_or_default(),
            );
        }
        value
    };
    let reals: HashSet<Vec<u8>> = plan.targets.iter().map(&real_of).collect();
    let mut names = Pseudonyms::new(reals);
    for (count, target) in plan.targets.iter().enumerate() {
        if count % 4096 == 0 {
            clock.check()?;
        }
        let real = real_of(target);
        let fake = names.stand_in(target.kind, &real, &target.keep)?;
        if fake.len() != real.len() {
            return Err(Error::Limit);
        }
        let lane = lanes.lanes.get_mut(target.lane).ok_or(Error::Limit)?;
        for run in &target.runs {
            let part = fake
                .get(run.at..run.at.saturating_add(run.len))
                .ok_or(Error::Limit)?;
            for start in &run.starts {
                lane.bytes
                    .get_mut(*start..start.saturating_add(run.len))
                    .ok_or(Error::Limit)?
                    .copy_from_slice(part);
            }
        }
    }
    fix_sums(&plan.sums, &original, &mut lanes, case)?;
    clock.check()?;
    let mut marked = header.clone();
    marked.de_identification = format!(
        "{MARK} (ADR 0037): patient fields replaced with synthetic values of the same shape, checksums recomputed. Review before sharing."
    );
    marked.source = format!("{}; de-identified copy", header.source);
    Ok(Copy {
        header: marked,
        events: lanes.split(events),
        replaced: plan.targets.len(),
        distinct: names.distinct(),
    })
}

/// Letter case seen in one lane's original checksums: upper, lower, both.
fn lane_cases(sums: &[Sum], original: &[Vec<u8>]) -> Vec<(bool, bool)> {
    let mut cases = vec![(false, false); original.len()];
    for sum in sums {
        let digits = original
            .get(sum.lane)
            .and_then(|lane| lane.get(sum.digits.clone()))
            .unwrap_or_default();
        if let Some(case) = cases.get_mut(sum.lane) {
            case.0 |= digits.iter().any(|b| matches!(b, b'A'..=b'F'));
            case.1 |= digits.iter().any(|b| matches!(b, b'a'..=b'f'));
        }
    }
    cases
}

/// Adds the change of the covered bytes to each checksum, in its own spelling.
fn fix_sums(
    sums: &[Sum],
    original: &[Vec<u8>],
    lanes: &mut Lanes,
    case: Option<LetterCase>,
) -> Result<(), Error> {
    let cases = lane_cases(sums, original);
    for sum in sums {
        let old = original.get(sum.lane).ok_or(Error::Limit)?;
        let lane = lanes.lanes.get_mut(sum.lane).ok_or(Error::Limit)?;
        let mut delta = 0_u8;
        for range in &sum.covered {
            for index in range.clone() {
                let new = lane.bytes.get(index).copied().unwrap_or_default();
                let was = old.get(index).copied().unwrap_or_default();
                delta = delta.wrapping_add(new).wrapping_sub(was);
            }
        }
        if delta == 0 {
            continue;
        }
        let event = lane.event_at(sum.digits.start);
        let digits = old.get(sum.digits.clone()).unwrap_or_default();
        let text = std::str::from_utf8(digits).ok();
        let spelled = match sum.format {
            Format::Hex { standard_upper } => {
                let value = text
                    .filter(|text| text.len() == 2)
                    .and_then(|text| u8::from_str_radix(text, 16).ok())
                    .ok_or(Error::Checksum {
                        event,
                        reason: analyte_i18n::text("cli-deidentify-checksum-not-hex"),
                    })?;
                let new = value.wrapping_add(delta);
                let upper = format!("{new:02X}");
                let lower = format!("{new:02x}");
                let own = (
                    digits.iter().any(|b| matches!(b, b'A'..=b'F')),
                    digits.iter().any(|b| matches!(b, b'a'..=b'f')),
                );
                let seen = cases.get(sum.lane).copied().unwrap_or_default();
                match (own, seen) {
                    _ if upper == lower => upper,
                    ((true, false), _) | ((false, false), (true, false)) => upper,
                    ((false, true), _) | ((false, false), (false, true)) => lower,
                    _ => match case {
                        Some(LetterCase::Upper) => upper,
                        Some(LetterCase::Lower) => lower,
                        None if standard_upper => upper,
                        None => {
                            return Err(Error::Checksum {
                                event,
                                reason: analyte_i18n::text("cli-deidentify-checksum-case-unknown"),
                            });
                        }
                    },
                }
            }
            Format::Decimal => {
                let value = text
                    .filter(|text| text.len() == 3)
                    .and_then(|text| text.parse::<u8>().ok())
                    .ok_or(Error::Checksum {
                        event,
                        reason: analyte_i18n::text("cli-deidentify-checksum-not-sum"),
                    })?;
                format!("{:03}", value.wrapping_add(delta))
            }
        };
        let slots = lane.bytes.get_mut(sum.digits.clone()).ok_or(Error::Limit)?;
        if slots.len() != spelled.len() {
            return Err(Error::Limit);
        }
        slots.copy_from_slice(spelled.as_bytes());
    }
    Ok(())
}
