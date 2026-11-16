//! Opens, migrates, backs up and restores stores with capacity and startup checks.
pub(crate) mod backup;
pub(crate) mod capacity;
pub(crate) mod migration;
pub(crate) mod open;
pub(crate) mod restore;
pub(crate) mod startup;
pub(crate) mod worker;

pub(crate) mod retention;
