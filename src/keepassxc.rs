use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct Keepassxc {
    binary: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeepassMerge {
    pub database: PathBuf,
    pub database_from: PathBuf,
    pub same_credentials: bool,
    pub password: Option<String>,
}

#[derive(Debug)]
pub enum KeepassxcError {
    Spawn(std::io::Error),
    Stdin(std::io::Error),
    Failed { status: i32, stderr: String },
}

impl Keepassxc {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
        }
    }

    pub fn default_binary() -> Self {
        Self::new("keepassxc-cli")
    }

    pub fn merge(&self, request: &KeepassMerge) -> Result<(), KeepassxcError> {
        let mut command = Command::new(&self.binary);
        command.arg("merge").arg("-q");
        if request.same_credentials {
            command.arg("--same-credentials");
        }
        command.arg(&request.database).arg(&request.database_from);

        if request.password.is_some() {
            command.stdin(Stdio::piped());
        }
        command.stderr(Stdio::piped());

        let mut child = command.spawn().map_err(KeepassxcError::Spawn)?;
        if let Some(password) = &request.password {
            let mut stdin = child.stdin.take().expect("stdin is piped");
            stdin
                .write_all(format!("{password}\n").as_bytes())
                .map_err(KeepassxcError::Stdin)?;
        }

        let output = child.wait_with_output().map_err(KeepassxcError::Spawn)?;
        if output.status.success() {
            return Ok(());
        }

        Err(KeepassxcError::Failed {
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }

    pub fn is_available(&self) -> bool {
        Command::new(&self.binary)
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub fn binary(&self) -> &Path {
        &self.binary
    }
}

impl Default for Keepassxc {
    fn default() -> Self {
        Self::default_binary()
    }
}

impl std::fmt::Display for KeepassxcError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(error) => write!(formatter, "failed to run keepassxc-cli: {error}"),
            Self::Stdin(error) => write!(formatter, "failed to send KeePass password: {error}"),
            Self::Failed { status, stderr } => {
                write!(
                    formatter,
                    "keepassxc-cli failed with status {status}: {stderr}"
                )
            }
        }
    }
}

impl std::error::Error for KeepassxcError {}
