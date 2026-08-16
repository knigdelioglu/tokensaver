#![forbid(unsafe_code)]

//! TokenSaver core library.
//!
//! The crate is organized as a modular monolith. Domain modules are private to
//! the product and are composed through the application layer. The public API
//! intentionally starts at `application`; internal modules must not be treated
//! as an accidental shared library surface.

pub mod application;
pub(crate) mod modules;
pub(crate) mod shared;
