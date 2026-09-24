mod hub;
mod session;

use std::{ffi::OsString, io, path::PathBuf};

use anyhow::Context;
use svp_aggregator::{
    sink::{LogSink, Sink},
    unified,
    venue::{self, Coin, FeedsBuilder, Market, Venue},
};
use svp_transport::protocols::unix;
use tracing_subscriber::EnvFilter;

use crate::hub::{ChannelSink, Hub};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Nautilus owns the global `log` logger (the kernel refuses to start
    // otherwise), so the tracing subscriber is installed without the
    // `log` bridge that `fmt().init()` would add.
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "svp starting");
    let socket = socket_path(std::env::args_os().skip(1), std::env::var_os("SVP_SOCKET"))?;

    let feeds = FeedsBuilder::new()
        .add_venue(Venue::Binance)
        .add_venue(Venue::Bybit)
        .add_venue(Venue::Okx)
        .add_venue(Venue::Kraken)
        .add_venue(Venue::Coinbase)
        .add_venue(Venue::Hyperliquid)
        .add_market(Market::Futures)
        .add_instrument(Coin::BTC)
        .build()?;

    let instruments = unified::unify(&venue::subscriptions(&feeds))
        .iter()
        .map(unified::Unified::describe)
        .collect();
    let (channel, hub) = ChannelSink::new(4096, instruments);
    let server = unix::Server::bind(&socket)
        .await
        .with_context(|| format!("binding {}", socket.display()))?;
    tracing::info!(socket = %server.path().display(), "serving clients");
    tokio::spawn(async move {
        if let Err(e) = serve_clients(&server, &hub).await {
            tracing::error!("unix socket server stopped: {e}");
        }
    });

    let sinks: Vec<Box<dyn Sink>> = vec![Box::new(LogSink), Box::new(channel)];
    let mut node = svp_aggregator::node::build(&feeds, sinks)?;
    node.run().await
}

/// Hands each client that connects to [`session::serve`] in its own task.
async fn serve_clients(server: &unix::Server, hub: &Hub) -> io::Result<()> {
    loop {
        let (frames, peer) = server.accept().await?;
        let hub = hub.clone();
        tokio::spawn(async move { session::serve(&hub, frames, &peer).await });
    }
}

/// `--socket PATH`, else `SVP_SOCKET`, else [`unix::default_path`].
fn socket_path(
    mut args: impl Iterator<Item = OsString>,
    env: Option<OsString>,
) -> anyhow::Result<PathBuf> {
    match args.next() {
        None => Ok(env.map_or_else(unix::default_path, PathBuf::from)),
        Some(flag) if flag == "--socket" => args
            .next()
            .map(PathBuf::from)
            .context("--socket needs a path"),
        Some(other) => anyhow::bail!("unknown argument {}", other.display()),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use svp_wire::{BookData, BookSide, Message};

    use super::*;
    use crate::hub::tests::{ID, channel, px, qty, subscription, trade, update};

    #[tokio::test]
    async fn a_client_subscribes_over_the_socket_to_what_the_welcome_offers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("svp.sock");
        let (mut sink, hub) = channel(8);
        sink.send(&update(1, &[(BookSide::Bid, "100", "1")]));
        let server = unix::Server::bind(&path).await.unwrap();
        tokio::spawn(async move { serve_clients(&server, &hub).await });

        let mut client = svp_client::connect(&path, "test").await.unwrap();
        assert_eq!(client.instruments()[0].id, ID);
        client
            .subscribe(vec![subscription(ID, true, true)])
            .await
            .unwrap();
        let mut next = async || {
            tokio::time::timeout(Duration::from_secs(1), client.recv())
                .await
                .expect("a message within a second")
                .expect("the server is still running")
                .unwrap()
        };
        let Message::Book(snapshot) = next().await else {
            panic!("expected a snapshot first");
        };
        assert_eq!(
            snapshot.data,
            BookData::Snapshot {
                bids: vec![(px("100"), qty("1"))],
                asks: vec![],
            }
        );
        sink.send(&trade(2));
        assert_eq!(next().await, trade(2));
    }

    fn args(args: &[&str]) -> impl Iterator<Item = OsString> {
        args.iter()
            .map(OsString::from)
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn socket_flag_beats_env_beats_default() {
        let env = || Some(OsString::from("/env.sock"));
        assert_eq!(
            socket_path(args(&["--socket", "/flag.sock"]), env()).unwrap(),
            PathBuf::from("/flag.sock")
        );
        assert_eq!(
            socket_path(args(&[]), env()).unwrap(),
            PathBuf::from("/env.sock")
        );
        assert_eq!(socket_path(args(&[]), None).unwrap(), unix::default_path());
    }

    #[test]
    fn rejects_a_bare_flag_and_unknown_arguments() {
        assert!(socket_path(args(&["--socket"]), None).is_err());
        assert!(socket_path(args(&["--port", "1"]), None).is_err());
    }
}
