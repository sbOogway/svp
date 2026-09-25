//! Frames over a Unix domain socket, each prefixed with its length.

use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

use tokio::net::{UnixListener, UnixStream};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

pub type Connection = Framed<UnixStream, LengthDelimitedCodec>;

/// `$XDG_RUNTIME_DIR/svp.sock`, or `svp.sock` in the temp dir without it.
pub fn default_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("svp.sock")
}

/// The socket a binary uses, from its arguments (without the program
/// name) and `SVP_SOCKET`: `--socket PATH`, else the variable, else
/// [`default_path`].
pub fn socket_path(
    mut args: impl Iterator<Item = OsString>,
    env: Option<OsString>,
) -> io::Result<PathBuf> {
    let invalid = |message: String| io::Error::new(io::ErrorKind::InvalidInput, message);
    match args.next() {
        None => Ok(env.map_or_else(default_path, PathBuf::from)),
        Some(flag) if flag == "--socket" => args
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| invalid("--socket needs a path".into())),
        Some(other) => Err(invalid(format!("unknown argument {}", other.display()))),
    }
}

/// Accepts connections on a socket file, which it removes when dropped.
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

    /// The next connection, and what is known of its peer for the logs.
    pub async fn accept(&self) -> io::Result<(Connection, String)> {
        let (stream, _) = self.listener.accept().await?;
        let peer = match stream.peer_cred().ok().and_then(|cred| cred.pid()) {
            Some(pid) => format!("unix pid {pid}"),
            None => "unix".to_owned(),
        };
        Ok((Framed::new(stream, LengthDelimitedCodec::new()), peer))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub async fn connect(path: &Path) -> io::Result<Connection> {
    let stream = UnixStream::connect(path).await?;
    Ok(Framed::new(stream, LengthDelimitedCodec::new()))
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use futures::{SinkExt, StreamExt};

    use super::*;

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
        assert_eq!(socket_path(args(&[]), None).unwrap(), default_path());
    }

    #[test]
    fn rejects_a_bare_flag_and_unknown_arguments() {
        assert!(socket_path(args(&["--socket"]), None).is_err());
        assert!(socket_path(args(&["--port", "1"]), None).is_err());
    }

    #[tokio::test]
    async fn frames_cross_the_socket_both_ways() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("svp.sock");
        let server = Server::bind(&path).await.unwrap();

        let mut client = connect(&path).await.unwrap();
        let (mut accepted, peer) = server.accept().await.unwrap();
        assert_eq!(peer, format!("unix pid {}", std::process::id()));

        client.send(Bytes::from_static(b"hello")).await.unwrap();
        assert_eq!(accepted.next().await.unwrap().unwrap(), &b"hello"[..]);
        accepted.send(Bytes::from_static(b"welcome")).await.unwrap();
        assert_eq!(client.next().await.unwrap().unwrap(), &b"welcome"[..]);

        drop(server);
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
