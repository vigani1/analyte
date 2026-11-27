//! Finds patient fields in LIS02-A2 records carried in LIS01-A2 frames.
//!
//! Frames are joined per transfer (ENQ to EOT) into record text, so a record
//! split over frames is read whole. A frame repeated after a NAK carries the
//! same text and gets the same replacement; a repeat whose text differs
//! refuses the file. Only standard positions are used: P fields after the
//! sequence (LIS02-A2 §7), O specimen and instrument specimen IDs, collector,
//! clinical information and physician (§8.4), every component of the Q range
//! IDs, whose later components the manufacturer defines (§11), and comment
//! text not attached to a result (§10). A manufacturer (M) record or an
//! unknown record type refuses the file.
use super::{
    Clock, Error, Format, Plan, Run, Sum, Target,
    fields::{Delimiters, Escapes, Select, atoms, pieces},
    lanes::{Lane, Lanes},
    unparsed,
    values::Kind,
};
use analyte_framing::astm::{ACK, CR, ENQ, EOT, ETB, ETX, LF, NAK, STX};
use std::ops::Range;

/// One frame's place in its lane.
struct Frame {
    number: Option<u8>,
    text: Range<usize>,
    digits: Option<Range<usize>>,
}

/// Record text of one transfer and where each piece of it sits in the lane.
#[derive(Default)]
struct Transfer {
    text: Vec<u8>,
    /// Text start, and every lane start holding that piece (a repeat adds one).
    pieces: Vec<(usize, Vec<usize>)>,
}

impl Transfer {
    /// The lane start of text byte `index`, for error messages.
    fn position(&self, index: usize) -> usize {
        let after = self.pieces.partition_point(|(start, _)| *start <= index);
        self.pieces
            .get(after.saturating_sub(1))
            .and_then(|(start, lanes)| {
                lanes
                    .first()
                    .map(|lane| lane.saturating_add(index.saturating_sub(*start)))
            })
            .unwrap_or_default()
    }

    /// The runs holding text bytes `from..to`: one per frame piece crossed,
    /// each at every lane place that piece was sent.
    fn runs(&self, from: usize, to: usize) -> Vec<Run> {
        let mut runs = Vec::new();
        let first = self
            .pieces
            .partition_point(|(start, _)| *start <= from)
            .saturating_sub(1);
        for (number, (start, lanes)) in self.pieces.iter().enumerate().skip(first) {
            let end = self
                .pieces
                .get(number.saturating_add(1))
                .map_or(self.text.len(), |(next, _)| *next);
            let low = from.max(*start);
            let high = to.min(end);
            if low >= high {
                if *start >= to {
                    break;
                }
                continue;
            }
            runs.push(Run {
                at: low.saturating_sub(from),
                len: high.saturating_sub(low),
                starts: lanes
                    .iter()
                    .map(|lane| lane.saturating_add(low.saturating_sub(*start)))
                    .collect(),
            });
        }
        runs
    }
}

pub(super) fn scan(lanes: &Lanes, plan: &mut Plan, clock: &Clock) -> Result<(), Error> {
    for (index, lane) in lanes.lanes.iter().enumerate() {
        for frames in transfers(lane)? {
            clock.check()?;
            let transfer = assemble(lane, &frames, index, plan)?;
            records(lane, index, &transfer, plan)?;
        }
    }
    Ok(())
}

/// The frames of each transfer, in order. LIS01-A2 §8.3.1.2: STX, frame
/// number, text, ETB or ETX, two checksum characters, CR LF.
fn transfers(lane: &Lane) -> Result<Vec<Vec<Frame>>, Error> {
    let bytes = &lane.bytes;
    let mut all = Vec::new();
    let mut current = Vec::new();
    let mut index = 0_usize;
    while let Some(byte) = bytes.get(index).copied() {
        match byte {
            STX => {
                let number = bytes.get(index.saturating_add(1)).copied();
                let start = index.saturating_add(2).min(bytes.len());
                let end = bytes
                    .get(start..)
                    .unwrap_or_default()
                    .iter()
                    .position(|b| matches!(*b, ETX | ETB | STX | EOT | ENQ))
                    .map_or(bytes.len(), |n| start.saturating_add(n));
                let ended = matches!(bytes.get(end), Some(&(ETX | ETB)));
                let digits = ended.then(|| {
                    let from = end.saturating_add(1);
                    from..from.saturating_add(2).min(bytes.len())
                });
                index = match &digits {
                    Some(digits) => digits.end,
                    None => end,
                };
                current.push(Frame {
                    number,
                    text: start..end,
                    digits: digits.filter(|digits| digits.len() == 2),
                });
            }
            ENQ | EOT => {
                if !current.is_empty() {
                    all.push(std::mem::take(&mut current));
                }
                index = index.saturating_add(1);
            }
            0x20..=0x7E => {
                return Err(unparsed(
                    lane,
                    index,
                    analyte_i18n::text("cli-deidentify-astm-outside-frames"),
                ));
            }
            ACK | NAK | CR | LF | 0..=0x1F | 0x7F..=0xFF => index = index.saturating_add(1),
        }
    }
    if !current.is_empty() {
        all.push(current);
    }
    Ok(all)
}

