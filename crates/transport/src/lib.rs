//! How unified data leaves the aggregator and reaches clients: the sinks the
//! aggregator feeds, the frame codec, both sides of the connection scheme, and
//! the transports that carry frames. No Nautilus, so the app can use it too.

pub mod client;
pub mod codec;
pub mod protocols;
pub mod session;
pub mod sink;
