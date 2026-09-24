//! Connects to every venue through Nautilus Trader, merges their trades and L2
//! books into one unified instrument per coin, and publishes the result to
//! sinks. "Aggregation" here means merging venues, not building candles.
//!
//! The node is single-threaded (`Rc<RefCell>`, `!Send`): nothing in this
//! crate does I/O from an actor; data leaves the node through channels.

pub mod actor;
pub mod node;
pub mod publisher;
pub mod unified;
pub mod venue;
