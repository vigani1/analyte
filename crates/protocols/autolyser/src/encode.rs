//! Encodes Autolyser worklist and report record bodies from typed fields (Rev18 pp.175–179).
use crate::syntax::{Control, Error, List, PatientCode, Report, Sample, Value, Worklist};

impl List {
    fn byte(self) -> u8 {
        match self {
            Self::Routine => b'T',
            Self::Stat => b'R',
        }
    }
}

impl Sample {
    fn byte(self) -> u8 {
        match self {
            Self::Serum => b'S',
            Self::Urine => b'U',
        }
    }
}

impl Worklist {
    /// Serializes the record body; framing adds its checksum and EOT.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = self.patient().bytes().to_vec();
        out.extend_from_slice(&[
            self.list().byte(),
            self.sample().byte(),
            if self.is_clone() { b'Y' } else { b'N' },
        ]);
        out.extend_from_slice(
            format!("{:02}{:02}", self.position(), self.tests().len()).as_bytes(),
        );
        for test in self.tests() {
            out.extend_from_slice(test.bytes());
        }
        out
    }
}

impl Report {
    /// Builds a report from typed fields, checking bounds and control invariants.
    /// # Errors
    /// Rejects counts above 999 and inconsistent control fields.
    pub fn new(
        patient: &PatientCode,
        list: List,
        sample: Sample,
        control: Option<&Control>,
        values: &[Value],
    ) -> Result<Self, Error> {
        if values.len() > 999 {
            return Err(Error::Count);
        }
        let mut out = patient.bytes().to_vec();
        out.extend_from_slice(&[list.byte(), sample.byte()]);
        if let Some(c) = control {
            out.extend_from_slice(&[
                if c.is_control { b'Y' } else { b'N' },
                if c.known { b'K' } else { b'U' },
            ]);
            out.extend_from_slice(c.level.bytes());
            out.extend_from_slice(c.lot.bytes());
        }
        out.extend_from_slice(format!("{:03}", values.len()).as_bytes());
        for v in values {
            out.extend_from_slice(v.code.bytes());
            out.extend_from_slice(v.text.bytes());
        }
        Self::parse(&out, control.is_some())
    }
}
