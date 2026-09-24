//! The connection scheme of [`svp_wire`], served to one client. A transport
//! accepts a connection, frames it both ways and hands it to [`serve`]; the
//! handshake, subscriptions, goodbyes and their logs are the same whatever
//! carries them.

use std::{
    collections::{HashMap, VecDeque},
    io,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use bytes::{Bytes, BytesMut};
use futures::{Sink, SinkExt, Stream, StreamExt};
use svp_wire::{Message, PROTOCOL_VERSION, Request, Subscription};
use tokio::sync::broadcast::{self, error::RecvError};
use tracing::Instrument;

use crate::{
    codec::{decode, encode},
    sink::Hub,
};

/// How long a new connection has to send its [`Request::Hello`].
pub const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

/// Numbers sessions across every transport of the process.
static SESSIONS: AtomicU64 = AtomicU64::new(1);

/// Serves one connection until either side ends it. `peer` is what the
/// transport knows of the client, for the logs.
///
/// Welcomes the client with the hub's instruments, then sends what the hub
/// broadcasts for those it subscribes to. A client that falls behind the
/// hub's capacity gets [`Message::Resync`] and fresh snapshots instead of
/// growing a queue.
pub async fn serve<T>(hub: &Hub, mut frames: T, peer: &str)
where
    T: Stream<Item = io::Result<BytesMut>> + Sink<Bytes, Error = io::Error> + Unpin,
{
    let hello = match handshake(&mut frames).await {
        Ok(hello) => hello,
        Err(Refusal::Closed) => {
            tracing::debug!(peer, "connection closed before hello");
            return;
        }
        Err(Refusal::Reject(reason)) => {
            tracing::warn!(peer, "client rejected: {reason}");
            let _ = frames.send(encode(&Message::Reject { reason })).await;
            return;
        }
    };
    let session = SESSIONS.fetch_add(1, Ordering::Relaxed);
    let span = tracing::info_span!("session", session, client = %hello.name, peer);
    run(hub, frames, session, hello).instrument(span).await;
}

struct Hello {
    name: String,
    version: svp_wire::Version,
}

enum Refusal {
    /// Nothing to answer, e.g. [`crate::protocols::unix::Server::bind`]
    /// checking whether a server is alive.
    Closed,
    Reject(String),
}

async fn handshake<T>(frames: &mut T) -> Result<Hello, Refusal>
where
    T: Stream<Item = io::Result<BytesMut>> + Unpin,
{
    let frame = match tokio::time::timeout(HELLO_TIMEOUT, frames.next()).await {
        Err(_) => {
            return Err(Refusal::Reject(format!(
                "no hello within {HELLO_TIMEOUT:?}"
            )));
        }
        Ok(None) => return Err(Refusal::Closed),
        Ok(Some(frame)) => frame.map_err(|e| Refusal::Reject(e.to_string()))?,
    };
    match decode(&frame) {
        Ok(Request::Hello { version, name }) => {
            if PROTOCOL_VERSION.serves(version) {
                Ok(Hello { name, version })
            } else {
                Err(Refusal::Reject(format!(
                    "protocol {version} is not served by {PROTOCOL_VERSION}"
                )))
            }
        }
        Ok(request) => Err(Refusal::Reject(format!("expected hello, got {request:?}"))),
        Err(e) => Err(Refusal::Reject(format!("undecodable hello: {e}"))),
    }
}

enum End {
    Goodbye(String),
    Eof,
    Failed(io::Error),
    Violation(String),
    HubClosed,
}

async fn run<T>(hub: &Hub, mut frames: T, session: u64, hello: Hello)
where
    T: Stream<Item = io::Result<BytesMut>> + Sink<Bytes, Error = io::Error> + Unpin,
{
    let welcome = Message::Welcome {
        session,
        version: PROTOCOL_VERSION,
        instruments: hub.instruments().to_vec(),
    };
    if let Err(e) = frames.send(encode(&welcome)).await {
        tracing::info!("client disconnected during handshake: {e}");
        return;
    }
    tracing::info!(version = %hello.version, "client connected");

    let end = stream(hub, &mut frames).await.unwrap_or_else(End::Failed);
    let goodbye = match end {
        End::Goodbye(reason) => {
            tracing::info!("client disconnected: {reason}");
            None
        }
        End::Eof => {
            tracing::info!("client disconnected without goodbye");
            None
        }
        End::Failed(e) => {
            tracing::info!("client disconnected: {e}");
            None
        }
        End::Violation(reason) => {
            tracing::warn!("client dropped: {reason}");
            Some(reason)
        }
        End::HubClosed => {
            tracing::info!("client dropped: server shutting down");
            Some("server shutting down".to_owned())
        }
    };
    if let Some(reason) = goodbye {
        let _ = frames.send(encode(&Message::Goodbye { reason })).await;
    }
}

async fn stream<T>(hub: &Hub, frames: &mut T) -> io::Result<End>
where
    T: Stream<Item = io::Result<BytesMut>> + Sink<Bytes, Error = io::Error> + Unpin,
{
    let mut filter = Filter::default();
    let (_, mut rx) = hub.subscribe(|_| false);
    let mut queue = Queue::default();
    loop {
        tokio::select! {
            received = rx.recv() => match received {
                Ok(message) => {
                    if queue.filter_next(&filter).wants(&message) {
                        frames.send(encode(&message)).await?;
                    }
                }
                Err(RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "client fell behind, resyncing");
                    let (snapshots, resubscribed) = hub.subscribe(|i| filter.wants_book(i));
                    rx = resubscribed;
                    queue = Queue::default();
                    frames.send(encode(&Message::Resync { missed })).await?;
                    send_all(frames, &snapshots).await?;
                }
                Err(RecvError::Closed) => return Ok(End::HubClosed),
            },
            frame = frames.next() => {
                let Some(frame) = frame else { return Ok(End::Eof) };
                match decode(&frame?) {
                    Ok(Request::Subscribe { subscriptions }) => {
                        let subscriptions = offered(hub, frames, subscriptions).await?;
                        let before = filter.clone();
                        filter.add(&subscriptions);
                        if !subscriptions.is_empty() {
                            tracing::info!("subscribed to {}", describe(&subscriptions));
                        }
                        let snapshots = queue.catch_up(hub, &rx, before, &filter);
                        send_all(frames, &snapshots).await?;
                    }
                    Ok(Request::Unsubscribe { subscriptions }) => {
                        let subscriptions = offered(hub, frames, subscriptions).await?;
                        filter.remove(&subscriptions);
                        queue.remove(&subscriptions);
                        if !subscriptions.is_empty() {
                            tracing::info!("unsubscribed from {}", describe(&subscriptions));
                        }
                    }
                    Ok(Request::Goodbye { reason }) => return Ok(End::Goodbye(reason)),
                    Ok(Request::Hello { .. }) => {
                        return Ok(End::Violation("hello after the handshake".into()));
                    }
                    Err(e) => return Ok(End::Violation(format!("undecodable request: {e}"))),
                }
            }
        }
    }
}

