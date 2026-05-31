use crate::Revision;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub revision: Revision,
    pub updated_at: String,
    pub updated_by: String,
}

#[derive(Debug)]
pub enum ManifestError {
    Io(io::Error),
    Json(serde_json::Error),
    UnsupportedSchema { schema_version: u32 },
}

impl Manifest {
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;

    pub fn new(revision: Revision, updated_at: String, updated_by: String) -> Self {
        Self {
            schema_version: Self::CURRENT_SCHEMA_VERSION,
            revision,
            updated_at,
            updated_by,
        }
    }

    pub fn read(path: impl AsRef<Path>) -> Result<Self, ManifestError> {
        let contents = fs::read_to_string(path).map_err(ManifestError::Io)?;
        let manifest = serde_json::from_str::<Self>(&contents).map_err(ManifestError::Json)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn write_pretty(&self, path: impl AsRef<Path>) -> Result<(), ManifestError> {
        self.validate()?;
        let contents = serde_json::to_string_pretty(self).map_err(ManifestError::Json)?;
        fs::write(path, format!("{contents}\n")).map_err(ManifestError::Io)
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != Self::CURRENT_SCHEMA_VERSION {
            return Err(ManifestError::UnsupportedSchema {
                schema_version: self.schema_version,
            });
        }

        Ok(())
    }
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "manifest IO error: {error}"),
            Self::Json(error) => write!(formatter, "manifest JSON error: {error}"),
            Self::UnsupportedSchema { schema_version } => {
                write!(
                    formatter,
                    "unsupported manifest schema version: {schema_version}"
                )
            }
        }
    }
}

impl std::error::Error for ManifestError {}

#[cfg(test)]
mod tests {
    use super::Manifest;
    use crate::Revision;

    #[test]
    fn manifest_round_trips_through_json() {
        let manifest = Manifest::new(
            Revision::from_bytes(b"db"),
            "2026-05-31T12:00:00Z".to_string(),
            "macbook-pro".to_string(),
        );

        let json = serde_json::to_string(&manifest).unwrap();
        let parsed = serde_json::from_str::<Manifest>(&json).unwrap();

        assert_eq!(parsed, manifest);
    }

    #[test]
    fn unsupported_schema_fails_validation() {
        let manifest = Manifest {
            schema_version: 2,
            revision: Revision::from_bytes(b"db"),
            updated_at: "2026-05-31T12:00:00Z".to_string(),
            updated_by: "macbook-pro".to_string(),
        };

        assert!(manifest.validate().is_err());
    }
}
