//! Validates lab app destinations kept after the last machine using them was removed.
use super::Error;

/// A lab app destination kept in the program file when a machine using it was
/// removed. Results and order messages still waiting for it are delivered to it;
/// its orders are never read. A set-up machine that uses the same name wins.
pub struct Retired {
    pub(crate) settings: analyte_config::Destination,
    pub(crate) limits: analyte_sink::Limits,
}

impl Retired {
    /// Borrows the kept destination settings: name, address, key names and deadlines.
    #[must_use]
    pub fn settings(&self) -> &analyte_config::Destination {
        &self.settings
    }
}

/// Checks each kept destination with the same rules as a site file's `[destination]`.
pub(super) fn validate(
    listed: Vec<analyte_config::Destination>,
    path: &std::path::Path,
) -> Result<Vec<Retired>, Error> {
    if listed.len() > 128 {
        return Err(Error::TooManyRetiredDestinations);
    }
    let mut retired: Vec<Retired> = Vec::new();
    for settings in listed {
        if retired
            .iter()
            .any(|kept| kept.settings.name == settings.name)
        {
            return Err(Error::DuplicateRetiredDestination(
                settings.name.as_str().into(),
            ));
        }
        settings.check(&path.display().to_string(), "retired_destinations")?;
        retired.push(Retired {
            limits: crate::runtime::delivery::limits(&settings),
            settings,
        });
    }
    Ok(retired)
}
