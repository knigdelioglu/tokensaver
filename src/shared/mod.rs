//! Narrow cross-cutting primitives only.
//!
//! `shared` is not a home for domain logic. Only concerns that are genuinely
//! cross-cutting and low-level belong here, such as common error primitives,
//! filesystem safety helpers, product-local paths, and outward secret-redaction
//! utilities.

pub(crate) mod filesystem;
pub(crate) mod paths;
pub(crate) mod security;
