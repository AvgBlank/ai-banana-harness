//! The sidecar protocol: newline-delimited JSON frames over a reader and a writer, served to the
//! desktop app by `banana-harness serve --stdio`. Protocol version 1.
#![deny(clippy::print_stdout, clippy::print_stderr)]

mod frame;
mod guard;
mod methods;
mod server;
mod turns;
mod workspaces;

pub use guard::claim_stdout;
pub use server::{BridgeOptions, serve};

/// The protocol version in the `ready` frame; a breaking change bumps it.
pub const PROTOCOL_VERSION: u32 = 1;

/// The only code that writes to stderr: method names, ids, codes and paths, never params,
/// results, event payloads or file content.
#[allow(clippy::print_stderr)]
pub(crate) fn log(message: &str) {
    eprintln!("{message}");
}
