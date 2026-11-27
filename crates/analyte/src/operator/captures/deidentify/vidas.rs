//! Finds patient fields in VIDAS PC Literal packets (BCI Link 161150-486 C chapters 3 and 4).
//!
//! A packet is STX, data chunks each after RS, GS, two checksum characters and,
//! in Alternate mode, ETX; any control may carry a CR or LF suffix. The data is
//! `|`-separated tagged fields, and RS may split a field. Patient ID `pi`, name
//! `pn`, birth date `pb` and sex `ps` are replaced, and so is the sample ID
//! `ci` unless it names a kit or quality control. A tag outside the
//! documented set is treated as free text and replaced whole.
use super::{
    Clock, Error, Format, Plan, Run, Sum, Target,
    lanes::{Lane, Lanes},
    unparsed,
    values::Kind,
};
use std::ops::Range;

const STX: u8 = 0x02;
const ETX: u8 = 0x03;
const EOT: u8 = 0x04;
const ENQ: u8 = 0x05;
const ACK: u8 = 0x06;
const NAK: u8 = 0x15;
const GS: u8 = 0x1D;
const RS: u8 = 0x1E;
const CR: u8 = b'\r';
const LF: u8 = b'\n';

/// Tags documented for requests and results (BCI Link rev C pp. 4-2 to 4-8).
const KNOWN: &[&[u8]] = &[
    b"mt", b"pi", b"pn", b"pb", b"ps", b"so", b"si", b"ci", b"rt", b"rn", b"tt", b"td", b"ql",
    b"qn", b"y3", b"qd", b"nc", b"id", b"sn", b"m4",
];

/// One packet: the lane position of each data byte, and its checksum digits.
struct Packet {
    data: Vec<usize>,
    digits: Option<Range<usize>>,
}

pub(super) fn scan(lanes: &Lanes, plan: &mut Plan, clock: &Clock) -> Result<(), Error> {
    for (index, lane) in lanes.lanes.iter().enumerate() {
        for packet in packets(lane)? {
            clock.check()?;
            fields(lane, index, &packet, plan);
            if let Some(digits) = packet.digits {
                plan.sums.push(Sum {
                    lane: index,
                    covered: ranges(&packet.data),
                    digits,
                    // The letter case is a site setting; it is read from the recording.
                    format: Format::Hex {
                        standard_upper: false,
                    },
                });
            }
        }
    }
    Ok(())
}

/// Consecutive lane positions folded into ranges.
fn ranges(positions: &[usize]) -> Vec<Range<usize>> {
    let mut found: Vec<Range<usize>> = Vec::new();
    for position in positions {
        match found.last_mut() {
            Some(last) if last.end == *position => last.end = position.saturating_add(1),
            _ => found.push(*position..position.saturating_add(1)),
        }
    }
    found
}

fn packets(lane: &Lane) -> Result<Vec<Packet>, Error> {
    let bytes = &lane.bytes;
    let mut found = Vec::new();
    let mut index = 0_usize;
    while let Some(byte) = bytes.get(index).copied() {
        match byte {
            STX => {
                let mut data = Vec::new();
                let mut digits = None;
                index = index.saturating_add(1);
                while let Some(byte) = bytes.get(index).copied() {
                    match byte {
                        RS | CR | LF => index = index.saturating_add(1),
                        GS => {
                            let mut from = index.saturating_add(1);
                            while matches!(bytes.get(from), Some(&(CR | LF))) {
                                from = from.saturating_add(1);
                            }
                            let to = from.saturating_add(2).min(bytes.len());
                            digits = Some(from..to).filter(|range| range.len() == 2);
                            index = to;
                            break;
                        }
                        0x20..=0x7E => {
                            data.push(index);
                            index = index.saturating_add(1);
                        }
                        // STX, ENQ, EOT or ETX end an interrupted packet here.
                        STX | ENQ | EOT | ETX => break,
                        _ => return Err(unparsed(lane, index, "a byte VIDAS packets never carry")),
                    }
                }
                found.push(Packet { data, digits });
            }
            ENQ | ACK | NAK | EOT | ETX | CR | LF => index = index.saturating_add(1),
            _ => return Err(unparsed(lane, index, "bytes outside VIDAS packets")),
        }
    }
    Ok(found)
}

/// Whether `ci` names a kit control (C1-C3 with a strip lot) or a quality control,
/// as BCI Link rev C p. 4-5 and the `nc` control flags define them.
fn control(ci: &[u8], flag: Option<&[u8]>) -> bool {
    let name = ci.split(|b| *b == b' ').next().unwrap_or_default();
    matches!(name, b"C1" | b"C2" | b"C3")
        || matches!(ci, b"QC1" | b"QC2" | b"QC3" | b"QC4" | b"QC5" | b"QC6")
        || matches!(flag, Some(b"conthnor" | b"conthwst"))
}

fn fields(lane: &Lane, index: usize, packet: &Packet, plan: &mut Plan) {
    let data: Vec<u8> = packet
        .data
        .iter()
        .map(|position| lane.bytes.get(*position).copied().unwrap_or_default())
        .collect();
    let split = super::fields::pieces(&data, b'|');
    let value = |tag: &[u8]| {
        split
            .iter()
            .filter_map(|range| data.get(range.clone()))
            .find(|field| field.get(..2) == Some(tag))
            .and_then(|field| field.get(2..))
    };
    let flag = value(b"nc");
    for range in &split {
        let field = data.get(range.clone()).unwrap_or_default();
        let Some(tag) = field.get(..2) else {
            continue;
        };
        let kind = match tag {
            b"pi" | b"pn" => Kind::Text,
            b"pb" => Kind::BirthDate,
            b"ps" => Kind::Sex,
            b"ci" if !control(field.get(2..).unwrap_or_default(), flag) => Kind::Text,
            _ if !KNOWN.contains(&tag) => Kind::Text,
            _ => continue,
        };
        let from = range.start.saturating_add(2);
        if from >= range.end {
            continue;
        }
        let mut runs = Vec::new();
        let mut at = 0_usize;
        for piece in ranges(packet.data.get(from..range.end).unwrap_or_default()) {
            let len = piece.len();
            runs.push(Run {
                at,
                len,
                starts: vec![piece.start],
            });
            at = at.saturating_add(len);
        }
        plan.targets.push(Target {
            lane: index,
            runs,
            keep: Vec::new(),
            kind,
        });
    }
}
