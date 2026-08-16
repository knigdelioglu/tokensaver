use rand::RngCore;
use rand::rngs::OsRng;

const CAPABILITY_BYTES: usize = 32;
const CAPABILITY_HEX_LENGTH: usize = CAPABILITY_BYTES * 2;
const API_PREFIX: &str = "v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallerCapability(String);

impl CallerCapability {
    pub(crate) fn generate() -> Self {
        let mut bytes = [0u8; CAPABILITY_BYTES];
        OsRng.fill_bytes(&mut bytes);
        Self(hex_encode(&bytes))
    }

    pub(crate) fn loopback_base_url(&self, port: u16) -> String {
        format!("http://127.0.0.1:{port}/{}/{API_PREFIX}", self.0)
    }

    /// Recover the port and capability from a TokenSaver-owned base URL. This is
    /// used after a restart so durable Codex config can reconnect to exactly the
    /// endpoint it already owns instead of silently rotating it.
    pub(crate) fn from_loopback_base_url(url: &str) -> Option<(u16, Self)> {
        let rest = url.strip_prefix("http://127.0.0.1:")?;
        let (port, path) = rest.split_once('/')?;
        let mut segments = path.split('/');
        let secret = segments.next()?;
        let api_prefix = segments.next()?;
        if segments.next().is_some()
            || api_prefix != API_PREFIX
            || secret.len() != CAPABILITY_HEX_LENGTH
            || !secret.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return None;
        }
        let port = port.parse::<u16>().ok()?;
        if port == 0 {
            return None;
        }
        Some((port, Self(secret.to_ascii_lowercase())))
    }

    /// Strip the secret prefix while retaining `/v1`, which is part of the
    /// configured built-in OpenAI base URL contract.
    pub(crate) fn authenticate_path<'a>(&self, path: &'a str) -> Option<&'a str> {
        let path = path.strip_prefix('/')?;
        let (candidate, remainder) = path.split_once('/')?;
        if !constant_time_equal(candidate.as_bytes(), self.0.as_bytes()) {
            return None;
        }
        if remainder != API_PREFIX && !remainder.starts_with("v1/") {
            return None;
        }
        path.get(candidate.len()..)
    }

    #[cfg(test)]
    pub(super) fn from_secret(secret: &str) -> Self {
        Self(secret.to_owned())
    }
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in left.iter().zip(right.iter()) {
        difference |= left ^ right;
    }
    difference == 0
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 0x0f) as usize] as char);
    }
    result
}
