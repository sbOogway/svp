//! Prints every message from a running `svp` server until Ctrl-C:
//! `cargo run -p svp-transport --example tail [socket]`.

use std::path::PathBuf;

use svp_transport::protocols::unix;
use svp_wire::Subscription;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .map_or_else(unix::default_path, PathBuf::from);
    let mut client = unix::connect(&path, "tail", vec![Subscription::everything()]).await?;
    loop {
        tokio::select! {
            message = client.recv() => match message {
                Some(message) => println!("{:?}", message?),
                None => return Ok(()),
            },
            _ = tokio::signal::ctrl_c() => return client.goodbye("interrupted").await,
        }
    }
}