/// The subscriptions to instruments the hub offers; the client hears of
/// the others in a [`Message::Error`].
async fn offered<T>(
    hub: &Hub,
    frames: &mut T,
    subscriptions: Vec<Subscription>,
) -> io::Result<Vec<Subscription>>
where
    T: Sink<Bytes, Error = io::Error> + Unpin,
{
    let (offered, unknown): (Vec<_>, Vec<_>) = subscriptions
        .into_iter()
        .partition(|s| hub.instruments().iter().any(|i| i.id == s.instrument));
    if !unknown.is_empty() {
        let ids: Vec<_> = unknown.iter().map(|s| s.instrument.as_str()).collect();
        let reason = format!("unknown instruments: {}", ids.join(", "));
        tracing::info!("{reason}");
        frames.send(encode(&Message::Error { reason })).await?;
    }
    Ok(offered)
}

/// `BTC-PERP.SVP trades+books, ETH-PERP.SVP trades`
fn describe(subscriptions: &[Subscription]) -> String {
    let each = subscriptions.iter().map(|s| {
        let kinds = match (s.trades, s.books) {
            (true, true) => "trades+books",
            (true, false) => "trades",
            (false, true) => "books",
            (false, false) => "nothing",
        };
        format!("{} {kinds}", s.instrument)
    });
    each.collect::<Vec<_>>().join(", ")
}

async fn send_all<T>(frames: &mut T, messages: &[Message]) -> io::Result<()>
where
    T: Sink<Bytes, Error = io::Error> + Unpin,
{
    for message in messages {
        frames.send(encode(message)).await?;
    }
    Ok(())
}

/// The filters still owed to messages the receiver queued before a
/// subscription: their book updates are already in its snapshots.
#[derive(Debug, Default)]
struct Queue {
    received: u64,
    /// Messages numbered below the bound pass the filter; bounds ascend.
    earlier: VecDeque<(u64, Filter)>,
}

