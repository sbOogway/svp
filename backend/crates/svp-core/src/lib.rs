//! Market data ingestion: the Nautilus Trader live node, the aggregation actor
//! and the broadcast bridge towards the web server.
//!
//! Milestone M1. The node is single-threaded (`Rc<RefCell>`, `!Send`): nothing
//! in this crate does I/O from an actor; data leaves the node through channels.

pub mod actor;
pub mod node;
