//! Keeps a connection to the server open for as long as the app runs,
//! reconnecting with backoff, and reports it as [`Event`]s.

use std::{io, path::PathBuf, time::Duration};

use bytes::{Bytes, BytesMut};
use futures::{Sink, SinkExt, Stream, channel::mpsc::Sender};
use svp_common::protocol::Message;

use super::{Command, Commands, Event};
use crate::Client;

const NAME: &str = "svp-app";
const FIRST_RETRY: Duration = Duration::from_millis(250);
const LAST_RETRY: Duration = Duration::from_secs(5);

/// The connection to the server on `socket`.
pub fn connection(socket: PathBuf) -> iced::Subscription<Event> {
    iced::Subscription::run_with(socket, |socket| {
        let socket = socket.clone();
        iced::stream::channel(1024, async move |output| run(socket, output).await)
    })
}

async fn run(socket: PathBuf, mut output: Sender<Event>) {
    let mut retry_in = FIRST_RETRY;
    loop {
        let reason = match crate::connect(&socket, NAME).await {
            Ok(client) => {
                retry_in = FIRST_RETRY;
                match session(client, &mut output).await {
                    Some(reason) => reason,
                    None => return,
                }
            }
            Err(e) => format!("{}: {e}", socket.display()),
        };
        let event = Event::Disconnected { reason, retry_in };
        if output.send(event).await.is_err() {
            return;
        }
        tokio::time::sleep(retry_in).await;
        retry_in = (retry_in * 2).min(LAST_RETRY);
    }
}

/// Forwards what the server sends and sends what the app asks, until the
/// connection ends, with why; `None` once the app stops listening.
pub async fn session<T>(mut client: Client<T>, output: &mut Sender<Event>) -> Option<String>
where
    T: Stream<Item = io::Result<BytesMut>> + Sink<Bytes, Error = io::Error> + Unpin,
{
    let (commands, mut requests) = Commands::new();
    let connected = Event::Connected {
        session: client.session(),
        instruments: client.instruments().to_vec(),
        commands,
    };
    output.send(connected).await.ok()?;
    loop {
        tokio::select! {
            message = client.recv() => match message {
                Some(Ok(Message::Goodbye { reason })) => {
                    return Some(format!("the server said goodbye: {reason}"));
                }
                Some(Ok(message)) => output.send(Event::Received(message)).await.ok()?,
                Some(Err(e)) => return Some(e.to_string()),
                None => return Some("the server closed the connection".into()),
            },
            Some(command) = requests.recv() => {
                let sent = match command {
                    Command::Subscribe(subscriptions) => client.subscribe(subscriptions).await,
                    Command::Unsubscribe(subscriptions) => client.unsubscribe(subscriptions).await,
                };
                if let Err(e) = sent {
                    return Some(e.to_string());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use futures::StreamExt;
    use svp_common::protocol::{
        BookData, BookSide, BookUpdate, PROTOCOL_VERSION, Request, decode, encode,
    };
    use tokio::io::DuplexStream;
    use tokio_util::codec::{Framed, LengthDelimitedCodec};

    use super::{
        super::{
            Feed, Streams,
            tests::{instrument, snapshot, trade},
        },
        *,
    };

    type Frames = Framed<DuplexStream, LengthDelimitedCodec>;

    fn pipe() -> (Frames, Frames) {
        let (a, b) = tokio::io::duplex(1 << 16);
        (
            Framed::new(a, LengthDelimitedCodec::new()),
            Framed::new(b, LengthDelimitedCodec::new()),
        )
    }

    async fn request(server: &mut Frames) -> Request {
        decode(&server.next().await.unwrap().unwrap()).unwrap()
    }

    #[tokio::test]
    async fn a_session_over_frames_keeps_the_book_and_marks_a_gap() {
        let (client, mut server) = pipe();
        let server = tokio::spawn(async move {
            assert!(matches!(request(&mut server).await, Request::Hello { .. }));
            let welcome = Message::Welcome {
                session: 3,
                version: PROTOCOL_VERSION,
                instruments: vec![instrument("A")],
            };
            server.send(encode(&welcome)).await.unwrap();
            let Request::Subscribe { subscriptions } = request(&mut server).await else {
                panic!("expected a subscribe");
            };
            assert_eq!(subscriptions, [Streams::ALL.subscription("A")]);

            let update = Message::Book(BookUpdate {
                instrument: "A".into(),
                ts: 2,
                data: BookData::Update {
                    levels: vec![(BookSide::Bid, "100".parse().unwrap(), "2".parse().unwrap())],
                },
            });
            for message in [
                snapshot("A", "99", "101"),
                update,
                trade("A", "100.5"),
                Message::Resync { missed: 4 },
                Message::Goodbye {
                    reason: "bye".into(),
                },
            ] {
                server.send(encode(&message)).await.unwrap();
            }
        });

        let (mut output, mut events) = futures::channel::mpsc::channel(16);
        let session = tokio::spawn(async move {
            let client = Client::connect(client, NAME).await.unwrap();
            session(client, &mut output).await
        });

        let mut feed = Feed::default();
        feed.apply(events.next().await.unwrap());
        feed.sync(&BTreeMap::from([("A".into(), Streams::ALL)]));
        while let Some(event) = events.next().await {
            feed.apply(event);
        }
        server.await.unwrap();
        assert_eq!(
            session.await.unwrap().as_deref(),
            Some("the server said goodbye: bye")
        );

        let market = feed.market("A").unwrap();
        assert_eq!(
            market.book.snapshot(),
            BookData::Snapshot {
                bids: vec![
                    ("100".parse().unwrap(), "2".parse().unwrap()),
                    ("99".parse().unwrap(), "1".parse().unwrap()),
                ],
                asks: vec![("101".parse().unwrap(), "1".parse().unwrap())],
            }
        );
        assert_eq!((market.gaps, market.missed), (1, 4));
        assert_eq!(
            market.last_trade.as_ref().unwrap().price,
            "100.5".parse().unwrap()
        );
    }

    #[tokio::test]
    async fn a_session_ends_when_the_server_goes_away() {
        let (client, mut server) = pipe();
        let server = tokio::spawn(async move {
            request(&mut server).await;
            let welcome = Message::Welcome {
                session: 1,
                version: PROTOCOL_VERSION,
                instruments: vec![],
            };
            server.send(encode(&welcome)).await.unwrap();
        });
        let client = Client::connect(client, NAME).await.unwrap();
        server.await.unwrap();

        let (mut output, _events) = futures::channel::mpsc::channel(16);
        assert_eq!(
            session(client, &mut output).await.as_deref(),
            Some("the server closed the connection")
        );
    }
}