impl Queue {
    fn catch_up(
        &mut self,
        hub: &Hub,
        rx: &broadcast::Receiver<Message>,
        before: Filter,
        after: &Filter,
    ) -> Vec<Message> {
        let (snapshots, queued) =
            hub.catch_up(rx, |i| after.wants_book(i) && !before.wants_book(i));
        if queued > 0 {
            self.earlier
                .push_back((self.received + queued as u64, before));
        }
        snapshots
    }

    fn remove(&mut self, subscriptions: &[Subscription]) {
        for (_, filter) in &mut self.earlier {
            filter.remove(subscriptions);
        }
    }

    fn filter_next<'a>(&'a mut self, current: &'a Filter) -> &'a Filter {
        let n = self.received;
        self.received += 1;
        while self.earlier.front().is_some_and(|&(bound, _)| bound <= n) {
            self.earlier.pop_front();
        }
        self.earlier.front().map_or(current, |(_, filter)| filter)
    }
}

#[derive(Debug, Clone, Default)]
struct Filter {
    instruments: HashMap<String, Kinds>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Kinds {
    trades: bool,
    books: bool,
}

impl Filter {
    fn kinds(&mut self, instrument: &str) -> &mut Kinds {
        self.instruments.entry(instrument.to_owned()).or_default()
    }

    fn add(&mut self, subscriptions: &[Subscription]) {
        for s in subscriptions {
            let kinds = self.kinds(&s.instrument);
            kinds.trades |= s.trades;
            kinds.books |= s.books;
        }
    }

    fn remove(&mut self, subscriptions: &[Subscription]) {
        for s in subscriptions {
            let kinds = self.kinds(&s.instrument);
            kinds.trades &= !s.trades;
            kinds.books &= !s.books;
        }
    }

    fn wants_trades(&self, instrument: &str) -> bool {
        self.instruments.get(instrument).is_some_and(|k| k.trades)
    }

    fn wants_book(&self, instrument: &str) -> bool {
        self.instruments.get(instrument).is_some_and(|k| k.books)
    }

