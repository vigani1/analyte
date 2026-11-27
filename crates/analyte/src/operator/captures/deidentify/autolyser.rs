//! Finds the patient code in Autolyser internal-protocol exchanges (Rev18 chapter 23).
//!
//! The host sends STX, the machine ACK, then the host a one-letter request
//! (R, L or A) or a work-list record, each ending in EOT. The machine answers
//! a request with NAK or a report, and a work-list record with two insertion
//! reply bytes. Reports and work-list records start with the fifteen-byte
//! patient code and end with a three-digit decimal checksum before EOT
//! (Rev18 pp.175-179). Bytes that arrive outside this exchange refuse the file,
//! because only the exchange says what they are.
use super::{Clock, Error, Format, Plan, Sum, lanes::Lanes, unparsed, values::Kind};
use analyte_framing::autolyser::{EOT, NAK, STX};
use analyte_transport::recording::Direction;

/// Bytes of the patient code (Rev18 p.176).
const PATIENT: usize = 15;
/// Decimal checksum digits before EOT (Rev18 p.179).
const DIGITS: usize = 3;

#[derive(Clone, Copy)]
enum Host {
    Idle,
    /// STX sent; the request starts with the next host byte.
    Sent,
    /// A request or work-list record that started here.
    Request(usize),
}

/// The lanes of the current link session.
#[derive(Default, Clone, Copy)]
struct Open {
    host: Option<usize>,
    machine: Option<usize>,
}

#[derive(Clone, Copy)]
enum Machine {
    Nothing,
    Ack,
    /// A report or NAK; the report started here once its first byte arrived.
    Report(Option<usize>),
    /// This many insertion reply bytes are still due.
    Insertion(u8),
}

pub(super) fn scan(lanes: &Lanes, plan: &mut Plan, clock: &Clock) -> Result<(), Error> {
    let mut host = Host::Idle;
    let mut machine = Machine::Nothing;
    let mut open = Open::default();
    for place in &lanes.places {
        clock.check()?;
        let Some(place) = place else {
            finish(lanes, std::mem::take(&mut open), host, machine, plan)?;
            host = Host::Idle;
            machine = Machine::Nothing;
            continue;
        };
        let lane = lanes.lanes.get(place.lane).ok_or(Error::Limit)?;
        for index in place.bytes.clone() {
            let byte = lane.bytes.get(index).copied().ok_or(Error::Limit)?;
            if lane.direction == Direction::HostToMachine {
                open.host = Some(place.lane);
                host = match (host, byte) {
                    (Host::Idle, STX) => {
                        machine = Machine::Ack;
                        Host::Sent
                    }
                    (Host::Idle, _) => {
                        return Err(unparsed(
                            lane,
                            index,
                            analyte_i18n::text("cli-deidentify-autolyser-host-outside"),
                        ));
                    }
                    (Host::Sent, _) | (Host::Request(_), _) => {
                        let start = match host {
                            Host::Request(start) => start,
                            Host::Idle | Host::Sent => index,
                        };
                        if byte == EOT {
                            machine = request(lanes, place.lane, start, index, plan)?;
                            Host::Idle
                        } else {
                            Host::Request(start)
                        }
                    }
                };
            } else {
                open.machine = Some(place.lane);
                machine = match (machine, byte) {
                    (Machine::Ack, _)
                    | (Machine::Report(None), NAK)
                    | (Machine::Insertion(1), _) => Machine::Nothing,
                    (Machine::Insertion(due), _) => Machine::Insertion(due.saturating_sub(1)),
                    (Machine::Report(None), EOT) => {
                        record(lanes, place.lane, index, index, plan)?;
                        Machine::Nothing
                    }
                    (Machine::Report(None), _) => Machine::Report(Some(index)),
                    (Machine::Report(Some(start)), EOT) => {
                        record(lanes, place.lane, start, index, plan)?;
                        Machine::Nothing
                    }
                    (Machine::Report(Some(start)), _) => Machine::Report(Some(start)),
                    (Machine::Nothing, _) => {
                        return Err(unparsed(
                            lane,
                            index,
                            analyte_i18n::text("cli-deidentify-autolyser-unasked"),
                        ));
                    }
                };
            }
        }
    }
    finish(lanes, open, host, machine, plan)
}

/// A host record ended at EOT: a one-letter request or a work-list record.
fn request(
    lanes: &Lanes,
    lane: usize,
    start: usize,
    eot: usize,
    plan: &mut Plan,
) -> Result<Machine, Error> {
    if eot.saturating_sub(start) == 1 {
        return Ok(Machine::Report(None));
    }
    record(lanes, lane, start, eot, plan)?;
    Ok(Machine::Insertion(2))
}

/// A checksummed record from `start` to its EOT: patient code first.
fn record(
    lanes: &Lanes,
    lane: usize,
    start: usize,
    eot: usize,
    plan: &mut Plan,
) -> Result<(), Error> {
    let bytes = lanes.lanes.get(lane).ok_or(Error::Limit)?;
    let payload_end = eot
        .checked_sub(DIGITS)
        .filter(|end| end.saturating_sub(start) >= PATIENT)
        .ok_or_else(|| {
            unparsed(
                bytes,
                start,
                analyte_i18n::text("cli-deidentify-autolyser-short-record"),
            )
        })?;
    plan.value(
        lane,
        start..start.saturating_add(PATIENT),
        Vec::new(),
        Kind::Text,
    );
    let payload = start..payload_end;
    plan.sums.push(Sum {
        lane,
        covered: vec![payload],
        digits: payload_end..eot,
        format: Format::Decimal,
    });
    Ok(())
}

/// A record cut short by a disconnect or the end of the file: its patient code
/// is still replaced, as far as it arrived.
fn finish(
    lanes: &Lanes,
    open: Open,
    host: Host,
    machine: Machine,
    plan: &mut Plan,
) -> Result<(), Error> {
    let partial = match (host, machine) {
        (Host::Request(start), _) => open.host.map(|lane| (lane, start)),
        (_, Machine::Report(Some(start))) => open.machine.map(|lane| (lane, start)),
        (
            Host::Idle | Host::Sent,
            Machine::Nothing | Machine::Ack | Machine::Report(None) | Machine::Insertion(_),
        ) => None,
    };
    if let Some((lane, start)) = partial {
        let length = lanes.lanes.get(lane).ok_or(Error::Limit)?.bytes.len();
        let end = start.saturating_add(PATIENT).min(length);
        plan.value(lane, start..end, Vec::new(), Kind::Text);
    }
    Ok(())
}
