//! Serves one client, whatever transport carries its frames.

use bytes::Bytes;
use futures::{Sink, SinkExt};
use svp_wire::Message;
use tokio::sync::broadcast::error::RecvError;

use crate::{codec::encode, sink::Hub};

/// Sends every book's snapshot, then everything the hub broadcasts, until the
/// hub closes or a send fails. A client that falls behind the hub's capacity
/// gets [`Message::Resync`] and fresh snapshots instead of growing a queue.
pub async fn serve<S>(hub: &Hub, mut frames: S) -> Result<(), S::Error>
where
    S: Sink<Bytes> + Unpin,
{
    let (snapshots, mut rx) = hub.subscribe();
    for message in &snapshots {
        frames.send(encode(message)).await?;
    }
    loop {
        match rx.recv().await {
            Ok(message) => frames.send(encode(&message)).await?,
            Err(RecvError::Lagged(missed)) => {
                tracing::warn!(missed, "client fell behind, resyncing");
                let (snapshots, resubscribed) = hub.subscribe();
                rx = resubscribed;
                frames.send(encode(&Message::Resync { missed })).await?;
                for message in &snapshots {
                    frames.send(encode(message)).await?;
                }
            }
            Err(RecvError::Closed) => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::{StreamExt, channel::mpsc};
    use svp_wire::{Book, BookData, BookSide};

    use super::*;
    use crate::{
        codec::decode,
        sink::{
            ChannelSink, Sink as _,
            tests::{trade, update},
        },
    };

    async fn next(frames: &mut mpsc::Receiver<Bytes>) -> Message {
        let frame = tokio::time::timeout(Duration::from_secs(1), frames.next())
            .await
            .expect("a frame within a second")
            .expect("the session is still running");
        decode(&frame).unwrap()
    }

    fn apply(book: &mut Book, message: &Message) {
        if let Message::Book(update) = message {
            book.apply(&update.data);
        }
    }

    #[tokio::test]
    async fn snapshots_first_then_messages_in_order() {
        let (mut sink, hub) = ChannelSink::new(8);
        sink.send(&update(1, &[(BookSide::Bid, "100", "1")]));
        let (tx, mut frames) = mpsc::channel(8);
        tokio::spawn(async move { serve(&hub, tx).await });

        let Message::Book(snapshot) = next(&mut frames).await else {
            panic!("expected a snapshot first");
        };
        assert!(matches!(snapshot.data, BookData::Snapshot { .. }));

        sink.send(&trade(2));
        sink.send(&update(3, &[(BookSide::Ask, "101", "1")]));
        assert_eq!(next(&mut frames).await, trade(2));
        assert_eq!(
            next(&mut frames).await,
            update(3, &[(BookSide::Ask, "101", "1")])
        );
    }

    #[tokio::test]
    async fn a_slow_client_resyncs_from_snapshots() {
        let (mut sink, hub) = ChannelSink::new(4);
        let (tx, mut frames) = mpsc::channel(0);
        let server_hub = hub.clone();
        tokio::spawn(async move { serve(&server_hub, tx).await });
        tokio::task::yield_now().await;

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
            let message = next(&mut frames).await;
            if let Message::Resync { missed: n } = message {
                missed = Some(n);
            }
            apply(&mut book, &message);
            if matches!(&message, Message::Book(u) if u.ts == 49) {
                break;
            }
        }
        assert!(missed.is_some_and(|n| n > 0));

        let (snapshots, _) = hub.subscribe();
        let mut expected = Book::default();
        apply(&mut expected, &snapshots[0]);
        assert_eq!(book, expected);
    }
}
