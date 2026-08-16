//! Application composition boundary.
//!
//! Cross-module use cases belong here. UI, CLI, and future platform shells
//! should call application services rather than reaching into module internals.
//! Concrete use cases are intentionally introduced only in the phase that owns
//! their behavior; Phase 0 establishes the dependency boundary itself.
