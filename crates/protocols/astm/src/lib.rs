//! Parses bounded ASTM records and exposes typed fields without I/O or model mapping.
#![forbid(unsafe_code)]

mod syntax;
pub use syntax::{
    Component, Delimiters, Error, Field, Kind, Limits, Message, Presence, Record, ascii, latin1,
    parse, parse_with, utf8,
};

mod views;
pub use views::{
    Comment, Header, Manufacturer, Order, Patient, Query, Result, Scientific, Terminator,
};

mod codes;
pub use codes::{ProcessingId, ResultStatus, TerminationCode};

mod query;
pub use query::{QueryIntent, QuerySelectorError, query_intent, starts_query};

mod encode;
pub use encode::{OrderTermination, encode_order_envelope};

mod order;
pub use order::{OrderError, OrderRecord};
