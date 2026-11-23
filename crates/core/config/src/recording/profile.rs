//! Holds the shipped capture profiles: the link rules of each protocol a record-only connection speaks (ADR 0034).
use crate::{AckPoint, ChecksumAcceptance, ChecksumInclusion, Error, HexCase};
use crate::{Terminator, invalid};
use serde::{Deserialize, Serialize};

/// A protocol a record-only connection can speak; each has one shipped profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureProtocol {
    /// ASTM: LIS01-A2 link layer only.
    Astm,
    /// HL7 v2 over MLLP: blocks and acknowledgments only.
    Hl7,
    /// VIDAS Literal: upload packets and their acknowledgment only.
    Vidas,
    /// Dialab Autolyser: next-report polls only.
    Autolyser,
}

impl CaptureProtocol {
    /// The definition's `protocol` key for this protocol.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Astm => "astm",
            Self::Hl7 => "hl7",
            Self::Vidas => "vidas",
            Self::Autolyser => "autolyser",
        }
    }

    /// Reads a definition's `protocol` key; LAW and POCT have no capture profile yet.
    pub(crate) fn of(key: &str, path: &str) -> Result<Self, Error> {
        match key {
            "astm" => Ok(Self::Astm),
            "hl7" => Ok(Self::Hl7),
            "vidas" => Ok(Self::Vidas),
            "autolyser" => Ok(Self::Autolyser),
            _ => Err(invalid(
                path,
                "protocol",
                "record only has a capture profile for ASTM, HL7, VIDAS and Autolyser; this protocol cannot record yet",
            )),
        }
    }

    /// Whether this protocol runs over the transport `kind`. HL7 MLLP Release 1
    /// runs over TCP; a watched folder has no conversation to keep alive.
    pub(crate) const fn supports(self, kind: crate::TransportKind) -> bool {
        use crate::TransportKind;
        match (self, kind) {
            (_, TransportKind::WatchedFolder) | (Self::Hl7, TransportKind::Serial) => false,
            (
                Self::Astm | Self::Vidas | Self::Autolyser,
                TransportKind::Serial | TransportKind::TcpClient | TransportKind::TcpServer,
            )
            | (Self::Hl7, TransportKind::TcpClient | TransportKind::TcpServer) => true,
        }
    }

    fn text(self) -> &'static str {
        match self {
            Self::Astm => include_str!("profiles/astm.toml"),
            Self::Hl7 => include_str!("profiles/hl7.toml"),
            Self::Vidas => include_str!("profiles/vidas.toml"),
            Self::Autolyser => include_str!("profiles/autolyser.toml"),
        }
    }

    /// The shipped profile of this protocol, read from its embedded file.
    pub(crate) fn profile(self) -> Result<Profile, Error> {
        let path = format!("capture profile {}", self.key());
        let profile: Profile = toml::from_str(self.text()).map_err(|source| Error::Schema {
            path: path.clone(),
            source,
        })?;
        if profile.protocol() != self {
            return Err(invalid(&path, "protocol", "profile names another protocol"));
        }
        profile.check(&path)?;
        Ok(profile)
    }
}

/// One shipped capture profile, exactly as its file writes it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "protocol", rename_all = "snake_case")]
pub(crate) enum Profile {
    Astm(AstmCapture),
    Hl7(Hl7Capture),
    Vidas(VidasProfile),
    Autolyser(AutolyserProfile),
}

impl Profile {
    const fn protocol(&self) -> CaptureProtocol {
        match self {
            Self::Astm(_) => CaptureProtocol::Astm,
            Self::Hl7(_) => CaptureProtocol::Hl7,
            Self::Vidas(_) => CaptureProtocol::Vidas,
            Self::Autolyser(_) => CaptureProtocol::Autolyser,
        }
    }

    pub(crate) const fn operation_timeout_ms(&self) -> u32 {
        match self {
            Self::Astm(p) => p.operation_timeout_ms,
            Self::Hl7(p) => p.operation_timeout_ms,
            Self::Vidas(p) => p.operation_timeout_ms,
            Self::Autolyser(p) => p.operation_timeout_ms,
        }
    }

