use crate::Revision;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalState {
    pub schema_version: u32,
    pub device_id: String,
    pub base_revision: Option<Revision>,
}

#[derive(Debug)]
pub enum LocalStateError {
    Io(io::Error),
    Json(serde_json::Error),
    UnsupportedSchema { schema_version: u32 },
}

impl LocalState {
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;

    pub fn new(device_id: impl Into<String>) -> Self {
        Self {
            schema_version: Self::CURRENT_SCHEMA_VERSION,
            device_id: device_id.into(),
            base_revision: None,
        }
    }

    pub fn read_or_new(
        path: impl AsRef<Path>,
        device_id: impl Into<String>,
    ) -> Result<Self, LocalStateError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::new(device_id));
        }

        let contents = fs::read_to_string(path).map_err(LocalStateError::Io)?;
        let state = serde_json::from_str::<Self>(&contents).map_err(LocalStateError::Json)?;
        state.validate()?;
        Ok(state)
    }

    pub fn write_pretty(&self, path: impl AsRef<Path>) -> Result<(), LocalStateError> {
        self.validate()?;
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent).map_err(LocalStateError::Io)?;
        }
        let contents = serde_json::to_string_pretty(self).map_err(LocalStateError::Json)?;
        fs::write(path, format!("{contents}\n")).map_err(LocalStateError::Io)
    }

    pub fn with_base_revision(&self, base_revision: Revision) -> Self {
        Self {
            schema_version: self.schema_version,
            device_id: self.device_id.clone(),
            base_revision: Some(base_revision),
        }
    }

    fn validate(&self) -> Result<(), LocalStateError> {
        if self.schema_version != Self::CURRENT_SCHEMA_VERSION {
            return Err(LocalStateError::UnsupportedSchema {
                schema_version: self.schema_version,
            });
        }

        Ok(())
    }
}

impl std::fmt::Display for LocalStateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "local state IO error: {error}"),
            Self::Json(error) => write!(formatter, "local state JSON error: {error}"),
            Self::UnsupportedSchema { schema_version } => {
                write!(
                    formatter,
                    "unsupported local state schema version: {schema_version}"
                )
            }
        }
    }
}

impl std::error::Error for LocalStateError {}
