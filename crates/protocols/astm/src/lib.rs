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
pub use query::{
    OrderProfile, QueryIntent, QuerySelectorError, StructuredQuery, query_intent, starts_query,
    structured_query_intent,
};

mod encode;
pub use encode::{
    OrderError, OrderRecord, OrderTermination, encode_order_envelope,
    encode_structured_order_envelope,
};