    /// Refuses a zero deadline or bound, or a profile beyond the transport cap.
    fn check(&self, path: &str) -> Result<(), Error> {
        let (deadlines, bounds): (Vec<u32>, Vec<usize>) = match self {
            Self::Astm(p) => (
                vec![
                    p.operation_timeout_ms,
                    p.establish_ms,
                    p.contention_ms,
                    p.frame_ms,
                    p.receive_ms,
                    p.commit_ms,
                ],
                vec![p.frame_limit, p.message_bytes, p.frames],
            ),
            Self::Hl7(p) => (
                vec![p.operation_timeout_ms, p.wait_ms],
                vec![p.message_bytes, p.segments, p.fields, p.repetitions],
            ),
            Self::Vidas(p) => (
                vec![
                    p.operation_timeout_ms,
                    p.acknowledgment_ms,
                    p.interbyte_ms,
                    p.session_ms,
                    p.commit_ms,
                ],
                vec![p.chunk, p.frame_limit],
            ),
            Self::Autolyser(p) => (
                vec![
                    p.operation_timeout_ms,
                    p.acknowledgment_ms,
                    p.response_ms,
                    p.interbyte_ms,
                    p.storage_ms,
                ],
                vec![p.frame_limit],
            ),
        };
        if deadlines.iter().any(|ms| !(1..=60_000).contains(ms)) || bounds.contains(&0) {
            return Err(invalid(
                path,
                "profile",
                "every deadline is 1 to 60000 milliseconds and every bound above zero",
            ));
        }
        Ok(())
    }
}

/// The ASTM capture profile: LIS01-A2 link settings and Analyte's transfer bounds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AstmCapture {
    operation_timeout_ms: u32,
    frame_limit: usize,
    establish_ms: u32,
    contention_ms: u32,
    frame_ms: u32,
    receive_ms: u32,
    commit_ms: u32,
    message_bytes: usize,
    frames: usize,
}

impl AstmCapture {
    /// Most bytes in one frame, overhead included (LIS01-A2 6.3.1).
    #[must_use]
    pub const fn frame_limit(&self) -> usize {
        self.frame_limit
    }
    /// ENQ reply budget in milliseconds (LIS01-A2 6.5.2.1).
    #[must_use]
    pub const fn establish_ms(&self) -> u32 {
        self.establish_ms
    }
    /// Wait for the instrument's ENQ after contention (LIS01-A2 6.5.2.2).
    #[must_use]
    pub const fn contention_ms(&self) -> u32 {
        self.contention_ms
    }
    /// Frame reply budget as sender (LIS01-A2 6.5.2.3).
    #[must_use]
    pub const fn frame_ms(&self) -> u32 {
        self.frame_ms
    }
    /// Receiver silence budget (LIS01-A2 6.5.2.4).
    #[must_use]
    pub const fn receive_ms(&self) -> u32 {
        self.receive_ms
    }
    /// Durable store budget before a frame reply (LIS01-A2 6.5.2.5).
    #[must_use]
    pub const fn commit_ms(&self) -> u32 {
        self.commit_ms
    }
    /// Most bytes kept for one transfer.
    #[must_use]
    pub const fn message_bytes(&self) -> usize {
        self.message_bytes
    }
    /// Most frames in one transfer.
    #[must_use]
    pub const fn frames(&self) -> usize {
        self.frames
    }
}

/// The HL7 capture profile: MLLP block bounds and the parsing options an
/// acknowledgment needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hl7Capture {
    operation_timeout_ms: u32,
    terminator: Terminator,
    wait_ms: u32,
    message_bytes: usize,
    segments: usize,
    fields: usize,
    repetitions: usize,
}

impl Hl7Capture {
    /// Segment terminator.
    #[must_use]
    pub const fn terminator(&self) -> Terminator {
        self.terminator
    }
    /// Budget for storing and answering one message, in milliseconds.
    #[must_use]
    pub const fn wait_ms(&self) -> u32 {
        self.wait_ms
    }
    /// Most bytes in one message.
    #[must_use]
    pub const fn message_bytes(&self) -> usize {
        self.message_bytes
    }
    /// Most segments in one message.
    #[must_use]
    pub const fn segments(&self) -> usize {
        self.segments
    }
    /// Most fields in one segment.
    #[must_use]
    pub const fn fields(&self) -> usize {
        self.fields
    }
    /// Most repetitions in one field.
    #[must_use]
    pub const fn repetitions(&self) -> usize {
        self.repetitions
    }
}

/// The VIDAS profile file; the acknowledgment point comes from the site.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VidasProfile {
    operation_timeout_ms: u32,
    chunk: usize,
    frame_limit: usize,
    suffix: Vec<u8>,
    include_suffix: ChecksumInclusion,
    include_stx: ChecksumInclusion,
    uppercase: HexCase,
    accept_either_case: ChecksumAcceptance,
    acknowledgment_ms: u32,
    interbyte_ms: u32,
    session_ms: u32,
    commit_ms: u32,
}