    fn wants(&self, message: &Message) -> bool {
        match message {
            Message::Trade(t) => self.wants_trades(&t.instrument),
            Message::Book(b) => self.wants_book(&b.instrument),
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use svp_wire::{Book, BookData, BookSide, BookUpdate, Version};
    use tokio::io::DuplexStream;
    use tokio_util::codec::{Framed, LengthDelimitedCodec};

    use super::*;
    use crate::{
        client::Client,
        sink::{
            Sink as _,
            tests::{ID, OTHER, channel, px, qty, subscription, trade, update},
        },
    };

    type Frames = Framed<DuplexStream, LengthDelimitedCodec>;

    /// The client's end of a connection [`serve`] runs on, holding `capacity`
    /// bytes in flight.
    fn connection(hub: &Hub, capacity: usize) -> Frames {
        let (client, server) = tokio::io::duplex(capacity);
        let hub = hub.clone();
        tokio::spawn(async move {
            serve(
                &hub,
                Framed::new(server, LengthDelimitedCodec::new()),
                "test",
            )
            .await;
        });
        Framed::new(client, LengthDelimitedCodec::new())
    }

    async fn connect(hub: &Hub) -> Client<Frames> {
        Client::connect(connection(hub, 1 << 16), "test")
            .await
            .unwrap()
    }

    /// Subscribes, and returns once the server applied it with the snapshots
    /// it sent: the server answers the unknown instrument that follows in
    /// order.
    async fn subscribe(
        client: &mut Client<Frames>,
        subscriptions: Vec<Subscription>,
    ) -> Vec<Message> {
        client.subscribe(subscriptions).await.unwrap();
        barrier(client).await
    }

    async fn unsubscribe(client: &mut Client<Frames>, subscriptions: Vec<Subscription>) {
        client.unsubscribe(subscriptions).await.unwrap();
        assert_eq!(barrier(client).await, []);
    }

    async fn barrier(client: &mut Client<Frames>) -> Vec<Message> {
        client
            .subscribe(vec![subscription("BARRIER", true, true)])
            .await
            .unwrap();
        let mut before = Vec::new();
        loop {
            match next(client).await {
                Message::Error { reason } if reason.contains("BARRIER") => return before,
                message => before.push(message),
            }
        }
    }

    async fn next(client: &mut Client<Frames>) -> Message {
        tokio::time::timeout(Duration::from_secs(1), client.recv())
            .await
            .expect("a message within a second")
            .expect("the session is still running")
            .unwrap()
    }

    async fn nothing_more(client: &mut Client<Frames>) {
        let more = tokio::time::timeout(Duration::from_millis(50), client.recv()).await;
        assert!(more.is_err(), "unexpected {more:?}");
    }

    fn apply(book: &mut Book, message: &Message) {
        if let Message::Book(update) = message {
            book.apply(&update.data);
        }
    }

    fn snapshot(ts: u64, bid: &str, size: &str) -> Message {
        Message::Book(BookUpdate {
            instrument: ID.into(),
            ts,
            data: BookData::Snapshot {
                bids: vec![(px(bid), qty(size))],
                asks: vec![],
            },
        })
    }

    #[tokio::test]
    async fn the_welcome_offers_the_hubs_instruments_and_nothing_streams_unasked() {
        let (mut sink, hub) = channel(8);
        sink.send(&update(1, &[(BookSide::Bid, "100", "1")]));
        let mut client = connect(&hub).await;

        let ids: Vec<_> = client.instruments().iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, [ID, OTHER]);
        sink.send(&trade(2));
        nothing_more(&mut client).await;
    }

    #[tokio::test]
    async fn a_subscription_starts_from_snapshots_then_messages_in_order() {
        let (mut sink, hub) = channel(8);
        sink.send(&update(1, &[(BookSide::Bid, "100", "1")]));
        let mut client = connect(&hub).await;

        let snapshots = subscribe(&mut client, vec![subscription(ID, true, true)]).await;
        assert_eq!(snapshots, [snapshot(1, "100", "1")]);

        sink.send(&trade(2));
        sink.send(&update(3, &[(BookSide::Ask, "101", "1")]));
        assert_eq!(next(&mut client).await, trade(2));
        assert_eq!(
            next(&mut client).await,
            update(3, &[(BookSide::Ask, "101", "1")])
        );
    }

    #[test]
    fn subscriptions_are_logged_by_instrument_and_kind() {
        let subscriptions = [
            subscription(ID, true, true),
            subscription(OTHER, true, false),
        ];
        assert_eq!(
            describe(&subscriptions),
            "BTC-PERP.SVP trades+books, ETH-PERP.SVP trades"
        );
    }

    #[tokio::test]
    async fn sessions_are_numbered() {
        let (_sink, hub) = channel(8);
        let a = connect(&hub).await;
        let b = connect(&hub).await;
        assert!(b.session() > a.session());
    }

    #[tokio::test]
    async fn an_unserved_version_is_rejected() {
        let (_sink, hub) = channel(8);
        let mut frames = connection(&hub, 1 << 16);
        let hello = Request::Hello {
            version: Version {
                major: PROTOCOL_VERSION.major + 1,
                minor: 0,
            },
            name: "future".into(),
        };
        frames.send(encode(&hello)).await.unwrap();
        let reply = frames.next().await.unwrap().unwrap();
        assert!(matches!(decode(&reply).unwrap(), Message::Reject { .. }));
        assert!(frames.next().await.is_none(), "the server closes");
    }

    #[tokio::test]
    async fn a_request_before_hello_is_rejected() {
        let (_sink, hub) = channel(8);
        let mut frames = connection(&hub, 1 << 16);
        let goodbye = Request::Goodbye {
            reason: "hi".into(),
        };
        frames.send(encode(&goodbye)).await.unwrap();
        let reply = frames.next().await.unwrap().unwrap();
        assert!(matches!(decode(&reply).unwrap(), Message::Reject { .. }));
    }

    #[tokio::test]
    async fn a_second_hello_ends_with_goodbye() {
        let (_sink, hub) = channel(8);
        let mut frames = connection(&hub, 1 << 16);
        let hello = Request::Hello {
            version: PROTOCOL_VERSION,
            name: "twice".into(),
        };
        frames.send(encode(&hello)).await.unwrap();
        let welcome = frames.next().await.unwrap().unwrap();
        assert!(matches!(decode(&welcome).unwrap(), Message::Welcome { .. }));
        frames.send(encode(&hello)).await.unwrap();
        let reply = frames.next().await.unwrap().unwrap();
        assert!(matches!(decode(&reply).unwrap(), Message::Goodbye { .. }));
    }

    #[tokio::test]
    async fn an_unknown_instrument_is_an_error_and_the_rest_applies() {
        let (mut sink, hub) = channel(8);
        let mut client = connect(&hub).await;

        client
            .subscribe(vec![
                subscription("NOPE", true, true),
                subscription(ID, true, false),
            ])
            .await
            .unwrap();
        assert_eq!(
            next(&mut client).await,
            Message::Error {
                reason: "unknown instruments: NOPE".into()
            }
        );
        sink.send(&trade(1));
        assert_eq!(next(&mut client).await, trade(1));
    }

    #[tokio::test]
    async fn only_subscribed_kinds_and_instruments_arrive() {
        let (mut sink, hub) = channel(8);
        sink.send(&update(1, &[(BookSide::Bid, "100", "1")]));
        let mut client = connect(&hub).await;
        assert_eq!(
            subscribe(&mut client, vec![subscription(ID, true, false)]).await,
            []
        );

        sink.send(&update(2, &[(BookSide::Bid, "100", "2")]));
        sink.send(&trade(3));
        assert_eq!(next(&mut client).await, trade(3), "no update");

        unsubscribe(&mut client, vec![subscription(ID, true, false)]).await;
        subscribe(&mut client, vec![subscription(OTHER, true, true)]).await;
        sink.send(&trade(4));
        nothing_more(&mut client).await;
    }

    #[tokio::test]
    async fn subscribing_to_a_book_starts_it_from_a_snapshot() {
        let (mut sink, hub) = channel(8);
        sink.send(&update(1, &[(BookSide::Bid, "100", "1")]));
        let mut client = connect(&hub).await;
        subscribe(&mut client, vec![subscription(ID, true, false)]).await;

        let snapshots = subscribe(&mut client, vec![subscription(ID, false, true)]).await;
        assert_eq!(snapshots, [snapshot(1, "100", "1")]);
        sink.send(&update(2, &[(BookSide::Ask, "101", "1")]));
        assert_eq!(
            next(&mut client).await,
            update(2, &[(BookSide::Ask, "101", "1")])
        );
    }

    #[tokio::test]
    async fn updates_queued_before_a_subscription_are_not_applied_twice() {
        let (mut sink, hub) = channel(64);
        let mut filter = Filter::default();
        filter.add(&[subscription(ID, true, false)]);
        let (_, mut rx) = hub.subscribe(|i| filter.wants_book(i));
        sink.send(&update(1, &[(BookSide::Bid, "100", "1")]));
        sink.send(&trade(2));

        let mut queue = Queue::default();
        let before = filter.clone();
        filter.add(&[subscription(ID, false, true)]);
        let snapshots = queue.catch_up(&hub, &rx, before, &filter);
        sink.send(&update(3, &[(BookSide::Bid, "100", "0")]));

        let mut passed = snapshots;
        while let Ok(message) = rx.try_recv() {
            if queue.filter_next(&filter).wants(&message) {
                passed.push(message);
            }
        }
        assert_eq!(
            passed,
            [
                snapshot(1, "100", "1"),
                trade(2),
                update(3, &[(BookSide::Bid, "100", "0")])
            ]
        );
    }

    #[tokio::test]
    async fn a_client_goodbye_ends_the_session() {
        let (_sink, hub) = channel(8);
        let (client, server) = tokio::io::duplex(1 << 16);
        let session = tokio::spawn(async move {
            serve(
                &hub,
                Framed::new(server, LengthDelimitedCodec::new()),
                "test",
            )
            .await;
        });
        let client = Client::connect(Framed::new(client, LengthDelimitedCodec::new()), "test")
            .await
            .unwrap();
        client.goodbye("done").await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), session)
            .await
            .expect("the session ends")
            .unwrap();
    }

    #[tokio::test]
    async fn a_slow_client_resyncs_from_snapshots() {
        let (mut sink, hub) = channel(4);
        let mut client = Client::connect(connection(&hub, 1 << 10), "slow")
            .await
            .unwrap();
        subscribe(&mut client, vec![subscription(ID, true, true)]).await;

        // The client reads nothing while the book moves far past the capacity.
        for i in 0..50_u32 {
            sink.send(&update(
                u64::from(i),
                &[(BookSide::Bid, &i.to_string(), "1")],
            ));
        }

        let mut book = Book::default();
        let mut missed = None;
        loop {
            let message = next(&mut client).await;
            if let Message::Resync { missed: n } = message {
                missed = Some(n);
            }
            apply(&mut book, &message);
            if matches!(&message, Message::Book(u) if u.ts == 49) {
                break;
            }
        }
        assert!(missed.is_some_and(|n| n > 0));

        let (snapshots, _) = hub.subscribe(|_| true);
        let mut expected = Book::default();
        apply(&mut expected, &snapshots[0]);
        assert_eq!(book, expected);
    }
}
