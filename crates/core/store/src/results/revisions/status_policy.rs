//! Applies caller-supplied status transitions, withheld statuses and single-run statuses without protocol codes.
use crate::{Interpretation, Reading, ReconciliationError as Error, key};
use analyte_model::{IdentityStatus, ObservationIdentity, ResultContent};
use rusqlite::{OptionalExtension, params};

/// Source syntax supplies its complete changed-content lifecycle and reporting rules.
/// Omitting a policy preserves correction/withdrawal-only reconciliation.
#[derive(Clone, Copy)]
pub struct StatusLifecycle<'a> {
    /// Allowed transitions for changed content, including any correction transition.
    pub transitions: &'a [(&'a str, &'a str)],
    /// Statuses stored without production delivery.
    pub withheld: &'a [&'a str],
    /// Statuses limited to one run per machine, order and observation test.
    pub single_run: &'a [&'a str],
    /// Syntax-owned run extraction; absent evidence cannot establish exclusivity.
    pub run: fn(&ObservationIdentity) -> Option<String>,
}
fn status(result: &ResultContent) -> Option<&str> {
    let parts = result.parts.as_slice();
    let first = parts.first()?.source_status.as_str();
    parts
        .iter()
        .all(|p| p.source_status.as_str() == first)
        .then_some(first)
}
impl StatusLifecycle<'_> {
    pub(super) fn permits(self, before: &ResultContent, after: &ResultContent) -> bool {
        status(before)
            .zip(status(after))
            .is_some_and(|pair| self.transitions.contains(&pair))
    }
    /// Reports whether every part is reportable according to the supplied source policy.
    #[must_use]
    pub fn may_deliver(self, result: &ResultContent) -> bool {
        result
            .parts
            .as_slice()
            .iter()
            .all(|part| !self.withheld.contains(&part.source_status.as_str()))
    }
    pub(super) fn exclusive(self, result: &ResultContent) -> bool {
        result
            .parts
            .as_slice()
            .iter()
            .any(|p| self.single_run.contains(&p.source_status.as_str()))
    }
}

pub(super) fn latest(
    db: &rusqlite::Connection,
    identity: &str,
    limit: usize,
) -> Result<Option<ResultContent>, Error> {
    let encoded:Option<(String,i64)>=db.query_row("SELECT i.evidence,r.revision FROM observation_revisions r JOIN interpretations i ON i.receipt=r.receipt AND i.version=r.interpretation_version WHERE r.identity_key=?1 ORDER BY r.revision DESC LIMIT 1",[identity],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let Some((encoded, revision)) = encoded else {
        return Ok(None);
    };
    if encoded.len() > limit {
        return Err(Error::Corruption);
    }
    let evidence: Interpretation = serde_json::from_str(&encoded)?;
    if let Reading::Results(results) = evidence.outcome {
        let (_, expected): (analyte_model::Identifier, ObservationIdentity) =
            serde_json::from_str(identity)?;
        for result in results {
            if let IdentityStatus::Resolved(id) = &result.identity
                && id.observation == expected
                && i64::try_from(id.revision.0).map_err(|_| Error::Corruption)? == revision
            {
                return Ok(Some(result));
            }
        }
    }
    Err(Error::Corruption)
}

pub(super) fn other_final_run(
    db: &rusqlite::Connection,
    machine: &analyte_model::Identifier,
    candidate: &ResultContent,
    staged: &[(String, ResultContent)],
    policy: StatusLifecycle<'_>,
    rows: usize,
    limit: usize,
) -> Result<bool, Error> {
    if !policy.exclusive(candidate) {
        return Ok(false);
    }
    let IdentityStatus::Resolved(id) = &candidate.identity else {
        return Ok(true);
    };
    let Some(run) = (policy.run)(&id.observation) else {
        return Ok(true);
    };
    let incompatible = |other: &ResultContent| {
        if !policy.exclusive(other) {
            return false;
        }
        let IdentityStatus::Resolved(other) = &other.identity else {
            return true;
        };
        other.observation.order == id.observation.order
            && other.observation.code == id.observation.code
            && (policy.run)(&other.observation).is_none_or(|other| other != run)
    };
    if staged.iter().any(|(_, result)| incompatible(result)) {
        return Ok(true);
    }
    let mut query=db.prepare("SELECT DISTINCT identity_key FROM observation_revisions WHERE json_extract(identity_key,'$[0]')=?1 AND json_extract(identity_key,'$[1].order')=?2 AND json_extract(identity_key,'$[1].code')=?3 LIMIT ?4")?;
    let keys = query
        .query_map(
            params![
                machine.as_str(),
                key(&id.observation.order)?,
                key(&id.observation.code)?,
                i64::try_from(rows.checked_add(1).ok_or(Error::Capacity)?)
                    .map_err(|_| Error::Capacity)?
            ],
            |r| r.get::<_, String>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    if keys.len() > rows {
        return Err(Error::Capacity);
    }
    for identity in keys {
        if let Some(other) = latest(db, &identity, limit)?
            && incompatible(&other)
        {
            return Ok(true);
        }
    }
    Ok(false)
}
