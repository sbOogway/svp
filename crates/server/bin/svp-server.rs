use std::{ffi::OsString, path::PathBuf};

use anyhow::Context;
use svp_common::unix;
use tracing_subscriber::EnvFilter;

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
    svp_server::run(&socket).await
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
        assert_eq!(socket_path(args(&[]), None).unwrap(), unix::default_path());
    }

    #[test]
    fn rejects_a_bare_flag_and_unknown_arguments() {
        assert!(socket_path(args(&["--socket"]), None).is_err());
        assert!(socket_path(args(&["--port", "1"]), None).is_err());
    }
}
