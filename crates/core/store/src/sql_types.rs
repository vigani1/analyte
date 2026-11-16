//! Encodes and decodes SQL lifecycle values, rejecting unknown stored states.
use crate::{DeliveryKind, DeliveryState, PollIntentState};
use analyte_model::OrderState;
use rusqlite::{
    ToSql,
    types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, ValueRef},
};

macro_rules! text_sql {
    ($ty:ty, $( $variant:pat => $text:literal => $value:expr ),+ $(,)?) => {
        impl ToSql for $ty {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                let text = match self { $( $variant => $text, )+ };
                Ok(ToSqlOutput::Borrowed(ValueRef::Text(text.as_bytes())))
            }
        }
        impl FromSql for $ty {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                match value.as_str()? {
                    $( $text => Ok($value), )+
                    _ => Err(FromSqlError::Other(Box::new(crate::Error::Corruption))),
                }
            }
        }
    };
}

// The model deliberately has no dependency on SQLite, so its state needs this adapter.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SqlOrderState(pub(crate) OrderState);
text_sql!(SqlOrderState,
    Self(OrderState::Requested) => "Requested" => Self(OrderState::Requested),
    Self(OrderState::Sending) => "Sending" => Self(OrderState::Sending),
    Self(OrderState::Sent) => "Sent" => Self(OrderState::Sent),
    Self(OrderState::Accepted) => "Accepted" => Self(OrderState::Accepted),
    Self(OrderState::Done) => "Done" => Self(OrderState::Done),
    Self(OrderState::Rejected) => "Rejected" => Self(OrderState::Rejected),
    Self(OrderState::CancelRequested) => "CancelRequested" => Self(OrderState::CancelRequested),
    Self(OrderState::CancelConfirmed) => "CancelConfirmed" => Self(OrderState::CancelConfirmed),
    Self(OrderState::Uncertain) => "Uncertain" => Self(OrderState::Uncertain),
);
text_sql!(DeliveryState,
    Self::Pending => "pending" => Self::Pending,
    Self::InFlight => "in_flight" => Self::InFlight,
    Self::Done => "done" => Self::Done,
    Self::Parked => "parked" => Self::Parked,
    Self::Paused => "paused" => Self::Paused,
    Self::Conflict => "conflict" => Self::Conflict,
);
text_sql!(DeliveryKind,
    Self::Result => "result" => Self::Result,
    Self::OrderEvent => "order_event" => Self::OrderEvent,
);

#[derive(Debug, Clone, Copy)]
pub(crate) enum OwnerKind {
    None,
    Reservation,
    Cancellation,
}
text_sql!(OwnerKind,
    Self::None => "none" => Self::None,
    Self::Reservation => "reservation" => Self::Reservation,
    Self::Cancellation => "cancellation" => Self::Cancellation,
);

text_sql!(PollIntentState,
    Self::Pending => "pending" => Self::Pending,
    Self::Confirmed => "confirmed" => Self::Confirmed,
    Self::Uncertain => "uncertain" => Self::Uncertain,
);
