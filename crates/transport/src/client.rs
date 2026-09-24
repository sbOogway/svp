//! The client's side of the connection scheme, over any transport that
//! carries frames both ways.

use std::io;

use bytes::{Bytes, BytesMut};
use futures::{Sink, SinkExt, Stream, StreamExt};
use svp_wire::{Instrument, Message, PROTOCOL_VERSION, Request, Subscription};

use crate::codec::{decode, encode};

#[derive(Debug)]
pub struct Client<T> {
    frames: T,
    session: u64,
    instruments: Vec<Instrument>,
}

impl<T> Client<T>
where
    T: Stream<Item = io::Result<BytesMut>> + Sink<Bytes, Error = io::Error> + Unpin,
{
    /// Says hello and waits for the server's welcome. A rejection is an
    /// [`io::ErrorKind::ConnectionRefused`] carrying the server's reason.
    pub async fn connect(mut frames: T, name: impl Into<String>) -> io::Result<Self> {
        let hello = Request::Hello {
            version: PROTOCOL_VERSION,
            name: name.into(),
        };
        frames.send(encode(&hello)).await?;
        match read(&mut frames).await {
            Some(Ok(Message::Welcome {
                session,
                instruments,
                ..
            })) => Ok(Self {
                frames,
                session,
                instruments,
            }),
            Some(Ok(Message::Reject { reason })) => {
                Err(io::Error::new(io::ErrorKind::ConnectionRefused, reason))
            }
            Some(Ok(message)) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("expected welcome, got {message:?}"),
            )),
            Some(Err(e)) => Err(e),
            None => Err(io::ErrorKind::UnexpectedEof.into()),
        }
    }

    /// The number the server's logs know this client by.
    pub fn session(&self) -> u64 {
        self.session
    }

    /// What the server offers to [`Self::subscribe`] to.
    pub fn instruments(&self) -> &[Instrument] {
        &self.instruments
    }

    /// `None` once the server has closed the connection.
    pub async fn recv(&mut self) -> Option<io::Result<Message>> {
        read(&mut self.frames).await
    }

    pub async fn subscribe(&mut self, subscriptions: Vec<Subscription>) -> io::Result<()> {
        self.frames
            .send(encode(&Request::Subscribe { subscriptions }))
            .await
    }

    pub async fn unsubscribe(&mut self, subscriptions: Vec<Subscription>) -> io::Result<()> {
        self.frames
            .send(encode(&Request::Unsubscribe { subscriptions }))
            .await
    }

    pub async fn goodbye(mut self, reason: impl Into<String>) -> io::Result<()> {
        let goodbye = Request::Goodbye {
            reason: reason.into(),
        };
        self.frames.send(encode(&goodbye)).await?;
        self.frames.close().await
    }
}

async fn read<T>(frames: &mut T) -> Option<io::Result<Message>>
where
    T: Stream<Item = io::Result<BytesMut>> + Unpin,
{
    let frame = match frames.next().await? {
        Ok(frame) => frame,
        Err(e) => return Some(Err(e)),
    };
    Some(decode(&frame).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)))
}
