//! Validates configuration mappings and protocol stacks.
use crate::ParsedMachine;
use crate::{Error, invalid, schema::*};

pub(super) fn validate_site(site: &Site, transport: &Transport, path: &str) -> Result<(), Error> {
    if site.destination.freshness_seconds == 0 {
        return Err(invalid(
            path,
            "site.destination.freshness_seconds",
            "must be at least one second",
        ));
    }
    let endpoint = &site.endpoint;
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

mod mappings;
use mappings::*;

mod stack;
use stack::*;

mod departures;
use departures::validate_departures;
