//! Frames over a Unix domain socket, each prefixed with its length.

use std::{
    io,
    path::{Path, PathBuf},
};

use svp_wire::Subscription;
use tokio::net::{UnixListener, UnixStream};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use crate::{client::Client, session, sink::Hub};

pub type Connection = Framed<UnixStream, LengthDelimitedCodec>;

/// `$XDG_RUNTIME_DIR/svp.sock`, or `svp.sock` in the temp dir without it.
pub fn default_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("svp.sock")
}

/// Removes its socket file when dropped.
#[derive(Debug)]
pub struct Server {
    listener: UnixListener,
    path: PathBuf,
}

impl Server {
    /// Replaces a socket file left by a server that died, but not one a
    /// server is still listening on.
    pub async fn bind(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        if path.exists() {
            if UnixStream::connect(&path).await.is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!("a server is already listening on {}", path.display()),
                ));
            }
            std::fs::remove_file(&path)?;
        }
        let listener = UnixListener::bind(&path)?;
        Ok(Self { listener, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Hands each client that connects to [`session::serve`] in its own task.
    pub async fn run(self, hub: Hub) -> io::Result<()> {
        loop {
            let (stream, _) = self.listener.accept().await?;
            let peer = match stream.peer_cred().ok().and_then(|cred| cred.pid()) {
                Some(pid) => format!("unix pid {pid}"),
                None => "unix".to_owned(),
            };
            let hub = hub.clone();
            tokio::spawn(async move {
                let frames = Framed::new(stream, LengthDelimitedCodec::new());
                session::serve(&hub, frames, &peer).await;
            });
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub async fn connect(
    path: &Path,
    name: impl Into<String>,
    subscriptions: Vec<Subscription>,
) -> io::Result<Client<Connection>> {
    let stream = UnixStream::connect(path).await?;
    let frames = Framed::new(stream, LengthDelimitedCodec::new());
    Client::connect(frames, name, subscriptions).await
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use svp_wire::{BookData, BookSide, Message};

    use super::*;
    use crate::sink::{
        ChannelSink, Sink as _,
        tests::{px, qty, trade, update},
    };

    async fn next(client: &mut Client<Connection>) -> Message {
        tokio::time::timeout(Duration::from_secs(1), client.recv())
            .await
            .expect("a message within a second")
            .expect("the server is still running")
            .unwrap()
    }

    #[tokio::test]
    async fn a_client_receives_snapshots_then_live_messages() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("svp.sock");
        let (mut sink, hub) = ChannelSink::new(8);
        sink.send(&update(1, &[(BookSide::Bid, "100", "1")]));
        let server = Server::bind(&path).await.unwrap();
        let task = tokio::spawn(server.run(hub));

        let mut messages = connect(&path, "test", vec![Subscription::everything()])
            .await
            .unwrap();
        let Message::Book(snapshot) = next(&mut messages).await else {
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
        assert_eq!(next(&mut messages).await, trade(2));

        task.abort();
        let _ = task.await;
        assert!(!path.exists(), "the socket file is removed with the server");
    }

    #[tokio::test]
    async fn bind_refuses_a_live_socket_and_replaces_a_dead_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("svp.sock");

        let live = Server::bind(&path).await.unwrap();
        let err = Server::bind(&path).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AddrInUse);
        drop(live);

        // A server that died without cleaning up leaves its file behind.
        drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
        assert!(path.exists());
        Server::bind(&path).await.unwrap();
    }
}
