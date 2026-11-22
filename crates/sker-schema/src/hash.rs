use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HashDigest(String);

impl HashDigest {
    pub fn new(value: String) -> Result<Self, HashError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(HashError::InvalidDigest(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HashDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HashError {
    #[error("invalid lowercase 64-character hex digest: {0}")]
    InvalidDigest(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HashChainEntry {
    pub sequence: u64,
    pub previous: Option<HashDigest>,
    pub payload_blake3: HashDigest,
    pub chain_blake3: HashDigest,
}

#[derive(Debug, Clone, Default)]
pub struct HashChain {
    entries: Vec<HashChainEntry>,
}

impl HashChain {
    pub fn entries(&self) -> &[HashChainEntry] {
        &self.entries
    }

    pub fn append(&mut self, payload: &[u8]) -> HashChainEntry {
        let previous = self.entries.last().map(|entry| entry.chain_blake3.clone());
        let payload_hash = blake3::hash(payload);
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"sker.hash-chain/v1\0");
        if let Some(previous_hash) = &previous {
            hasher.update(previous_hash.as_str().as_bytes());
        } else {
            hasher.update(b"genesis");
        }
        hasher.update(&(payload.len() as u64).to_be_bytes());
        hasher.update(payload);
        let entry = HashChainEntry {
            sequence: self.entries.len() as u64,
            previous,
            payload_blake3: HashDigest(payload_hash.to_hex().to_string()),
            chain_blake3: HashDigest(hasher.finalize().to_hex().to_string()),
        };
        self.entries.push(entry.clone());
        entry
    }

    pub fn verify(&self, payloads: &[Vec<u8>]) -> bool {
        if payloads.len() != self.entries.len() {
            return false;
        }
        let mut reconstructed = Self::default();
        for (payload, expected) in payloads.iter().zip(&self.entries) {
            let actual = reconstructed.append(payload);
            if &actual != expected {
                return false;
            }
        }
        true
    }
}
