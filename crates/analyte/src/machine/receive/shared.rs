//! Classifies shared link and evidence causes once for every protocol host.
use super::error::Error as Evidence;
use crate::machine::link::error::Error as Link;

/// Original typed causes; each protocol retains these through its Refusal field.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Shared {
    /// Transport ownership, shutdown and file-link failures.
    #[error(transparent)]
    Link(#[from] Link),
    /// Durable store, intake, interpretation and bounded evidence failures.
    #[error(transparent)]
    Evidence(Evidence),
}

// Evidence already names every accepted source and keeps its original type.
// This one adapter includes the concrete store boundaries through Evidence's
// closed store-source adapter, rather than copying their list per protocol.
impl<C> From<C> for Shared
where
    Evidence: From<C>,
{
    fn from(cause: C) -> Self {
        Self::Evidence(Evidence::from(cause))
    }
}
impl From<std::io::Error> for Shared {
    fn from(cause: std::io::Error) -> Self {
        Self::Link(Link::from(cause))
    }
}
impl From<analyte_transport::Error> for Shared {
    fn from(cause: analyte_transport::Error) -> Self {
        Self::Link(Link::from(cause))
    }
}
impl From<std::net::AddrParseError> for Shared {
    fn from(cause: std::net::AddrParseError) -> Self {
        Self::Link(Link::from(cause))
    }
}
impl From<analyte_transport::folder::WatchError> for Shared {
    fn from(cause: analyte_transport::folder::WatchError) -> Self {
        Self::Link(Link::from(cause))
    }
}
impl<E: std::error::Error + 'static> From<Shared> for super::custody::Refusal<E> {
    fn from(cause: Shared) -> Self {
        match cause {
            Shared::Link(cause) => Self::Link(Box::new(cause)),
            Shared::Evidence(cause) => Self::Evidence(Box::new(cause)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adr0003_shared_conversions_retain_typed_link_and_store_causes() {
        let link = Shared::from(analyte_transport::Error::Deadline("write all"));
        assert!(matches!(
            link,
            Shared::Link(Link::Transport(analyte_transport::Error::Deadline(
                "write all"
            )))
        ));
        let store = Shared::from(analyte_store::StorageError::Capacity);
        assert!(matches!(
            store,
            Shared::Evidence(Evidence::Store(analyte_store::Error::Capacity))
        ));
    }
}
