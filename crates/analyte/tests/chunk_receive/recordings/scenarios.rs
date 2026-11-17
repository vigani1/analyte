//! Defines synthetic exchanges from LAW, POCT01-A2, VIDAS Literal and Autolyser sources.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Protocol {
    Law,
    Poct,
    Vidas,
    Autolyser,
}
impl Protocol {
    pub(super) const ALL: [Self; 4] = [Self::Law, Self::Poct, Self::Vidas, Self::Autolyser];
    pub(super) fn key(self) -> &'static str {
        match self {
            Self::Law => "law",
            Self::Poct => "poct",
            Self::Vidas => "vidas",
            Self::Autolyser => "autolyser",
        }
    }
    pub(super) fn source(self) -> &'static str {
        match self {
            Self::Law => "IHE PaLM TF-2b LAB-29 section 3.29.4 and B.7.6",
            Self::Poct => "CLSI POCT01-A2 Appendix B sections 3.2, 5.5, 5.13 and 5.21",
            Self::Vidas => "VIDAS BCI-A chapter 3 Literal transfer and Literal notes section 3",
            Self::Autolyser => "Autolyser Operator Manual Rev18 chapter 23 pages 175-179",
        }
    }
    pub(super) fn definition(self) -> &'static str {
        match self {
            Self::Law => include_str!("../../../../core/config/examples/law.toml"),
            Self::Poct => include_str!("../../../../core/config/examples/poct.toml"),
            Self::Vidas => include_str!("../../../../core/config/examples/vidas_literal.toml"),
            Self::Autolyser => include_str!("../../../../core/config/examples/autolyser.toml"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Case {
    Happy,
    LostAck,
    Disconnect,
}
impl Case {
    pub(super) const ALL: [Self; 3] = [Self::Happy, Self::LostAck, Self::Disconnect];
    pub(super) fn key(self) -> &'static str {
        match self {
            Self::Happy => "happy",
            Self::LostAck => "lost-ack",
            Self::Disconnect => "disconnect",
        }
    }
    pub(super) fn description(self, protocol: Protocol) -> &'static str {
        match self {
            Self::Happy => {
                "successful synthetic result custody; unresolved vendor identities remain parked"
            }
            Self::LostAck => match protocol {
                Protocol::Law => {
                    "discard observed host result ACK, retransmit result, retain one delivery"
                }
                Protocol::Poct => {
                    "discard observed result ACK, retransmit same control ID; reused ID requires ESC and abnormal END with retained evidence"
                }
                Protocol::Vidas => {
                    "discard observed packet ACK, retransmit packet while final controls are expected; close visibly and retain original custody"
                }
                Protocol::Autolyser => {
                    "omit machine initialization ACK; wait for acknowledgment deadline and require host EOF without R or retry"
                }
            },
            Self::Disconnect => {
                "machine disconnects during an incomplete result after any required initialization"
            }
        }
    }
}

pub(super) enum Step {
    Machine(Vec<u8>),
    Host(Vec<u8>),
    Xml,
    Mllp,
    Escape,
    End,
    Disconnect,
}

pub(super) fn observation() -> Vec<u8> {
    poct_peer::observation("observation", true, "<OBS.value V=\"12.30\" U=\"mg/L\"/>").into_bytes()
}

pub(super) fn vidas() -> Result<Vec<u8>, Error> {
    Ok(vidas_config()?
        .encode(b"mtrsl|pi|si|ci00012a|rtSYN|tt|td|qlpositive|qn<0.1|y3mg/L|qd1:10|")?)
}

fn law() -> Vec<u8> {
    let mut fields = vec![""; 29];
    for (index, value) in [
        (0, "1"),
        (1, "NM"),
        (2, "SYN^Synthetic^99LAB"),
        (3, "1"),
        (4, "12.30"),
        (5, "mg/L^mg/L^UCUM"),
        (6, "0-20"),
        (7, "N^Normal^HL70078"),
        (10, "F"),
        (15, "operator^Synthetic"),
        (17, "instrument^99LAB~module^99LAB"),
        (18, "20260101120000"),
        (28, "RSLT"),
    ] {
        if let Some(field) = fields.get_mut(index) {
            *field = value;
        }
    }
    format!("\x0bMSH|^~\\&|A|AF|M|MF|20260101120000+0000||OUL^R22^OUL_R22|recording|P|2.5.1|||NE|AL||UNICODE UTF-8|||LAB-29^IHE\rSPM|1|sample-01&99LAB||SER^Serum^HL70487|||||||P^^HL70369\rSAC|||00012a^LAB\rOBR||\"\"|awos-1^LAB|SYN^Synthetic^99LAB\rORC|SC||||CM\rOBX|{}\r\x1c\r", fields.join("|")).into_bytes()
}

pub(super) fn steps(protocol: Protocol, case: Case) -> Result<Vec<Step>, Error> {
    let (mut steps, raw, ack) = match protocol {
        Protocol::Law => (Vec::new(), law(), Step::Mllp),
        Protocol::Poct => (
            vec![
                Step::Machine(poct_peer::hello("hello").into_bytes()),
                Step::Xml,
                Step::Machine(poct_peer::status("status", "1").into_bytes()),
                Step::Xml,
                Step::Xml,
            ],
            observation(),
            Step::Xml,
        ),
        Protocol::Vidas => (
            vec![Step::Machine(vec![5]), Step::Host(vec![6])],
            vidas()?,
            Step::Host(vec![6]),
        ),
        Protocol::Autolyser => {
            if case == Case::LostAck {
                return Ok(vec![Step::Host(vec![2])]);
            }
            (
                vec![
                    Step::Host(vec![2]),
                    Step::Machine(vec![6]),
                    Step::Host(vec![b'R', 4]),
                ],
                report()?,
                Step::Host(Vec::new()),
            )
        }
    };
    if case == Case::Disconnect {
        let prefix = raw
            .get(..raw.len() / 2)
            .ok_or("partial result prefix")?
            .to_vec();
        steps.extend([Step::Machine(prefix), Step::Disconnect]);
        return Ok(steps);
    }
    // Split reads are deliberate machine writes, with host acceptance only after the suffix.
    let middle = raw.len() / 2;
    steps.push(Step::Machine(
        raw.get(..middle).ok_or("result prefix")?.to_vec(),
    ));
    steps.push(Step::Machine(
        raw.get(middle..).ok_or("result suffix")?.to_vec(),
    ));
    if protocol != Protocol::Autolyser {
        steps.push(ack);
    }
    if case == Case::LostAck {
        steps.push(Step::Machine(raw));
        if protocol == Protocol::Vidas {
            return Ok(steps);
        }
        steps.push(match protocol {
            Protocol::Law => Step::Mllp,
            Protocol::Poct => Step::Escape,
            Protocol::Vidas => Step::Host(vec![6]),
            Protocol::Autolyser => return Err("Autolyser lost ACK handled before command".into()),
        });
    }
    if protocol == Protocol::Poct && case == Case::LostAck {
        steps.push(Step::End);
    }
    if protocol == Protocol::Vidas {
        steps.push(Step::Machine(vec![3, 4]));
    }
    Ok(steps)
}
