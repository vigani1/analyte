//! Creates response timestamps and IDs for host-generated HL7 replies.
use analyte_hl7::Error;

#[derive(Clone, Copy)]
pub(crate) enum Profile {
    Hl7,
    Hl7Numeric,
    Law,
}

pub(crate) struct Metadata {
    id: String,
    pub(crate) timestamp: String,
    profile: Profile,
}

pub(crate) fn response_metadata(profile: Profile) -> Metadata {
    let uuid = uuid::Uuid::now_v7();
    Metadata {
        id: match profile {
            Profile::Hl7 => uuid.simple().to_string().chars().skip(12).collect(),
            // YHLO LIS V1.1 section 2.3.1 describes a numeric MSH-10.
            Profile::Hl7Numeric => format!("{:020}", uuid.as_u64_pair().1),
            Profile::Law => uuid.simple().to_string(),
        },
        timestamp: jiff::Timestamp::now()
            .strftime("%Y%m%d%H%M%S%z")
            .to_string(),
        profile,
    }
}
impl Metadata {
    pub(crate) fn response(&self) -> Result<analyte_hl7::Response<'_>, Error> {
        Ok(analyte_hl7::Response {
            control_id: match self.profile {
                Profile::Hl7 | Profile::Hl7Numeric => {
                    analyte_hl7::ControlId::new(self.id.as_bytes())?
                }
                Profile::Law => analyte_hl7::law::control_id(self.id.as_bytes())?,
            },
            timestamp: self.timestamp.as_bytes(),
        })
    }
    pub(crate) fn build<T, E: From<Error>>(
        &self,
        build: impl FnOnce(analyte_hl7::Response<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        build(self.response()?)
    }
}
