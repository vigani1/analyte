//! Tracks machine connection ownership separately from source evidence.
use super::{Error, Identifier, NonZeroU64};
use crate::sql_types::OwnerKind;

#[derive(Debug, Clone, Default)]
pub(crate) enum OrderOwner {
    #[default]
    None,
    Reservation(Identifier),
    Cancellation {
        connection: Option<Identifier>,
        version: NonZeroU64,
    },
}

impl OrderOwner {
    pub(crate) fn columns(&self) -> (OwnerKind, Option<&str>, Option<String>) {
        match self {
            Self::None => (OwnerKind::None, None, None),
            Self::Reservation(owner) => (OwnerKind::Reservation, Some(owner.as_str()), None),
            Self::Cancellation {
                connection,
                version,
            } => (
                OwnerKind::Cancellation,
                connection.as_ref().map(Identifier::as_str),
                Some(version.to_string()),
            ),
        }
    }

    pub(super) fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let kind: OwnerKind = row.get("owner_kind")?;
        let owner: Option<String> = row.get("owner_id")?;
        let version: Option<String> = row.get("cancel_version")?;
        let parse = || -> Result<Self, Error> {
            match (kind, owner, version) {
                (OwnerKind::None, None, None) => Ok(Self::None),
                (OwnerKind::Reservation, Some(owner), None) => Ok(Self::Reservation(
                    Identifier::new(&owner).map_err(|_| Error::Corruption)?,
                )),
                (OwnerKind::Cancellation, owner, Some(version)) => Ok(Self::Cancellation {
                    connection: owner
                        .map(|owner| Identifier::new(&owner).map_err(|_| Error::Corruption))
                        .transpose()?,
                    version: canonical_version(&version)?,
                }),
                _ => Err(Error::Corruption),
            }
        };
        parse().map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
    }
}

fn canonical_version(value: &str) -> Result<NonZeroU64, Error> {
    let version: NonZeroU64 = value.parse().map_err(|_| Error::Corruption)?;
    if version.to_string() != value {
        return Err(Error::Corruption);
    }
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR 0003 rule 8: normalizing a stored version token could invent a replay guard.
    #[test]
    fn adr0003_stored_owner_never_normalizes_a_cancellation_version() {
        for version in ["0001", "+1", "0", "invalid"] {
            let refusal = canonical_version(version).expect_err("input must be refused");
            assert!(
                matches!(&refusal, Error::Corruption),
                "unexpected refusal: {refusal:?}"
            );
        }
    }
}
