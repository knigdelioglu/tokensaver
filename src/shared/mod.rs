//! Narrow cross-cutting primitives only.
//!
//! `shared` is not a home for domain logic. Only concerns that are genuinely
//! cross-cutting and low-level belong here, such as common error primitives,
//! filesystem safety helpers, or security utilities introduced by later phases.

pub(crate) mod filesystem;
