#![forbid(unsafe_code)]

//! TokenSaver core, desktop, and CLI application.
//!
//! The crate is organized as a modular monolith. Domain modules remain private;
//! the macOS menu-bar shell and CLI both enter through the application layer
//! rather than reaching into transport, telemetry, or Codex configuration
//! internals.

pub(crate) mod application;
mod cli;
mod desktop;
pub(crate) mod modules;
pub(crate) mod shared;

pub fn should_run_cli(args: &[String]) -> bool {
    cli::is_cli_invocation(args)
}

pub fn run_cli(args: Vec<String>) -> Result<i32, Box<dyn std::error::Error>> {
    cli::run(args)
}

pub fn run_desktop() -> Result<(), Box<dyn std::error::Error>> {
    desktop::run()
}
