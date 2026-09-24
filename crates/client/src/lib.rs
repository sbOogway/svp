//! The client side of the svp protocol, and a mock client for trying and
//! testing the server in `examples/mock_client.rs`.

mod client;

use std::{io, path::Path};

pub use client::Client;
pub use svp_transport::protocols::unix::{Connection, default_path};
pub use svp_wire as wire;

/// Connects over the server's Unix socket and says hello.
pub async fn connect(path: &Path, name: impl Into<String>) -> io::Result<Client<Connection>> {
    Client::connect(svp_transport::protocols::unix::connect(path).await?, name).await
}
