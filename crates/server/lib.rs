//! Runs the aggregator and serves what it publishes to every client that
//! connects over the Unix socket.

mod aggregator;
mod hub;
mod session;

use std::{io, path::Path};

use anyhow::Context;
use svp_common::unix;

use crate::{
    aggregator::{
        sink::{LogSink, Sink},
        unified,
        venue::{self, Coin, FeedsBuilder, Market, Venue},
    },
    hub::{ChannelSink, Hub},
};

pub async fn run(socket: &Path) -> anyhow::Result<()> {
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
    let server = unix::Server::bind(socket)
        .await
        .with_context(|| format!("binding {}", socket.display()))?;
    tracing::info!(socket = %server.path().display(), "serving clients");
    tokio::spawn(async move {
        if let Err(e) = serve_clients(&server, &hub).await {
            tracing::error!("unix socket server stopped: {e}");
        }
    });

    let sinks: Vec<Box<dyn Sink>> = vec![Box::new(LogSink), Box::new(channel)];
    let mut node = aggregator::node::build(&feeds, sinks)?;
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use svp_common::protocol::{BookData, BookSide, Message};

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
}
