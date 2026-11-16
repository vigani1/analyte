//! Changed-content status decisions using the caller?s ADR 0011 policy.
use super::*;
impl Reconciliation<'_, '_> {
    pub(super) fn permits_progression(
        &self,
        identity_key: &str,
        candidate: &ResultContent,
        previous: Option<i64>,
        correction: bool,
        staged: &[(String, ResultContent)],
    ) -> Result<bool, Error> {
        let withdrawn = candidate
            .parts
            .as_slice()
            .iter()
            .any(|p| p.status == ResultStatus::Withdrawn);
        let progression = if let Some(policy) = self.lifecycle.filter(|_| previous.is_some()) {
            let prior = staged
                .iter()
                .rev()
                .find(|(key, _)| key == identity_key)
                .map(|(_, result)| result.clone())
                .map(Ok)
                .unwrap_or_else(|| {
                    status::latest(self.tx, identity_key, self.limit)
                        .and_then(|result| result.ok_or(Error::Corruption))
                })?;
            policy.permits(&prior, candidate)
        } else {
            self.lifecycle.is_none() && (correction || withdrawn)
        };

        Ok(progression)
    }
}
