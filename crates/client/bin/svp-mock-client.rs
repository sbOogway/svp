//! Prints every message from a running `svp-server` until Ctrl-C:
//! `cargo run --bin svp-mock-client [socket]`.

use std::path::PathBuf;

use svp_client::{connect, default_path, protocol::Subscription};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .map_or_else(default_path, PathBuf::from);
    let mut client = connect(&path, "mock-client").await?;
    for instrument in client.instruments() {
        println!("{instrument:?}");
    }
    let everything = client
        .instruments()
        .iter()
        .map(|instrument| Subscription {
            instrument: instrument.id.clone(),
            trades: true,
            books: true,
        })
        .collect();
    client.subscribe(everything).await?;
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
