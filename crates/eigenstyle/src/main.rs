//! eigenstyle — MCP server for Matt Porter's voice profile.
//!
//! check_draft · get_exemplars · get_profile · ingest_sample ·
//! list_corpus · recompute_fingerprint
//!
//! As of 0.2.0 (WS1 of the MCP standardization program, decision
//! D-113) this server runs on rmcp, the official Rust MCP SDK, and
//! speaks the modern protocol era only (2026-07-28): sessions open
//! with `server/discover`, and every request carries its protocol
//! version in `_meta` — there is no `initialize` handshake. Legacy
//! contact is refused at the stdio front door (see `guard`) with the
//! typed `-32022` unsupported-version error, naming 2026-07-28 as
//! the one supported version. Storage lives in eigenstyle-core
//! (JSON on disk under the eigenstyle home); this crate is only the
//! wire. Corpus, fingerprint, and on-disk formats are unchanged.
//!
//! Usage: `eigenstyle [--home <dir>]`. The home defaults to the
//! platform data dir (`eigenstyle` under it) or `$EIGENSTYLE_HOME`.

#![forbid(unsafe_code)]

mod guard;
mod server;

use std::process::ExitCode;
use std::sync::Arc;

use eigenstyle_core::Store;
use rmcp::ServiceExt;
use rmcp::transport::async_rw::AsyncRwTransport;
use tokio::sync::Mutex;

/// Duplex buffer between the era guard and the SDK service. Request
/// lines can reach the 4 MiB line bound; the SDK consumes
/// incrementally, so this only needs to cover response bursts.
const DUPLEX_BUFFER_BYTES: usize = 1024 * 1024;

fn parse_home(args: &[String]) -> Option<Option<String>> {
    let mut home = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--home" => {
                i += 1;
                home = Some(args.get(i)?.clone());
            }
            other => {
                eprintln!("eigenstyle: unknown argument: {other}");
                return None;
            }
        }
        i += 1;
    }
    Some(home)
}

/// Wire the pieces together: the guard owns stdio, the SDK service
/// sits behind an in-process duplex, and a pump copies its responses
/// back to stdout through the same lock the guard writes with.
async fn run(store: Store) -> std::io::Result<()> {
    let (client_side, server_side) = tokio::io::duplex(DUPLEX_BUFFER_BYTES);
    let (server_read, server_write) = tokio::io::split(server_side);
    let (client_read, client_write) = tokio::io::split(client_side);
    let stdout: guard::SharedStdout = Arc::new(Mutex::new(tokio::io::stdout()));

    let ordering = guard::Ordering::new();
    let handler = server::EigenstyleServer::new(store);
    let serve = tokio::spawn(async move {
        let transport = AsyncRwTransport::new_server(server_read, server_write);
        match handler.serve(transport).await {
            Ok(running) => running
                .waiting()
                .await
                .map(|_| ())
                .map_err(|e| std::io::Error::other(e.to_string())),
            // The client closed the connection before selecting a
            // lifecycle (a probe, a spawn check, a client that only
            // ever wanted the refusal): the baseline treated bare
            // EOF as a clean end of session, and so do we.
            Err(rmcp::service::ServerInitializeError::ConnectionClosed(_)) => Ok(()),
            Err(e) => Err(std::io::Error::other(e.to_string())),
        }
    });
    let mut pump = tokio::spawn(guard::pump(
        client_read,
        Arc::clone(&stdout),
        Arc::clone(&ordering),
    ));

    // When stdin reaches EOF the guard shuts the duplex write side
    // down, which ends the SDK service; the pump ends when the
    // service's transport drops. The pump wait is bounded as a
    // backstop: a response still in flight when the service ends
    // gets its chance to drain, but a wedged pump cannot hang exit.
    let guard_result = guard::run(tokio::io::stdin(), client_write, stdout, ordering).await;
    let serve_result = match serve.await {
        Ok(result) => result,
        Err(e) => Err(std::io::Error::other(e.to_string())),
    };
    if tokio::time::timeout(std::time::Duration::from_secs(2), &mut pump)
        .await
        .is_err()
    {
        pump.abort();
    }
    guard_result.and(serve_result)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(home) = parse_home(&args) else {
        return ExitCode::from(2);
    };
    let dir = match home {
        Some(dir) => std::path::PathBuf::from(dir),
        None => Store::default_dir(),
    };
    let store = match Store::open(&dir) {
        Ok(store) => store,
        Err(e) => {
            eprintln!("eigenstyle: cannot open store at {}: {e}", dir.display());
            return ExitCode::from(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("eigenstyle: cannot start runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(store)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("eigenstyle: serve loop ended: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_home_accepts_forms() {
        assert_eq!(parse_home(&[]), Some(None));
        assert_eq!(
            parse_home(&["--home".to_string(), "/tmp/x".to_string()]),
            Some(Some("/tmp/x".to_string()))
        );
        assert_eq!(parse_home(&["--bogus".to_string()]), None);
        assert_eq!(parse_home(&["--home".to_string()]), None);
    }
}
