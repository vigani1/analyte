//! Fixed-width records from Rev18 pp. 175–179. No identity or numeric normalization.
use thiserror::Error;

/// Specific record defect; display never contains patient data.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    /// Wrong field or record width.
    #[error("incorrect width")]
    Length,
    /// Invalid field repertoire or discriminator.
    #[error("invalid field: {0}")]
    Field(&'static str),
    /// Decimal checksum differs from the payload sum.
    #[error("checksum mismatch")]
    Checksum,
    /// Count exceeds the wire representation.
    #[error("count outside wire bound")]
    Count,
}

/// Exact printable ASCII field, without trimming or truncation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field<const N: usize>([u8; N]);

impl<const N: usize> Field<N> {
    /// Checks exact width and printable ASCII.
    /// # Errors
    /// Rejects wrong widths and nonprintable bytes.
    pub fn new(bytes: &[u8]) -> Result<Self, Error> {
        if !bytes.iter().all(|b| (32..=126).contains(b)) {
            return Err(Error::Field("ASCII"));
        }
        Ok(Self(bytes.try_into().map_err(|_| Error::Length)?))
    }

    /// Returns the original bytes, including spaces.
    #[must_use]
    pub fn bytes(&self) -> &[u8; N] {
        &self.0
    }
}

/// Fifteen-byte patient identity; never automatically trimmed.
pub type PatientCode = Field<15>;

impl PatientCode {
    /// Builds the documented eight-character identity followed by seven spaces.
    /// # Errors
    /// Rejects anything other than eight printable ASCII bytes.
    pub fn from_eight(bytes: &[u8]) -> Result<Self, Error> {
        let first = Field::<8>::new(bytes)?;
        let mut value = first.bytes().to_vec();
        value.extend_from_slice(b"       ");
        Self::new(&value)
    }
}

/// Internal protocol list codes, distinct from variable-protocol codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum List {
    /// Routine list, T on the wire.
    Routine,
    /// STAT list, R on the wire.
    Stat,
}

impl List {
    fn byte(self) -> u8 {
        match self {
            Self::Routine => b'T',
            Self::Stat => b'R',
        }
    }
    fn parse(b: u8) -> Result<Self, Error> {
        match b {
            b'T' => Ok(Self::Routine),
            b'R' => Ok(Self::Stat),
            _ => Err(Error::Field("list")),
        }
    }
}

/// Sample medium exactly as documented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sample {
    /// Serum.
    Serum,
    /// Urine.
    Urine,
}

impl Sample {
    fn byte(self) -> u8 {
        match self {
            Self::Serum => b'S',
            Self::Urine => b'U',
        }
    }
    fn parse(b: u8) -> Result<Self, Error> {
        match b {
            b'S' => Ok(Self::Serum),
            b'U' => Ok(Self::Urine),
            _ => Err(Error::Field("sample")),
        }
    }
}

/// Sum modulo 256, excluding framing and the checksum digits (Rev18 p.179).
#[must_use]
pub fn checksum(payload: &[u8]) -> u8 {
    payload.iter().fold(0u8, |sum, b| sum.wrapping_add(*b))
}

fn finish(mut payload: Vec<u8>) -> Vec<u8> {
    payload.extend_from_slice(format!("{:03}", checksum(&payload)).as_bytes());
    payload.push(4);
    payload
}

fn verified(raw: &[u8]) -> Result<&[u8], Error> {
    if raw.len() > 11038 || raw.last() != Some(&4) {
        return Err(Error::Length);
    }
    let end = raw.len().checked_sub(4).ok_or(Error::Length)?;
    let payload = raw.get(..end).ok_or(Error::Length)?;
    let digits = raw
        .get(end..raw.len().saturating_sub(1))
        .ok_or(Error::Length)?;
    if number(digits)? != usize::from(checksum(payload)) {
        return Err(Error::Checksum);
    }
    Ok(payload)
}

fn number(bytes: &[u8]) -> Result<usize, Error> {
    bytes.iter().try_fold(0usize, |n, b| {
        if !b.is_ascii_digit() {
            return Err(Error::Field("decimal"));
        }
        n.checked_mul(10)
            .and_then(|v| v.checked_add(usize::from(b.saturating_sub(b'0'))))
            .ok_or(Error::Count)
    })
}

struct Cursor<'a>(&'a [u8]);
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let head = self.0.get(..n).ok_or(Error::Length)?;
        self.0 = self.0.get(n..).ok_or(Error::Length)?;
        Ok(head)
    }
    fn byte(&mut self) -> Result<u8, Error> {
        self.take(1)?.first().copied().ok_or(Error::Length)
    }
}

/// One worklist insertion, bounded by the two-digit count and cup position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worklist {
    patient: PatientCode,
    list: List,
    sample: Sample,
    clone: bool,
    position: u8,
    tests: Vec<Field<4>>,
}

impl Worklist {
    /// Returns the exact patient field without trimming its padding.
    #[must_use]
    pub fn patient(&self) -> &PatientCode {
        &self.patient
    }

    /// Returns the internal routine/STAT list discriminator.
    #[must_use]
    pub fn list(&self) -> List {
        self.list
    }

    /// Returns the sample medium.
    #[must_use]
    pub fn sample(&self) -> Sample {
        self.sample
    }

