//! Tests ADR 0015 packing against LIS01-A2 §§3.3.2, 6.3.1.1 and 8.3.1.1.
use analyte_dialog::astm::{Action, Config, Dialog, Epoch, Event};
use analyte_framing::astm::{self, ACK, ENQ, FrameNumber, Limit, NAK, RecordBoundaries};
use std::time::Duration;
type Result = std::result::Result<(), Box<dyn std::error::Error>>;

fn feed(
    dialog: &mut Dialog,
    bytes: &[u8],
) -> std::result::Result<Vec<Action>, analyte_dialog::astm::Error> {
    let mut actions = Vec::new();
    for byte in bytes {
        actions.extend(dialog.step(Duration::ZERO, Event::Byte(*byte))?);
    }
    Ok(actions)
}
fn receiver(policy: RecordBoundaries) -> std::result::Result<Dialog, analyte_dialog::astm::Error> {
    let mut d = Dialog::new(Config::default().with_record_boundaries(policy), Epoch(1));
    d.step(Duration::ZERO, Event::Capacity(true))?;
    feed(&mut d, &[ENQ])?;
    Ok(d)
}

#[test]
fn adr0015_lis01_s6311_packing_requires_declaration_and_one_frame_custody() -> Result {
    let frames: Vec<_> =
        astm::encode(b"H|x\rP|1\r", FrameNumber::FIRST, Limit::default())?.collect();
    let frame = frames.first().ok_or("frame")?;
    let mut strict = receiver(RecordBoundaries::Separate)?;
    assert!(feed(&mut strict, frame)?.contains(&Action::Write(vec![NAK])));
    let mut packed = receiver(RecordBoundaries::Packed)?;
    let actions = feed(&mut packed, frame)?;
    assert!(!actions.contains(&Action::Write(vec![ACK])));
    let ids: Vec<_> = actions
        .iter()
        .filter_map(|a| {
            if let Action::Store(id, _, raw) = a {
                assert_eq!(raw, frame);
                Some(*id)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(ids.len(), 1);
    let id = *ids.first().ok_or("commit")?;
    let actions = packed.step(Duration::ZERO, Event::Stored(id))?;
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a,Action::Write(bytes) if bytes==&[ACK]))
            .count(),
        1
    );
    assert!(actions.contains(&Action::Record(b"H|x\r".to_vec())));
    assert!(actions.contains(&Action::Record(b"P|1\r".to_vec())));
    let retry = feed(&mut packed, frame)?;
    assert!(retry.contains(&Action::Write(vec![ACK])));
    assert!(
        !retry
            .iter()
            .any(|a| matches!(a, Action::Store(..) | Action::Record(_)))
    );
    Ok(())
}

#[test]
fn adr0015_lis01_s632_packing_checksum_and_failed_custody_remain_strict() -> Result {
    let frames: Vec<_> =
        astm::encode(b"H|x\rP|1\r", FrameNumber::FIRST, Limit::default())?.collect();
    let frame = frames.first().ok_or("frame")?;
    let mut wrong = frame.clone();
    *wrong.get_mut(3).ok_or("text byte")? = b'Z';
    let mut dialog = receiver(RecordBoundaries::Packed)?;
    let actions = feed(&mut dialog, &wrong)?;
    assert!(actions.contains(&Action::Write(vec![NAK])));
    assert!(!actions.iter().any(|a| matches!(a, Action::Store(..))));
    let actions = feed(&mut dialog, frame)?;
    let id = actions
        .iter()
        .find_map(|a| {
            if let Action::Store(id, ..) = a {
                Some(*id)
            } else {
                None
            }
        })
        .ok_or("commit")?;
    let failed = dialog.step(Duration::ZERO, Event::StoreFailed(id))?;
    assert!(failed.contains(&Action::Write(vec![NAK])));
    assert!(!failed.iter().any(|a| matches!(a, Action::Record(_))));
    Ok(())
}

#[test]
fn adr0015_lis01_s6311_packed_cr_boundaries_survive_etb_and_split_reads() -> Result {
    let frames: Vec<_> =
        astm::encode(b"H|x\rP|123\r", FrameNumber::FIRST, Limit::new(13)?)?.collect();
    let mut dialog = receiver(RecordBoundaries::Packed)?;
    let mut records = Vec::new();
    for frame in frames {
        let actions = feed(&mut dialog, &frame)?;
        let id = actions
            .iter()
            .find_map(|a| {
                if let Action::Store(id, ..) = a {
                    Some(*id)
                } else {
                    None
                }
            })
            .ok_or("commit")?;
        for action in dialog.step(Duration::ZERO, Event::Stored(id))? {
            if let Action::Record(record) = action {
                records.push(record);
            }
        }
    }
    assert_eq!(records, vec![b"H|x\r".to_vec(), b"P|123\r".to_vec()]);
    Ok(())
}
