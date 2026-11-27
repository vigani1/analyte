//! Finds patient fields in HL7 v2 messages carried in MLLP blocks, IHE LAW included.
//!
//! Delimiters come from each message's MSH. Every segment type must be known:
//! patient and visit segments are replaced after their set ID, order,
//! specimen and container segments in their identifier, person and
//! free-text fields, and notes, query filters and display lines whole.
//! Observation, acknowledgment and equipment segments stay as they are. An
//! unknown segment, Z segments included, refuses the file because only its
//! sender knows what it holds.
use super::{
    Clock, Error, Plan,
    fields::{Delimiters, Escapes, Select, atoms, pieces},
    lanes::{Lane, Lanes},
    unparsed,
    values::Kind,
};
use analyte_framing::hl7::{CR, EB, SB};
use std::ops::Range;

const LF: u8 = b'\n';

/// Which fields of a segment hold patient values.
enum Rules {
    /// No patient values.
    Keep,
    /// Every field from this one on.
    From(usize),
    /// These fields only.
    Fields(&'static [(usize, Select)]),
}

const ID: Select = Select::First(&[1]);
const PARENT: Select = Select::First(&[1, 2]);

/// Segment rules from HL7 2.5.1 (chapters 2, 3, 4, 6, 7, 13) and 2.3.1 queries (chapter 2, 5).
fn rules(name: &[u8]) -> Option<Rules> {
    Some(match name {
        // Ch. 2 control, ch. 7 observations, ch. 13 equipment: no patient values.
        b"MSH" | b"MSA" | b"ERR" | b"QAK" | b"RCP" | b"DSC" | b"EVN" | b"SFT" | b"UAC" | b"OBX"
        | b"TQ1" | b"TQ2" | b"TCD" | b"INV" | b"EQU" | b"ECD" | b"ECR" | b"ISD" | b"NDS"
        | b"CTI" | b"SID" | b"BHS" | b"BTS" | b"FHS" | b"FTS" => Rules::Keep,
        // Ch. 3 patient and visit, ch. 6 insurance and guarantor: everything after the set ID.
        b"PID" | b"NK1" | b"GT1" | b"IN1" | b"IN3" | b"AL1" | b"DG1" => Rules::From(2),
        b"PD1" | b"PV2" | b"IN2" | b"ACC" => Rules::From(1),
        // PV1-2 is the patient class code, not a patient value; ch. 5 query
        // parameters follow the query name and tag.
        b"PV1" | b"QPD" => Rules::From(3),
        // Ch. 4 common order: placer, filler, group and parent numbers; people and places.
        b"ORC" => Rules::Fields(&[
            (2, ID),
            (3, ID),
            (4, ID),
            (8, PARENT),
            (10, Select::All),
            (11, Select::All),
            (12, Select::All),
            (13, Select::All),
            (14, Select::All),
            (21, Select::All),
            (22, Select::All),
            (23, Select::All),
            (24, Select::All),
        ]),
        // Ch. 7 observation request: order numbers, collector, clinical information, ordering provider.
        b"OBR" => Rules::Fields(&[
            (2, ID),
            (3, ID),
            (10, Select::All),
            (13, Select::All),
            (16, Select::All),
            (17, Select::All),
            (28, Select::All),
            (29, PARENT),
        ]),
        // Ch. 7 specimen: identifier, parent identifiers and description.
        b"SPM" => Rules::Fields(&[(2, PARENT), (3, PARENT), (14, Select::All)]),
        // Ch. 13 specimen container: accession, container and primary container IDs.
        b"SAC" => Rules::Fields(&[(2, ID), (3, ID), (4, ID)]),
        // Ch. 2 notes and 2.3.1 display lines; ch. 7 participation (the person)
        // and 2.3.1 other query filters; the 2.3.1 query who filter.
        b"NTE" | b"DSP" => Rules::Fields(&[(3, Select::All)]),
        b"PRT" | b"QRF" => Rules::Fields(&[(5, Select::All)]),
        b"QRD" => Rules::Fields(&[(8, Select::All)]),
        _ => return None,
    })
}

fn kind(name: &[u8], field: usize) -> Kind {
    match (name, field) {
        (b"PID", 7) | (b"NK1", 16) => Kind::BirthDate,
        (b"PID", 8) | (b"NK1", 15) => Kind::Sex,
        _ => Kind::Text,
    }
}

pub(super) fn scan(lanes: &Lanes, plan: &mut Plan, clock: &Clock) -> Result<(), Error> {
    for (index, lane) in lanes.lanes.iter().enumerate() {
        for block in blocks(lane)? {
            clock.check()?;
            message(lane, index, block, plan)?;
        }
    }
    Ok(())
}

/// MLLP blocks: SB, message, EB, CR. An unfinished last block is still read.
fn blocks(lane: &Lane) -> Result<Vec<Range<usize>>, Error> {
    let bytes = &lane.bytes;
    let mut found = Vec::new();
    let mut index = 0_usize;
    while let Some(byte) = bytes.get(index).copied() {
        match byte {
            SB => {
                let start = index.saturating_add(1);
                let end = bytes
                    .get(start..)
                    .unwrap_or_default()
                    .iter()
                    .position(|b| matches!(*b, EB | SB))
                    .map_or(bytes.len(), |n| start.saturating_add(n));
                found.push(start..end);
                index = if bytes.get(end) == Some(&EB) {
                    end.saturating_add(1)
                } else {
                    end
                };
            }
            CR | LF | b' ' | b'\t' => index = index.saturating_add(1),
            _ => {
                return Err(unparsed(
                    lane,
                    index,
                    analyte_i18n::text("cli-deidentify-hl7-outside-blocks"),
                ));
            }
        }
    }
    Ok(found)
}

fn message(lane: &Lane, index: usize, block: Range<usize>, plan: &mut Plan) -> Result<(), Error> {
    let text = lane.bytes.get(block.clone()).unwrap_or_default();
    let mut delimiters: Option<(u8, Delimiters)> = None;
    for segment in pieces(text, CR) {
        let start = text
            .get(segment.clone())
            .unwrap_or_default()
            .iter()
            .take_while(|b| **b == LF)
            .count()
            .saturating_add(segment.start);
        let bytes = text.get(start..segment.end).unwrap_or_default();
        let at = block.start.saturating_add(start);
        if bytes.is_empty() {
            continue;
        }
        let name = bytes.get(..3).unwrap_or(bytes);
        if name == b"MSH" {
            delimiters = Some(header(bytes).ok_or_else(|| {
                unparsed(
                    lane,
                    at,
                    analyte_i18n::text("cli-deidentify-hl7-msh-encoding"),
                )
            })?);
            continue;
        }
        let Some((field, delimiters)) = delimiters else {
            return Err(unparsed(
                lane,
                at,
                analyte_i18n::text("cli-deidentify-hl7-segment-before-msh"),
            ));
        };
        if bytes.len() > 3 && bytes.get(3) != Some(&field) {
            return Err(unparsed(
                lane,
                at,
                analyte_i18n::text("cli-deidentify-hl7-segment-no-separator"),
            ));
        }
        let rules = rules(name).ok_or_else(|| {
            unparsed(
                lane,
                at,
                analyte_i18n::text("cli-deidentify-hl7-segment-no-rules"),
            )
        })?;
        for (number, range) in pieces(bytes, field).into_iter().enumerate() {
            let select = match &rules {
                Rules::Keep => None,
                Rules::From(first) => (number >= *first).then_some(Select::All),
                Rules::Fields(list) => list
                    .iter()
                    .find(|(position, _)| *position == number)
                    .map(|(_, select)| *select),
            };
            let Some(select) = select else {
                continue;
            };
            let field_bytes = bytes.get(range.clone()).unwrap_or_default();
            let found = atoms(field_bytes, delimiters, select, Escapes::Hl7)
                .map_err(|reason| unparsed(lane, at, reason))?;
            for atom in found {
                let from = at
                    .saturating_add(range.start)
                    .saturating_add(atom.range.start);
                let to = at
                    .saturating_add(range.start)
                    .saturating_add(atom.range.end);
                plan.value(index, from..to, atom.keep, kind(name, number));
            }
        }
    }
    Ok(())
}

/// HL7 2.5.1 ch.2 §2.5.4: MSH-1 is the field separator; MSH-2 names the
/// component, repetition, escape and subcomponent characters in that order.
fn header(segment: &[u8]) -> Option<(u8, Delimiters)> {
    let [field, component, repeat, escape, subcomponent] = *segment.get(3..8)? else {
        return None;
    };
    let set = [field, component, repeat, escape, subcomponent];
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
            subcomponent: Some(subcomponent),
            escape,
        },
    ))
}
