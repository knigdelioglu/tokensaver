//! Diagnostics boundary.
//!
//! This module owns health/doctor checks and redacted status reporting. It may
//! query explicit application/module status interfaces but must not inspect
//! private persistence directly or expose credentials/tool-result contents.
