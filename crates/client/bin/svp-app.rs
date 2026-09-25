//! The svp desktop app, a client of a running `svp-server`:
//! `cargo run -p svp-client --features app --bin svp-app [-- --socket PATH]`.

use std::process::ExitCode;

use svp_client::socket_path;

fn main() -> ExitCode {
    let socket = match socket_path(std::env::args_os().skip(1), std::env::var_os("SVP_SOCKET")) {
        Ok(socket) => socket,
        Err(e) => {
            eprintln!("svp-app: {e}");
            return ExitCode::FAILURE;
        }
    };
    match svp_client::charts::run(socket) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("svp-app: {e}");
            ExitCode::FAILURE
        }
    }
}
