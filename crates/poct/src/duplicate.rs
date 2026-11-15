//! Bounded duplicate detection. The host persists identities and evidence in the
//! same transaction as the result, and rebuilds this index before accepting data.
//! Receipt/control/delivery IDs never participate in the observation key.
use crate::{Message, MessageError, structure};
use analyte_model::{Identifier, IdentityStatus, ResultIdentity};

/// A trusted result identity scoped to the physical device, independent of receipts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationKey {
    /// Stable configured device identity, not a connection or receipt identity.
    pub device: Identifier,
    /// Trusted observation and Analyte revision resolved by host reconciliation.
    pub result: ResultIdentity,
}

#[derive(Clone, PartialEq, Eq)]
struct EvidenceNode {
    name: String,
    depth: usize,
    attributes: Vec<(String, String)>,
    text: String,
}

/// Semantic source evidence for one observation, excluding receipt headers,
/// retransmission reason and sibling observations. Debug omits patient content.
#[derive(Clone, PartialEq, Eq)]
pub struct ObservationEvidence {
    nodes: Vec<EvidenceNode>,
    bytes: usize,
}

impl std::fmt::Debug for ObservationEvidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObservationEvidence")
            .finish_non_exhaustive()
    }
}

impl ObservationEvidence {
    /// Retains this observation and its service context without its receipt ID.
    /// # Errors
    /// Rejects indices that do not identify an OBS inside the given service and
    /// arithmetic exhaustion. All copied data is bounded by the parsed message.
    pub fn from_message(
        message: &Message,
        service: usize,
        observation: usize,
    ) -> Result<Self, MessageError> {
        let document = message.document();
        let svc = document
            .elements()
            .get(service)
            .ok_or(MessageError::Structure)?;
        let obs = document
            .elements()
            .get(observation)
            .ok_or(MessageError::Structure)?;
        if svc.name() != "SVC" || obs.name() != "OBS" || !inside(document, observation, service) {
            return Err(MessageError::Structure);
        }
        let mut nodes = Vec::new();
        let mut bytes = 0usize;
        for (i, e) in document.elements().iter().enumerate() {
            if !inside(document, i, service) || e.name() == "SVC.reason_cd" {
                continue;
            }
            let mut ancestor = Some(i);
            let mut depth = 0usize;
            let mut sibling = false;
            while let Some(index) = ancestor {
                if index == service {
                    break;
                }
                let node = document
                    .elements()
                    .get(index)
                    .ok_or(MessageError::Structure)?;
                if node.name() == "OBS" && index != observation {
                    sibling = true;
                    break;
                }
                depth = depth.checked_add(1).ok_or(MessageError::Structure)?;
                ancestor = node.parent();
            }
            if sibling {
                continue;
            }
            let mut attributes = e.attributes().to_vec();
            // XML attribute order is not semantic; value spelling remains exact.
            attributes.sort();
            bytes = bytes
                .checked_add(e.name().len())
                .and_then(|n| {
                    n.checked_add(
                        if e.name().contains('.')
                            || matches!(
                                e.name(),
                                "NOTE" | "ORIGTXT" | "GIV" | "MID" | "FAM" | "PFX" | "SFX" | "DEL"
                            )
                        {
                            e.text().len()
                        } else {
                            0
                        },
                    )
                })
                .ok_or(MessageError::Structure)?;
            for (k, v) in &attributes {
                bytes = bytes
                    .checked_add(k.len())
                    .and_then(|n| n.checked_add(v.len()))
                    .ok_or(MessageError::Structure)?;
            }
            // Object indentation is not data; field text is retained verbatim.
            let text = if e.name().contains('.')
                || matches!(
                    e.name(),
                    "NOTE" | "ORIGTXT" | "GIV" | "MID" | "FAM" | "PFX" | "SFX" | "DEL"
                ) {
                e.text().to_owned()
            } else {
                String::new()
            };
            nodes.push(EvidenceNode {
                name: e.name().to_owned(),
                depth,
                attributes,
                text,
            });
        }
        Ok(Self { nodes, bytes })
    }
}

fn inside(document: &crate::Document, mut index: usize, parent: usize) -> bool {
    loop {
        if index == parent {
            return true;
        }
        let Some(next) = document
            .elements()
            .get(index)
            .and_then(crate::Element::parent)
        else {
            return false;
        };
        index = next;
    }
}

