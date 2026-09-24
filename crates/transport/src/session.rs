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
/// Sends every subscribed book's snapshot, then what the hub broadcasts and
/// the client subscribed to. A client that falls behind the hub's capacity
/// gets [`Message::Resync`] and fresh snapshots instead of growing a queue.
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
    filter: Filter,
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
        Ok(Request::Hello {
            version,
            name,
            subscriptions,
        }) => {
            if PROTOCOL_VERSION.serves(version) {
                Ok(Hello {
                    name,
                    version,
                    filter: Filter::new(&subscriptions),
                })
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
    };
    if let Err(e) = frames.send(encode(&welcome)).await {
        tracing::info!("client disconnected during handshake: {e}");
        return;
    }
    tracing::info!(version = %hello.version, "client connected");

    let end = stream(hub, &mut frames, hello.filter)
        .await
        .unwrap_or_else(End::Failed);
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

async fn stream<T>(hub: &Hub, frames: &mut T, mut filter: Filter) -> io::Result<End>
where
    T: Stream<Item = io::Result<BytesMut>> + Sink<Bytes, Error = io::Error> + Unpin,
{
    let (snapshots, mut rx) = hub.subscribe(|i| filter.wants_book(i));
    send_all(frames, &snapshots).await?;
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
                        let before = filter.clone();
                        filter.add(&subscriptions);
                        tracing::debug!(?subscriptions, "subscribed");
                        let snapshots = queue.catch_up(hub, &rx, before, &filter);
                        send_all(frames, &snapshots).await?;
                    }
                    Ok(Request::Unsubscribe { subscriptions }) => {
                        filter.remove(&subscriptions);
                        queue.remove(&subscriptions);
                        tracing::debug!(?subscriptions, "unsubscribed");
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
    every: Kinds,
    instruments: HashMap<String, Kinds>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Kinds {
    trades: bool,
    books: bool,
}

impl Filter {
    fn new(subscriptions: &[Subscription]) -> Self {
        let mut filter = Self::default();
        filter.add(subscriptions);
        filter
    }

    fn kinds(&mut self, instrument: Option<&String>) -> &mut Kinds {
        match instrument {
            None => &mut self.every,
            Some(i) => self.instruments.entry(i.clone()).or_default(),
        }
    }

    fn add(&mut self, subscriptions: &[Subscription]) {
        for s in subscriptions {
            let kinds = self.kinds(s.instrument.as_ref());
            kinds.trades |= s.trades;
            kinds.books |= s.books;
        }
    }

    fn remove(&mut self, subscriptions: &[Subscription]) {
        for s in subscriptions {
            let kinds = self.kinds(s.instrument.as_ref());
            kinds.trades &= !s.trades;
            kinds.books &= !s.books;
        }
    }

    fn wants_trades(&self, instrument: &str) -> bool {
        self.every.trades || self.instruments.get(instrument).is_some_and(|k| k.trades)
    }

    fn wants_book(&self, instrument: &str) -> bool {
        self.every.books || self.instruments.get(instrument).is_some_and(|k| k.books)
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
            ChannelSink, Sink as _,
            tests::{ID, px, qty, trade, update},
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

    async fn connect(hub: &Hub, subscriptions: Vec<Subscription>) -> Client<Frames> {
        Client::connect(connection(hub, 1 << 16), "test", subscriptions)
            .await
            .unwrap()
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

    fn only(instrument: &str, trades: bool, books: bool) -> Subscription {
        Subscription {
            instrument: Some(instrument.into()),
            trades,
            books,
        }
    }

    #[tokio::test]
    async fn welcome_then_snapshots_then_messages_in_order() {
        let (mut sink, hub) = ChannelSink::new(8);
        sink.send(&update(1, &[(BookSide::Bid, "100", "1")]));
        let mut client = connect(&hub, vec![Subscription::everything()]).await;

        let Message::Book(snapshot) = next(&mut client).await else {
            panic!("expected a snapshot first");
        };
        assert!(matches!(snapshot.data, BookData::Snapshot { .. }));

        sink.send(&trade(2));
        sink.send(&update(3, &[(BookSide::Ask, "101", "1")]));
        assert_eq!(next(&mut client).await, trade(2));
        assert_eq!(
            next(&mut client).await,
            update(3, &[(BookSide::Ask, "101", "1")])
        );
    }

    #[tokio::test]
    async fn sessions_are_numbered() {
        let (_sink, hub) = ChannelSink::new(8);
        let a = connect(&hub, vec![]).await;
        let b = connect(&hub, vec![]).await;
        assert!(b.session() > a.session());
    }

    #[tokio::test]
    async fn an_unserved_version_is_rejected() {
        let (_sink, hub) = ChannelSink::new(8);
        let mut frames = connection(&hub, 1 << 16);
        let hello = Request::Hello {
            version: Version {
                major: PROTOCOL_VERSION.major + 1,
                minor: 0,
            },
            name: "future".into(),
            subscriptions: vec![],
        };
        frames.send(encode(&hello)).await.unwrap();
        let reply = frames.next().await.unwrap().unwrap();
        assert!(matches!(decode(&reply).unwrap(), Message::Reject { .. }));
        assert!(frames.next().await.is_none(), "the server closes");
    }

    #[tokio::test]
    async fn a_request_before_hello_is_rejected() {
        let (_sink, hub) = ChannelSink::new(8);
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
        let (_sink, hub) = ChannelSink::new(8);
        let mut frames = connection(&hub, 1 << 16);
        let hello = Request::Hello {
            version: PROTOCOL_VERSION,
            name: "twice".into(),
            subscriptions: vec![],
        };
        frames.send(encode(&hello)).await.unwrap();
        let welcome = frames.next().await.unwrap().unwrap();
        assert!(matches!(decode(&welcome).unwrap(), Message::Welcome { .. }));
        frames.send(encode(&hello)).await.unwrap();
        let reply = frames.next().await.unwrap().unwrap();
        assert!(matches!(decode(&reply).unwrap(), Message::Goodbye { .. }));
    }

    #[tokio::test]
    async fn only_subscribed_kinds_and_instruments_arrive() {
        let (mut sink, hub) = ChannelSink::new(8);
        sink.send(&update(1, &[(BookSide::Bid, "100", "1")]));
        let mut client = connect(&hub, vec![only(ID, true, false)]).await;

        sink.send(&update(2, &[(BookSide::Bid, "100", "2")]));
        sink.send(&trade(3));
        assert_eq!(next(&mut client).await, trade(3), "no snapshot, no update");

        client
            .unsubscribe(vec![only(ID, true, false)])
            .await
            .unwrap();
        client
            .subscribe(vec![only("ETH-PERP.SVP", true, true)])
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        sink.send(&trade(4));
        nothing_more(&mut client).await;
    }

    #[tokio::test]
    async fn subscribing_to_a_book_starts_it_from_a_snapshot() {
        let (mut sink, hub) = ChannelSink::new(8);
        sink.send(&update(1, &[(BookSide::Bid, "100", "1")]));
        let mut client = connect(&hub, vec![only(ID, true, false)]).await;

        client.subscribe(vec![only(ID, false, true)]).await.unwrap();
        assert_eq!(
            next(&mut client).await,
            Message::Book(BookUpdate {
                instrument: ID.into(),
                ts: 1,
                data: BookData::Snapshot {
                    bids: vec![(px("100"), qty("1"))],
                    asks: vec![],
                },
            })
        );
        sink.send(&update(2, &[(BookSide::Ask, "101", "1")]));
        assert_eq!(
            next(&mut client).await,
            update(2, &[(BookSide::Ask, "101", "1")])
        );
    }

    #[tokio::test]
    async fn updates_queued_before_a_subscription_are_not_applied_twice() {
        let (mut sink, hub) = ChannelSink::new(64);
        let mut filter = Filter::new(&[only(ID, true, false)]);
        let (_, mut rx) = hub.subscribe(|i| filter.wants_book(i));
        sink.send(&update(1, &[(BookSide::Bid, "100", "1")]));
        sink.send(&trade(2));

        let mut queue = Queue::default();
        let before = filter.clone();
        filter.add(&[only(ID, false, true)]);
        let snapshots = queue.catch_up(&hub, &rx, before, &filter);
        sink.send(&update(3, &[(BookSide::Bid, "100", "0")]));

        let mut passed = snapshots;
        while let Ok(message) = rx.try_recv() {
            if queue.filter_next(&filter).wants(&message) {
                passed.push(message);
            }
        }
        assert_eq!(passed.len(), 3, "snapshot, trade 2, update 3: {passed:?}");
        assert!(
            matches!(&passed[0], Message::Book(b) if matches!(b.data, BookData::Snapshot { .. }))
        );
        assert_eq!(passed[1], trade(2));
        assert_eq!(passed[2], update(3, &[(BookSide::Bid, "100", "0")]));
    }

    #[tokio::test]
    async fn a_client_goodbye_ends_the_session() {
        let (_sink, hub) = ChannelSink::new(8);
        let (client, server) = tokio::io::duplex(1 << 16);
        let session = tokio::spawn(async move {
            serve(
                &hub,
                Framed::new(server, LengthDelimitedCodec::new()),
                "test",
            )
            .await;
        });
        let client = Client::connect(
            Framed::new(client, LengthDelimitedCodec::new()),
            "test",
            vec![],
        )
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
        let (mut sink, hub) = ChannelSink::new(4);
        let mut client = Client::connect(
            connection(&hub, 64),
            "slow",
            vec![Subscription::everything()],
        )
        .await
        .unwrap();

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
