//! What the svp server and its clients share: the [`protocol`] they speak, and
//! the transports that carry its frames. No Nautilus, so the app can use it too.

pub mod protocol;
mod transport;

pub use transport::unix;
