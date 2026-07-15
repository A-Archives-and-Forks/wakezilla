use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub const TIMESTAMP_HEADER: &str = "x-wakezilla-timestamp";
pub const NONCE_HEADER: &str = "x-wakezilla-nonce";
pub const SIGNATURE_HEADER: &str = "x-wakezilla-signature";
pub const MAX_CLOCK_SKEW: Duration = Duration::from_secs(60);

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedRequestHeaders {
    pub timestamp: String,
    pub nonce: String,
    pub signature: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyError {
    Malformed,
    TimestampOutsideWindow,
    InvalidSignature,
    Replay,
}

impl fmt::Display for VerifyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Malformed => "malformed authentication headers",
            Self::TimestampOutsideWindow => "request timestamp is outside the accepted window",
            Self::InvalidSignature => "invalid request signature",
            Self::Replay => "request nonce has already been used",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for VerifyError {}

pub fn generate_key() -> String {
    let key: [u8; 32] = rand::random();
    URL_SAFE_NO_PAD.encode(key)
}

pub fn validate_key(key: &str) -> Result<()> {
    decode_key(key).map(|_| ())
}

fn decode_key(key: &str) -> Result<[u8; 32]> {
    let decoded = URL_SAFE_NO_PAD
        .decode(key)
        .context("shutdown key is not valid URL-safe base64")?;
    decoded
        .try_into()
        .map_err(|_| anyhow!("shutdown key must decode to exactly 32 bytes"))
}

pub fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn sign_request(key: &str, method: &str, path: &str) -> Result<SignedRequestHeaders> {
    sign_request_at(key, method, path, unix_timestamp())
}

pub fn sign_request_at(
    key: &str,
    method: &str,
    path: &str,
    timestamp: u64,
) -> Result<SignedRequestHeaders> {
    let nonce_bytes: [u8; 16] = rand::random();
    let nonce = URL_SAFE_NO_PAD.encode(nonce_bytes);
    let timestamp = timestamp.to_string();
    let signature = signature(key, method, path, &timestamp, &nonce)?;
    Ok(SignedRequestHeaders {
        timestamp,
        nonce,
        signature,
    })
}

fn canonical_request(method: &str, path: &str, timestamp: &str, nonce: &str) -> String {
    format!(
        "wakezilla-v1\n{}\n{}\n{}\n{}",
        method.to_ascii_uppercase(),
        path,
        timestamp,
        nonce
    )
}

fn signature(key: &str, method: &str, path: &str, timestamp: &str, nonce: &str) -> Result<String> {
    let key = decode_key(key)?;
    let mut mac = HmacSha256::new_from_slice(&key).expect("HMAC accepts keys of any size");
    mac.update(canonical_request(method, path, timestamp, nonce).as_bytes());
    Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

pub struct ReplayGuard {
    seen: HashMap<String, u64>,
    retention: Duration,
    capacity: usize,
}

impl ReplayGuard {
    pub fn new(retention: Duration, capacity: usize) -> Self {
        Self {
            seen: HashMap::new(),
            retention,
            capacity: capacity.max(1),
        }
    }

    pub fn verify(
        &mut self,
        key: &str,
        method: &str,
        path: &str,
        headers: &SignedRequestHeaders,
        now: u64,
    ) -> std::result::Result<(), VerifyError> {
        let timestamp = headers
            .timestamp
            .parse::<u64>()
            .map_err(|_| VerifyError::Malformed)?;
        if now.abs_diff(timestamp) > MAX_CLOCK_SKEW.as_secs() {
            return Err(VerifyError::TimestampOutsideWindow);
        }

        self.prune(now);
        if self.seen.contains_key(&headers.nonce) {
            return Err(VerifyError::Replay);
        }

        let key = decode_key(key).map_err(|_| VerifyError::Malformed)?;
        let signature = URL_SAFE_NO_PAD
            .decode(&headers.signature)
            .map_err(|_| VerifyError::Malformed)?;
        let mut mac = HmacSha256::new_from_slice(&key).expect("HMAC accepts keys of any size");
        mac.update(canonical_request(method, path, &headers.timestamp, &headers.nonce).as_bytes());
        mac.verify_slice(&signature)
            .map_err(|_| VerifyError::InvalidSignature)?;

        if self.seen.len() >= self.capacity {
            self.remove_oldest();
        }
        self.seen.insert(headers.nonce.clone(), now);
        Ok(())
    }

    fn prune(&mut self, now: u64) {
        let retention = self.retention.as_secs();
        self.seen
            .retain(|_, accepted_at| now.saturating_sub(*accepted_at) <= retention);
    }

    fn remove_oldest(&mut self) {
        if let Some(oldest) = self
            .seen
            .iter()
            .min_by_key(|(_, accepted_at)| **accepted_at)
            .map(|(nonce, _)| nonce.clone())
        {
            self.seen.remove(&oldest);
        }
    }
}

impl Default for ReplayGuard {
    fn default() -> Self {
        Self::new(Duration::from_secs(120), 4096)
    }
}
