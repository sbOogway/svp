//! What the svp server and its clients share: the [`protocol`] they speak, the
//! [`market`]s it describes, and the transports that carry its frames. No
//! Nautilus, so the app can use it too.

pub mod market;
pub mod protocol;
mod transport;

pub use transport::unix;
