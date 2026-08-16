//! Native Codex transport boundary.
//!
//! This module owns loopback request handling, request-body compression,
//! streaming relay, cancellation propagation, and transport compatibility.
//! It invokes the aging domain through an explicit normalized contract, but it
//! does not own aging policy or Codex configuration files.

mod capability;
mod compression;
mod headers;
mod observation;
mod request;
mod response_usage;
mod server;

pub(crate) use capability::CallerCapability;
pub(crate) use observation::TransportObservation;
pub(crate) use request::{PreparationOutcome, RequestDiagnostics};
pub(crate) use response_usage::ProviderUsageObservation;
pub(crate) use server::{BoundTransport, TransportControl, TransportError, TransportSettings};

#[cfg(test)]
mod tests;
