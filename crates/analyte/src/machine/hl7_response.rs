//! Host-generated response metadata; protocol crates never read clocks or create IDs.
use crate::Error;

#[derive(Clone, Copy)]
pub(crate) enum Profile {
    Hl7,
    Law,
}

pub(crate) struct Metadata {
    id: String,
    pub(crate) timestamp: String,
    profile: Profile,
}

pub(crate) fn response_metadata(profile: Profile) -> Metadata {
    let id = uuid::Uuid::now_v7().simple().to_string();
    Metadata {
        id: match profile {
            Profile::Hl7 => id.chars().skip(12).collect(),
            Profile::Law => id,
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
                Profile::Hl7 => analyte_hl7::ControlId::new(self.id.as_bytes())?,
                Profile::Law => analyte_hl7::law::control_id(self.id.as_bytes())?,
            },
            timestamp: self.timestamp.as_bytes(),
        })
    }
    pub(crate) fn build<T>(
        &self,
        build: impl FnOnce(analyte_hl7::Response<'_>) -> Result<T, Error>,
    ) -> Result<T, Error> {
        build(self.response()?)
    }
}
