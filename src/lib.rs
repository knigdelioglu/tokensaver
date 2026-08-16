#![forbid(unsafe_code)]

//! TokenSaver core and desktop application.
//!
//! The crate is organized as a modular monolith. Domain modules remain private;
//! the macOS menu-bar shell enters through the application layer rather than
//! reaching into transport, telemetry, or Codex configuration internals.

pub(crate) mod application;
mod desktop;
pub(crate) mod modules;
pub(crate) mod shared;

pub fn run_desktop() -> Result<(), Box<dyn std::error::Error>> {
    desktop::run()
}
