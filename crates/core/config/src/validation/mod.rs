//! Validates configuration mappings and protocol stacks.
use crate::ParsedMachine;
use crate::schema::law::validate;
use crate::{
    Encoding, Error, EvidenceLevel, Field, MachineDocument, Mappings, NoSubId, OrderNamespace,
    Site, SubId, TestMapping, Transport, TransportKind, UnitMapping, invalid,
};
mod departures;
mod mappings;
mod stack;
use departures::validate_departures;
use mappings::{validate_mapping_family, validate_mappings};
use stack::{
    validate_dialog_and_framing, validate_protocol, validate_stack, validate_syntax,
    validate_transport,
};

impl crate::Destination {
    /// Checks the bounds every destination keeps wherever it is written: in a
    /// site file, or kept in the program file after its machine was removed.
    /// `table` names where it is written (`site.destination`) for the error.
    /// # Errors
    /// Refuses a zero order freshness or a request deadline outside 1 through 120 seconds.
    pub fn check(&self, path: &str, table: &str) -> Result<(), Error> {
        if self.freshness_seconds == 0 {
            return Err(invalid(
                path,
                &format!("{table}.freshness_seconds"),
                "must be at least one second",
            ));
        }
        if !Self::REQUEST_TIMEOUT_SECONDS.contains(&self.request_timeout_seconds) {
            return Err(invalid(
                path,
                &format!("{table}.request_timeout_seconds"),
                "must be 1 through 120 seconds",
            ));
        }
        Ok(())
    }
}

pub(super) fn validate_site(site: &Site, transport: &Transport, path: &str) -> Result<(), Error> {
    site.destination.check(path, "site.destination")?;
    let endpoint = match &site.endpoint {
        crate::Endpoint::Fixed(endpoint) => endpoint,
        crate::Endpoint::UsbSerial(_) if transport.kind == TransportKind::Serial => return Ok(()),
        crate::Endpoint::UsbSerial(_) => {
            return Err(invalid(
                path,
                "usb_serial",
                "a USB serial number selects a serial port; TCP and folder links need endpoint",
            ));
        }
    };
    if endpoint.is_empty() || endpoint.len() > 1024 || endpoint.chars().any(char::is_control) {
        return Err(invalid(
            path,
            "site.endpoint",
            "supply a nonempty endpoint of at most 1024 bytes without control characters",
        ));
    }
    match transport.kind {
        TransportKind::TcpClient | TransportKind::TcpServer => {
            let address = endpoint.parse::<std::net::SocketAddr>().map_err(|_| {
                invalid(
                    path,
                    "site.endpoint",
                    "supply an explicit numeric IP address and nonzero port",
                )
            })?;
            if address.port() == 0
                || (transport.kind == TransportKind::TcpClient && address.ip().is_unspecified())
            {
                return Err(invalid(
                    path,
                    "site.endpoint",
                    "use a nonzero port and a concrete client destination",
                ));
            }
        }
        TransportKind::WatchedFolder => {
            if !std::path::Path::new(endpoint).is_absolute() {
                return Err(invalid(
                    path,
                    "site.endpoint",
                    "supply an absolute watched-folder path",
                ));
            }
        }
        TransportKind::Serial => {}
    }
    Ok(())
}

pub(super) fn validate_machine(
    document: &MachineDocument,
    path: &str,
    reference_path: &str,
) -> Result<crate::Stack, Error> {
    let parsed = ParsedMachine::parse(document, path)?;
    let m = &parsed;
    validate(m, path)?;
    if let Some(orders) = &m.orders {
        orders.validate(m, path)?;
    }
    validate_transport(m, path)?;
    validate_stack(m, path)?;
    validate_dialog_and_framing(m, path)?;
    validate_syntax(m, path)?;
    validate_departures(m, path, reference_path)?;
    validate_protocol(m, path)?;
    validate_mappings(m, path)?;
    validate_evidence(m, path)?;
    crate::Stack::from_parsed(m, path)
}

fn validate_evidence(m: &ParsedMachine<'_>, path: &str) -> Result<(), Error> {
    if m.deviations.len() > 128 || m.evidence.capabilities.len() > 128 {
        return Err(invalid(
            path,
            "evidence/deviations",
            "limit each list to 128 entries",
        ));
    }
    for capability in &m.evidence.capabilities {
        for list in [
            &capability.dialogs,
            &capability.failure_cases,
            &capability.evidence_files,
        ] {
            if list.len() > 128 {
                return Err(invalid(
                    path,
                    "evidence.capabilities",
                    "limit capability lists to 128 entries",
                ));
            }
        }
        if m.evidence.level != EvidenceLevel::Documented && capability.evidence_files.is_empty() {
            return Err(invalid(
                path,
                "evidence.capabilities.evidence_files",
                "simulated or proven claims require evidence file references",
            ));
        }
    }
    if m.evidence.capabilities.is_empty() {
        return Err(invalid(
            path,
            "evidence.capabilities",
            "declare at least one capability, including untested scope",
        ));
    }
    Ok(())
}
