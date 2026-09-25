//! The client side of the svp protocol, and the clients built on it in
//! `bin/`: `svp-mock-client` prints what the server streams, and `svp-app`
//! (with the `app` feature) draws it.

#[cfg(feature = "app")]
pub mod charts;
mod session;

use std::{io, path::Path};

pub use session::Client;
pub use svp_common::{
    protocol,
    unix::{Connection, default_path},
};

/// Connects over the server's Unix socket and says hello.
pub async fn connect(path: &Path, name: impl Into<String>) -> io::Result<Client<Connection>> {
    Client::connect(svp_common::unix::connect(path).await?, name).await
}