    /// Returns whether cloning was explicitly requested.
    #[must_use]
    pub fn is_clone(&self) -> bool {
        self.clone
    }

    /// Returns the requested cup position; zero means analyzer-assigned.
    #[must_use]
    pub fn position(&self) -> u8 {
        self.position
    }

    /// Returns analysis codes in their original order, including duplicates.
    #[must_use]
    pub fn tests(&self) -> &[Field<4>] {
        &self.tests
    }

    /// Creates an insertion; position zero asks the analyzer to assign one.
    /// # Errors
    /// Rejects positions above 99 and test counts outside 1–99.
    pub fn new(
        patient: PatientCode,
        list: List,
        sample: Sample,
        clone: bool,
        position: u8,
        tests: Vec<Field<4>>,
    ) -> Result<Self, Error> {
        if position > 99 {
            return Err(Error::Field("position"));
        }
        if tests.is_empty() || tests.len() > 99 {
            return Err(Error::Count);
        }
        Ok(Self {
            patient,
            list,
            sample,
            clone,
            position,
            tests,
        })
    }

    /// Serializes payload, decimal checksum and EOT; no STX or command letter.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = self.patient.bytes().to_vec();
        out.extend_from_slice(&[
            self.list.byte(),
            self.sample.byte(),
            if self.clone { b'Y' } else { b'N' },
        ]);
        out.extend_from_slice(format!("{:02}{:02}", self.position, self.tests.len()).as_bytes());
        for test in &self.tests {
            out.extend_from_slice(test.bytes());
        }
        finish(out)
    }

    /// Parses after checking length and checksum, retaining every fixed-width field.
    /// # Errors
    /// Reports checksum, count, length or field defects.
    pub fn parse(raw: &[u8]) -> Result<Self, Error> {
        let mut c = Cursor(verified(raw)?);
        let patient = PatientCode::new(c.take(15)?)?;
        let list = List::parse(c.byte()?)?;
        let sample = Sample::parse(c.byte()?)?;
        let clone = match c.byte()? {
            b'Y' => true,
            b'N' => false,
            _ => return Err(Error::Field("clone")),
        };
        let position = u8::try_from(number(c.take(2)?)?).map_err(|_| Error::Count)?;
        let count = number(c.take(2)?)?;
        let mut tests = Vec::new();
        for _ in 0..count {
            tests.push(Field::new(c.take(4)?)?);
        }
        if !c.0.is_empty() {
            return Err(Error::Length);
        }
        Self::new(patient, list, sample, clone, position, tests)
    }
}

/// Controls-enabled header; ordinary samples retain their documented control fields too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    /// True only for Y; never infer QC from level or patient identity.
    pub is_control: bool,
    /// True for K, false for U.
    pub known: bool,
    /// Exact three decimal digits.
    pub level: Field<3>,
    /// Exact twenty-byte lot field.
    pub lot: Field<20>,
}

/// One analysis code and its uninterpreted seven-byte value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    /// Exact four-byte analysis code.
    pub code: Field<4>,
    /// Exact value; no numeric assumptions, units or flags added.
    pub text: Field<7>,
}

/// Parsed report with exact original response retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Original response including checksum and EOT.
    pub raw: Vec<u8>,
    /// Exact patient field.
    pub patient: PatientCode,
    /// Internal list discriminator.
    pub list: List,
    /// Sample medium.
    pub sample: Sample,
    /// Present only when controls mode was explicitly selected.
    pub control: Option<Control>,
    /// Ordered results, including repeated codes.
    pub values: Vec<Value>,
}

impl Report {
    /// Parses the configured layout only after validating its checksum.
    /// # Errors
    /// Rejects length, checksum, count and field defects; callers must store raw first.
    pub fn parse(raw: &[u8], controls: bool) -> Result<Self, Error> {
        let mut c = Cursor(verified(raw)?);
        let patient = PatientCode::new(c.take(15)?)?;
        let list = List::parse(c.byte()?)?;
        let sample = Sample::parse(c.byte()?)?;
        let control = if controls {
            let is_control = match c.byte()? {
                b'Y' => true,
                b'N' => false,
                _ => return Err(Error::Field("control")),
            };
            let known = match c.byte()? {
                b'K' => true,
                b'U' => false,
                _ => return Err(Error::Field("known")),
            };
            let level = Field::new(c.take(3)?)?;
            number(level.bytes())?;
            let lot = Field::new(c.take(20)?)?;
            if !is_control
                && (!known || level.bytes() != b"001" || lot.bytes() != b"                    ")
            {
                return Err(Error::Field("ordinary control fields"));
            }
            Some(Control {
                is_control,
                known,
                level,
                lot,
            })
        } else {
            None
        };
        let count = number(c.take(3)?)?;
        let mut values = Vec::new();
        for _ in 0..count {
            values.push(Value {
                code: Field::new(c.take(4)?)?,
                text: Field::new(c.take(7)?)?,
            });
        }
        if !c.0.is_empty() {
            return Err(Error::Length);
        }
        Ok(Self {
            raw: raw.to_vec(),
            patient,
            list,
            sample,
            control,
            values,
        })
    }

    /// Builds a report from typed fields, checking bounds and control invariants.
    /// # Errors
    /// Rejects counts above 999 and inconsistent control fields.
    pub fn build(
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
        Self::parse(&finish(out), control.is_some())
    }
}
