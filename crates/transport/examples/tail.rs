//! Prints every message from a running `svp` server:
//! `cargo run -p svp-transport --example tail [socket]`.

use std::path::PathBuf;

use futures::StreamExt;
use svp_transport::protocols::unix;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .map_or_else(unix::default_path, PathBuf::from);
    let mut messages = Box::pin(unix::connect(&path).await?);
    while let Some(message) = messages.next().await {
        println!("{:?}", message?);
    }
    Ok(())
}
