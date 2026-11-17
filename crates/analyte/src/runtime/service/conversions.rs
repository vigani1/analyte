//! Converts each explicitly listed error cause through its named typed boundary.
/// Rust does not chain `From` conversions. Each invocation lists the concrete
/// causes accepted by that owner and the intermediate boundary that retains them.
/// This helper only writes that single conversion; it invokes no other macros
/// and never boxes, erases, classifies or handles cleanup itself.
macro_rules! causes {
    ($target:ty; $($boundary:ty => [$($cause:ty),+ $(,)?]),+ $(,)?) => {
        $($(impl From<$cause> for $target {
            fn from(cause: $cause) -> Self {
                Self::from(<$boundary>::from(cause))
            }
        })+)+
    };
}
pub(crate) use causes;
