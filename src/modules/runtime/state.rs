use std::sync::{Arc, RwLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ServiceStatus {
    #[default]
    Stopped,
    Starting,
    Running,
    Error,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CodexStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Drifted,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RuntimeStatus {
    pub(crate) service: ServiceStatus,
    pub(crate) codex: CodexStatus,
    pub(crate) saving_enabled: bool,
    pub(crate) active_requests: usize,
    pub(crate) last_error: Option<String>,
}

impl Default for RuntimeStatus {
    fn default() -> Self {
        Self {
            service: ServiceStatus::Stopped,
            codex: CodexStatus::Disconnected,
            saving_enabled: true,
            active_requests: 0,
            last_error: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RuntimeStatusStore {
    inner: Arc<RwLock<RuntimeStatus>>,
}

impl RuntimeStatusStore {
    pub(crate) fn snapshot(&self) -> RuntimeStatus {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub(crate) fn update(&self, update: impl FnOnce(&mut RuntimeStatus)) {
        let mut state = self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        update(&mut state);
    }
}
