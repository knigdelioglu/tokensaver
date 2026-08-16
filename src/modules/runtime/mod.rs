//! Local runtime and lifecycle boundary.
//!
//! This module owns process/service lifecycle, startup policy, local runtime
//! state, and later start-at-login supervision. It must expose state through
//! application services instead of becoming a backdoor around module boundaries.
