//! How unified data leaves the aggregator and reaches clients: the sinks the
//! aggregator feeds, the frame codec, a per-client session, and the transports
//! that carry frames. No Nautilus, so the app can use it too.

pub mod codec;
pub mod session;
pub mod sink;
pub mod unix;