/// The VIDAS capture settings in use: the profile and the lab's
/// acknowledgment point.
#[derive(Debug, Clone)]
pub struct VidasCapture {
    profile: VidasProfile,
    ack_point: AckPoint,
}

impl VidasCapture {
    pub(crate) const fn new(profile: VidasProfile, ack_point: AckPoint) -> Self {
        Self { profile, ack_point }
    }
    /// Literal (after the checksum) or Alternate (after ETX), as the lab chose.
    #[must_use]
    pub const fn ack_point(&self) -> AckPoint {
        self.ack_point
    }
    /// Most bytes in one RS chunk.
    #[must_use]
    pub const fn chunk(&self) -> usize {
        self.profile.chunk
    }
    /// Most payload bytes in one packet.
    #[must_use]
    pub const fn frame_limit(&self) -> usize {
        self.profile.frame_limit
    }
    /// Control suffix bytes.
    #[must_use]
    pub fn suffix(&self) -> &[u8] {
        &self.profile.suffix
    }
    /// Whether the suffix is in the checksum.
    #[must_use]
    pub const fn include_suffix(&self) -> ChecksumInclusion {
        self.profile.include_suffix
    }
    /// Whether STX is in the checksum.
    #[must_use]
    pub const fn include_stx(&self) -> ChecksumInclusion {
        self.profile.include_stx
    }
    /// Case of emitted checksum digits.
    #[must_use]
    pub const fn hex_case(&self) -> HexCase {
        self.profile.uppercase
    }
    /// Whether received checksum digits may be either case.
    #[must_use]
    pub const fn checksum_acceptance(&self) -> ChecksumAcceptance {
        self.profile.accept_either_case
    }
    /// Peer acknowledgment deadline in milliseconds.
    #[must_use]
    pub const fn acknowledgment_ms(&self) -> u32 {
        self.profile.acknowledgment_ms
    }
    /// Silence allowed between bytes, in milliseconds.
    #[must_use]
    pub const fn interbyte_ms(&self) -> u32 {
        self.profile.interbyte_ms
    }
    /// Whole conversation deadline in milliseconds.
    #[must_use]
    pub const fn session_ms(&self) -> u32 {
        self.profile.session_ms
    }
    /// Durable storage deadline in milliseconds.
    #[must_use]
    pub const fn commit_ms(&self) -> u32 {
        self.profile.commit_ms
    }
}

/// The Autolyser profile file; the poll interval comes from the site.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AutolyserProfile {
    operation_timeout_ms: u32,
    frame_limit: usize,
    acknowledgment_ms: u32,
    response_ms: u32,
    interbyte_ms: u32,
    storage_ms: u32,
}

/// The Autolyser capture settings in use: the profile and the lab's poll interval.
#[derive(Debug, Clone)]
pub struct AutolyserCapture {
    profile: AutolyserProfile,
    poll_ms: u32,
    controls: bool,
}

impl AutolyserCapture {
    pub(crate) const fn new(profile: AutolyserProfile, poll_ms: u32, controls: bool) -> Self {
        Self {
            profile,
            poll_ms,
            controls,
        }
    }
    /// Whether reports carry the controls block, as the lab answered.
    #[must_use]
    pub const fn controls(&self) -> bool {
        self.controls
    }
    /// Time between next-report (R) requests, as the lab chose.
    #[must_use]
    pub const fn poll_ms(&self) -> u32 {
        self.poll_ms
    }
    /// Most bytes in one response, checksum and EOT included.
    #[must_use]
    pub const fn frame_limit(&self) -> usize {
        self.profile.frame_limit
    }
    /// Initialization acknowledgment deadline in milliseconds.
    #[must_use]
    pub const fn acknowledgment_ms(&self) -> u32 {
        self.profile.acknowledgment_ms
    }
    /// Whole response deadline in milliseconds.
    #[must_use]
    pub const fn response_ms(&self) -> u32 {
        self.profile.response_ms
    }
    /// Silence allowed within a response, in milliseconds.
    #[must_use]
    pub const fn interbyte_ms(&self) -> u32 {
        self.profile.interbyte_ms
    }
    /// Durable storage deadline in milliseconds.
    #[must_use]
    pub const fn storage_ms(&self) -> u32 {
        self.profile.storage_ms
    }
}