/// Joins a transfer's frame texts; a repeated frame (same number, same text)
/// overlays the piece it repeats. Adds each frame's checksum.
fn assemble(
    lane: &Lane,
    frames: &[Frame],
    index: usize,
    plan: &mut Plan,
) -> Result<Transfer, Error> {
    let mut transfer = Transfer::default();
    let mut previous: Option<(Option<u8>, Range<usize>)> = None;
    for frame in frames {
        let text = lane.bytes.get(frame.text.clone()).unwrap_or_default();
        let repeat = previous
            .as_ref()
            .is_some_and(|(number, _)| *number == frame.number);
        if repeat {
            let same = previous
                .as_ref()
                .and_then(|(_, range)| lane.bytes.get(range.clone()))
                .is_some_and(|earlier| earlier == text);
            if !same {
                return Err(unparsed(
                    lane,
                    frame.text.start,
                    analyte_i18n::text("cli-deidentify-astm-repeat-differs"),
                ));
            }
            if let Some((_, lanes)) = transfer.pieces.last_mut() {
                lanes.push(frame.text.start);
            }
        } else {
            transfer
                .pieces
                .push((transfer.text.len(), vec![frame.text.start]));
            transfer.text.extend_from_slice(text);
        }
        if let Some(digits) = &frame.digits {
            plan.sums.push(Sum {
                lane: index,
                covered: vec![frame.text.clone()],
                digits: digits.clone(),
                // CLSI LIS01-A2 §8.3.3: two upper-case hex characters.
                format: Format::Hex {
                    standard_upper: true,
                },
            });
        }
        previous = Some((frame.number, frame.text.clone()));
    }
    Ok(transfer)
}

/// The field rule for one record type: which fields, how selected, how replaced.
fn rule(kind: u8, field: usize, after_result: bool) -> Option<(Select, Kind)> {
    match (kind, field) {
        // LIS02-A2 §7.8 birth date and §7.9 sex; every other P field after the sequence.
        (b'P', 7) => Some((Select::All, Kind::BirthDate)),
        (b'P', 8) => Some((Select::All, Kind::Sex)),
        (b'P', 2..) | (b'O' | b'Q', 2) | (b'O', 10 | 13 | 16 | 17) | (b'Q', 3) => {
            Some((Select::All, Kind::Text))
        }
        // LIS02-A2 §8.4.4: the instrument specimen ID; later components are the instrument's.
        (b'O', 3) => Some((Select::First(&[1]), Kind::Text)),
        // LIS02-A2 §10.4: comment text, unless it is an instrument comment on a result.
        (b'C', 3) if !after_result => Some((Select::All, Kind::Text)),
        _ => None,
    }
}

fn records(lane: &Lane, index: usize, transfer: &Transfer, plan: &mut Plan) -> Result<(), Error> {
    let text = &transfer.text;
    let mut delimiters: Option<(u8, Delimiters)> = None;
    let mut parent = b'H';
    let at = |offset: usize| transfer.position(offset);
    for record in pieces(text, CR) {
        // Some senders put LF after each record CR inside the frame text.
        let start = text
            .get(record.clone())
            .unwrap_or_default()
            .iter()
            .take_while(|b| **b == LF)
            .count()
            .saturating_add(record.start);
        let bytes = text.get(start..record.end).unwrap_or_default();
        let Some(kind) = bytes.first().map(u8::to_ascii_uppercase) else {
            continue;
        };
        if kind == b'H' {
            delimiters = Some(header(bytes).ok_or_else(|| {
                unparsed(
                    lane,
                    at(start),
                    analyte_i18n::text("cli-deidentify-astm-header-delimiters"),
                )
            })?);
            parent = b'H';
            continue;
        }
        let Some((field, delimiters)) = delimiters else {
            return Err(unparsed(
                lane,
                at(start),
                analyte_i18n::text("cli-deidentify-astm-record-before-header"),
            ));
        };
        if bytes.len() > 1 && bytes.get(1) != Some(&field) {
            return Err(unparsed(
                lane,
                at(start),
                analyte_i18n::text("cli-deidentify-astm-record-no-delimiter"),
            ));
        }
        match kind {
            b'P' | b'O' | b'Q' | b'C' | b'R' | b'L' | b'S' => (),
            b'M' => {
                return Err(unparsed(
                    lane,
                    at(start),
                    analyte_i18n::text("cli-deidentify-astm-manufacturer-record"),
                ));
            }
            _ => {
                return Err(unparsed(
                    lane,
                    at(start),
                    analyte_i18n::text("cli-deidentify-astm-unknown-record"),
                ));
            }
        }
        for (number, range) in pieces(bytes, field).into_iter().enumerate() {
            let Some((select, value)) = rule(kind, number, parent == b'R') else {
                continue;
            };
            let field_bytes = bytes.get(range.clone()).unwrap_or_default();
            let found = atoms(field_bytes, delimiters, select, Escapes::Astm)
                .map_err(|reason| unparsed(lane, at(start), reason))?;
            for atom in found {
                let from = start
                    .saturating_add(range.start)
                    .saturating_add(atom.range.start);
                let to = start
                    .saturating_add(range.start)
                    .saturating_add(atom.range.end);
                plan.targets.push(Target {
                    lane: index,
                    runs: transfer.runs(from, to),
                    keep: atom.keep,
                    kind: value,
                });
            }
        }
        if kind != b'C' {
            parent = kind;
        }
    }
    Ok(())
}

/// LIS02-A2 §6.2: the header's first four characters after H name the field,
/// repeat, component and escape delimiters.
fn header(record: &[u8]) -> Option<(u8, Delimiters)> {
    let [_, field, repeat, component, escape] = *record.get(..5)? else {
        return None;
    };
    let set = [field, repeat, component, escape];
    let distinct = set
        .iter()
        .enumerate()
        .all(|(i, a)| set.iter().skip(i.saturating_add(1)).all(|b| a != b));
    let plain = set
        .iter()
        .all(|b| b.is_ascii_graphic() && !b.is_ascii_alphanumeric());
    (distinct && plain).then_some((
        field,
        Delimiters {
            repeat,
            component,
            subcomponent: None,
            escape,
        },
    ))
}
