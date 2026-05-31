use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Revision(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevisionError {
    MissingPrefix,
    InvalidLength { actual: usize },
    InvalidHex,
}

impl Revision {
    pub fn parse(value: impl Into<String>) -> Result<Self, RevisionError> {
        let value = value.into();
        let Some(hex) = value.strip_prefix("sha256:") else {
            return Err(RevisionError::MissingPrefix);
        };

        if hex.len() != 64 {
            return Err(RevisionError::InvalidLength { actual: hex.len() });
        }

        if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(RevisionError::InvalidHex);
        }

        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let digest = Sha256::digest(bytes);
        Self(format!("sha256:{digest:x}"))
    }

    pub fn from_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];

        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }

        Ok(Self(format!("sha256:{:x}", hasher.finalize())))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Revision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for Revision {
    type Error = RevisionError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<Revision> for String {
    fn from(value: Revision) -> Self {
        value.0
    }
}

impl fmt::Display for RevisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPrefix => formatter.write_str("revision must start with sha256:"),
            Self::InvalidLength { actual } => {
                write!(
                    formatter,
                    "revision hex must be 64 characters, got {actual}"
                )
            }
            Self::InvalidHex => formatter.write_str("revision contains non-hex characters"),
        }
    }
}

impl std::error::Error for RevisionError {}

#[cfg(test)]
mod tests {
    use super::Revision;

    #[test]
    fn hashes_bytes_with_sha256_prefix() {
        let revision = Revision::from_bytes(b"abc");
        assert_eq!(
            revision.as_str(),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn rejects_revision_without_prefix() {
        assert!(Revision::parse("abc").is_err());
    }
}
