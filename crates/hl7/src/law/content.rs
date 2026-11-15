//! App-supplied enhanced work content. Absence never creates a default value.

/// Encoded patient/encounter fields supplied by the app, checked when rendered.
#[derive(Debug, Clone, Copy)]
pub struct PatientInfo<'a> {
    /// Encoded FT comments supplied by the app, not executable rules.
    pub notes: &'a [&'a [u8]],
    /// PID-3 identifier list with assigning authorities.
    pub identifiers: &'a [u8],
    /// PID-5 name, or an explicit null supplied by the app.
    pub name: &'a [u8],
    /// PID-7 birth timestamp.
    pub birth: Option<&'a [u8]>,
    /// PID-8 sex code.
    pub sex: Option<&'a [u8]>,
    /// PID-10 coded race.
    pub race: Option<&'a [u8]>,
    /// PID-35 coded taxonomic classification.
    pub taxonomy: Option<&'a [u8]>,
    /// Encounter supplied by the app; absence omits the visit group.
    pub visit: Option<Visit<'a>>,
}

/// Encoded FT comments at the four work-group owners; no scope is inferred.
#[derive(Debug, Clone, Copy, Default)]
pub struct WorkNotes<'a> {
    /// Comments belonging to the specimen.
    pub specimen: &'a [&'a [u8]],
    /// Comments belonging to the container.
    pub container: &'a [&'a [u8]],
    /// Comments belonging to the common order.
    pub order: &'a [&'a [u8]],
    /// Comments belonging to the observation request.
    pub request: &'a [&'a [u8]],
}

/// Explicit encounter class and location; no unknown class is synthesized.
#[derive(Debug, Clone, Copy)]
pub struct Visit<'a> {
    /// PV1-2 patient class from the app.
    pub class: &'a [u8],
    /// PV1-3 assigned patient location when available.
    pub location: Option<&'a [u8]>,
}

/// Additional specimen fields supplied by the app; source identity stays separate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpecimenDetails<'a> {
    /// SPM-3 parent specimen identifiers.
    pub parents: Option<&'a [u8]>,
    /// SPM-7 collection method.
    pub collection_method: Option<&'a [u8]>,
    /// SPM-8 source site.
    pub source_site: Option<&'a [u8]>,
    /// SPM-9 source site modifier.
    pub source_modifier: Option<&'a [u8]>,
    /// SPM-16 risk code.
    pub risk: Option<&'a [u8]>,
    /// SPM-17 collection start timestamp; LAW excludes the range-end component.
    pub collection_time: Option<&'a [u8]>,
    /// SPM-18 receipt timestamp.
    pub receipt_time: Option<&'a [u8]>,
    /// SPM-27 container type.
    pub container_type: Option<&'a [u8]>,
}

/// Additional container fields; volume units and dilution remain exactly supplied.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContainerDetails<'a> {
    /// SAC-4 parent container.
    pub parent: Option<&'a [u8]>,
    /// SAC-9 carrier type.
    pub carrier_type: Option<&'a [u8]>,
    /// SAC-10 carrier identity.
    pub carrier: Option<&'a [u8]>,
    /// SAC-11 position in carrier.
    pub carrier_position: Option<&'a [u8]>,
    /// SAC-13 tray identity.
    pub tray: Option<&'a [u8]>,
    /// SAC-14 position in tray.
    pub tray_position: Option<&'a [u8]>,
    /// SAC-15 coded location.
    pub location: Option<&'a [u8]>,
    /// SAC-21 container volume.
    pub container_volume: Option<&'a [u8]>,
    /// SAC-22 available specimen volume.
    pub specimen_volume: Option<&'a [u8]>,
    /// SAC-24 volume units.
    pub volume_units: Option<&'a [u8]>,
    /// SAC-29 dilution factor.
    pub dilution: Option<&'a [u8]>,
}

/// Order context supplied by the app; these fields never authorize a new test.
#[derive(Debug, Clone, Copy, Default)]
pub struct OrderDetails<'a> {
    /// ORC-4 placer group, linking work without changing individual AWOS identity.
    pub group: Option<&'a [u8]>,
    /// ORC-21 ordering facility.
    pub facility: Option<&'a [u8]>,
    /// OBR-16 ordering provider.
    pub provider: Option<&'a [u8]>,
    /// OBR-49 requested result handling.
    pub result_handling: Option<&'a [u8]>,
}
