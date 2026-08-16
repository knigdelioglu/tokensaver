//! Native Codex transport boundary.
//!
//! This module owns loopback request handling, request-body compression,
//! streaming relay, cancellation propagation, and transport compatibility.
//! It may invoke the aging module through an explicit application/domain
//! contract, but it must not own aging policy or Codex configuration files.
