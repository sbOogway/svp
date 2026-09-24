//! A mock client for trying and testing the svp server; see `examples/tail.rs`.

pub use svp_transport::{
    client::Client,
    protocols::unix::{connect, default_path},
};
pub use svp_wire as wire;
