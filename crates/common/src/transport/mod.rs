//! How frames move between the server and its clients. A transport only
//! carries frames and does its own framing; what they say is the
//! [`protocol`](crate::protocol)'s business.

pub mod unix;