/// Stored index entry. The host's protected store owns durable serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedObservation {
    /// Trusted result key, including revision.
    pub key: ObservationKey,
    /// Immutable source evidence; conflict never overwrites it.
    pub evidence: ObservationEvidence,
}

/// Duplicate decisions never treat unresolved identities as new results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateStatus {
    /// This trusted identity and revision have not been recorded.
    New,
    /// Same trusted identity, revision and semantic evidence.
    Duplicate,
    /// Same trusted identity and revision, different source evidence: park both.
    Conflict,
    /// Insufficient source evidence to distinguish retransmission, correction or rerun.
    Unresolved,
}

/// Capacity failures require host backpressure or protected-store lookup.
#[derive(Debug, thiserror::Error)]
pub enum DuplicateError {
    /// Both bounds must be positive.
    #[error("duplicate index bounds must be positive")]
    Config,
    /// Index capacity is exhausted; entries are never evicted silently.
    #[error("duplicate index capacity exhausted")]
    Full,
}

/// A bounded pure index. This is not durable storage and cannot authorize an ACK.
#[derive(Debug)]
pub struct DuplicateIndex {
    maximum_entries: usize,
    maximum_bytes: usize,
    bytes: usize,
    entries: Vec<RecordedObservation>,
}

impl DuplicateIndex {
    /// Creates an empty bounded index, without hidden eviction behavior.
    /// # Errors
    /// Rejects zero bounds.
    pub fn new(maximum_entries: usize, maximum_bytes: usize) -> Result<Self, DuplicateError> {
        if maximum_entries == 0 || maximum_bytes == 0 {
            return Err(DuplicateError::Config);
        }
        Ok(Self {
            maximum_entries,
            maximum_bytes,
            bytes: 0,
            entries: Vec::new(),
        })
    }

    /// Looks up a trusted observation independently of receipt and delivery IDs.
    #[must_use]
    pub fn check(
        &self,
        device: &Identifier,
        identity: &IdentityStatus,
        evidence: &ObservationEvidence,
    ) -> DuplicateStatus {
        match identity {
            IdentityStatus::Unresolved(_) => DuplicateStatus::Unresolved,
            IdentityStatus::Resolved(result) => self.compare(
                &ObservationKey {
                    device: device.clone(),
                    result: result.clone(),
                },
                evidence,
            ),
        }
    }

    fn compare(&self, key: &ObservationKey, evidence: &ObservationEvidence) -> DuplicateStatus {
        match self.entries.iter().find(|entry| &entry.key == key) {
            Some(entry) if &entry.evidence == evidence => DuplicateStatus::Duplicate,
            Some(_) => DuplicateStatus::Conflict,
            None => DuplicateStatus::New,
        }
    }

    /// Adds already durable evidence, or restores one entry from the host store.
    /// The host must atomically compare/store identities before reporting custody.
    /// # Errors
    /// Returns Full without mutation; duplicate/conflicting evidence is never replaced.
    pub fn record(
        &mut self,
        record: RecordedObservation,
    ) -> Result<DuplicateStatus, DuplicateError> {
        let status = self.compare(&record.key, &record.evidence);
        if status != DuplicateStatus::New {
            return Ok(status);
        }
        let bytes = self
            .bytes
            .checked_add(record.evidence.bytes)
            .ok_or(DuplicateError::Full)?;
        if self.entries.len() >= self.maximum_entries || bytes > self.maximum_bytes {
            return Err(DuplicateError::Full);
        }
        self.entries.push(record);
        self.bytes = bytes;
        Ok(DuplicateStatus::New)
    }

    /// Returns retained evidence for host recovery tests and protected persistence.
    #[must_use]
    pub fn entries(&self) -> &[RecordedObservation] {
        &self.entries
    }
}

/// Extracts evidence for every source observation in document order.
/// # Errors
/// Returns structural errors rather than guessing a service association.
pub fn observation_evidence(message: &Message) -> Result<Vec<ObservationEvidence>, MessageError> {
    let mut evidence = Vec::new();
    for (service, _) in
        structure::children(message.document(), 0).filter(|(_, e)| e.name() == "SVC")
    {
        for (subject, _) in structure::children(message.document(), service)
            .filter(|(_, e)| matches!(e.name(), "PT" | "CTC"))
        {
            for (observation, _) in
                structure::children(message.document(), subject).filter(|(_, e)| e.name() == "OBS")
            {
                evidence.push(ObservationEvidence::from_message(
                    message,
                    service,
                    observation,
                )?);
            }
        }
    }
    Ok(evidence)
}
