use crate::keepassxc::{KeepassMerge, Keepassxc, KeepassxcError};
use crate::{LocalState, Manifest, Revision, SyncAction, SyncExecutionReport, SyncInputs};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct FilesystemRemote {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingDatabase {
    pub device_id: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOutcome {
    pub report: SyncExecutionReport,
    pub local_revision: Revision,
    pub remote_revision: Option<Revision>,
}

#[derive(Debug)]
pub enum RemoteError {
    Io(io::Error),
    Manifest(crate::manifest::ManifestError),
    LocalState(crate::local_state::LocalStateError),
    Sync(crate::sync::SyncProblem),
    Keepassxc(KeepassxcError),
    ManifestHashMismatch {
        manifest: Revision,
        actual: Revision,
    },
    BaseRevisionMismatch {
        expected: Option<Revision>,
        actual: Option<Revision>,
    },
    LockHeld(PathBuf),
    NoCanonicalDatabase,
}

impl FilesystemRemote {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn sync(
        &self,
        local_db: impl AsRef<Path>,
        state_path: impl AsRef<Path>,
        device_id: &str,
    ) -> Result<SyncOutcome, RemoteError> {
        self.ensure_layout()?;

        let local_db = local_db.as_ref();
        let state_path = state_path.as_ref();
        let local_state = LocalState::read_or_new(state_path, device_id)?;
        let local_revision = Revision::from_file(local_db)?;
        let remote_revision = self.remote_revision()?;

        let inputs = SyncInputs {
            local_revision: local_revision.clone(),
            base_revision: local_state.base_revision.clone(),
            remote_revision: remote_revision.clone(),
        };
        let action = crate::sync::decide_sync(&inputs)?;

        let report = match action {
            SyncAction::InitializeRemote | SyncAction::PublishLocal => self.with_lock(|| {
                self.publish(local_db, &local_revision, device_id)?;
                local_state
                    .with_base_revision(local_revision.clone())
                    .write_pretty(state_path)?;
                Ok(SyncExecutionReport::published(action))
            })?,
            SyncAction::AdoptRemote => {
                let remote = remote_revision
                    .clone()
                    .expect("adopt remote only happens when remote exists");
                local_state
                    .with_base_revision(remote)
                    .write_pretty(state_path)?;
                SyncExecutionReport::adopted()
            }
            SyncAction::Noop => SyncExecutionReport::noop(),
            SyncAction::PullRemote => {
                let remote = remote_revision
                    .clone()
                    .expect("pull remote only happens when remote exists");
                self.backup_local(local_db, device_id)?;
                fs::copy(self.canonical_db(), local_db).map_err(RemoteError::Io)?;
                local_state
                    .with_base_revision(remote)
                    .write_pretty(state_path)?;
                SyncExecutionReport::pulled()
            }
            SyncAction::PreserveIncoming => self.with_lock(|| {
                let incoming = self.preserve_incoming(local_db, &local_revision, device_id)?;
                Ok(SyncExecutionReport::preserved_incoming(incoming))
            })?,
        };

        Ok(SyncOutcome {
            report,
            local_revision,
            remote_revision,
        })
    }

    pub fn merge_incoming(
        &self,
        device_id: &str,
        keepassxc: &Keepassxc,
        password: Option<String>,
    ) -> Result<Vec<PathBuf>, RemoteError> {
        self.ensure_layout()?;
        self.with_lock(|| {
            let incoming = self.incoming_databases()?;
            let mut archived = Vec::new();

            for database in incoming {
                self.backup_remote(device_id)?;
                let work_path = self.root.join(format!(
                    ".merge-{}-{}.kdbx",
                    database.device_id,
                    timestamp()
                ));
                fs::copy(self.canonical_db(), &work_path).map_err(RemoteError::Io)?;

                keepassxc.merge(&KeepassMerge {
                    database: work_path.clone(),
                    database_from: database.path.clone(),
                    same_credentials: true,
                    password: password.clone(),
                })?;

                let revision = Revision::from_file(&work_path)?;
                self.publish(&work_path, &revision, device_id)?;
                fs::remove_file(&work_path).map_err(RemoteError::Io)?;

                let archive_path = self.archive_incoming(&database)?;
                archived.push(archive_path);
            }

            Ok(archived)
        })
    }

    pub fn incoming_databases(&self) -> Result<Vec<IncomingDatabase>, RemoteError> {
        let incoming_root = self.incoming_dir();
        if !incoming_root.exists() {
            return Ok(Vec::new());
        }

        let mut databases = Vec::new();
        for device in fs::read_dir(incoming_root).map_err(RemoteError::Io)? {
            let device = device.map_err(RemoteError::Io)?;
            if !device.file_type().map_err(RemoteError::Io)?.is_dir() {
                continue;
            }
            let device_id = device.file_name().to_string_lossy().to_string();
            for file in fs::read_dir(device.path()).map_err(RemoteError::Io)? {
                let file = file.map_err(RemoteError::Io)?;
                if file
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "kdbx")
                {
                    databases.push(IncomingDatabase {
                        device_id: device_id.clone(),
                        path: file.path(),
                    });
                }
            }
        }
        databases.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(databases)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn current_revision(&self) -> Result<Option<Revision>, RemoteError> {
        self.remote_revision()
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, RemoteError> {
        if self.remote_revision()?.is_none() {
            return Err(RemoteError::NoCanonicalDatabase);
        }
        fs::read(self.canonical_db()).map_err(RemoteError::Io)
    }

    pub fn publish_bytes_if_base_matches(
        &self,
        bytes: &[u8],
        base_revision: Option<&Revision>,
        revision: &Revision,
        device_id: &str,
    ) -> Result<Manifest, RemoteError> {
        self.ensure_layout()?;
        self.with_lock(|| {
            let actual_base = self.remote_revision()?;
            if actual_base.as_ref() != base_revision {
                return Err(RemoteError::BaseRevisionMismatch {
                    expected: base_revision.cloned(),
                    actual: actual_base,
                });
            }

            let tmp_path = self.root.join(format!(
                ".upload-{}-{}.kdbx",
                sanitize_path_component(device_id),
                timestamp()
            ));
            fs::write(&tmp_path, bytes).map_err(RemoteError::Io)?;
            let result = self.publish(&tmp_path, revision, device_id);
            let cleanup = fs::remove_file(&tmp_path).map_err(RemoteError::Io);

            result?;
            cleanup?;
            Ok(Manifest::new(
                revision.clone(),
                timestamp(),
                device_id.to_string(),
            ))
        })
    }

    pub fn preserve_incoming_bytes(
        &self,
        bytes: &[u8],
        revision: &Revision,
        device_id: &str,
    ) -> Result<PathBuf, RemoteError> {
        self.ensure_layout()?;
        self.with_lock(|| {
            let device_dir = self.incoming_dir().join(sanitize_path_component(device_id));
            fs::create_dir_all(&device_dir).map_err(RemoteError::Io)?;
            let incoming_path = device_dir.join(format!("{}.kdbx", safe_revision(revision)));
            let tmp_path = incoming_path.with_extension("kdbx.tmp");
            fs::write(&tmp_path, bytes).map_err(RemoteError::Io)?;
            let actual = Revision::from_file(&tmp_path).map_err(RemoteError::Io)?;
            if &actual != revision {
                return Err(RemoteError::ManifestHashMismatch {
                    manifest: revision.clone(),
                    actual,
                });
            }
            fs::rename(tmp_path, &incoming_path).map_err(RemoteError::Io)?;
            Ok(incoming_path)
        })
    }

    fn publish(
        &self,
        local_db: &Path,
        revision: &Revision,
        device_id: &str,
    ) -> Result<(), RemoteError> {
        if self.canonical_db().exists() {
            self.backup_remote(device_id)?;
        }

        let tmp_db = self.canonical_dir().join("passwords.kdbx.tmp");
        fs::copy(local_db, &tmp_db).map_err(RemoteError::Io)?;
        let copied_revision = Revision::from_file(&tmp_db)?;
        if &copied_revision != revision {
            return Err(RemoteError::ManifestHashMismatch {
                manifest: revision.clone(),
                actual: copied_revision,
            });
        }
        fs::rename(&tmp_db, self.canonical_db()).map_err(RemoteError::Io)?;

        let manifest = Manifest::new(revision.clone(), timestamp(), device_id.to_string());
        let tmp_manifest = self.canonical_dir().join("manifest.json.tmp");
        manifest.write_pretty(&tmp_manifest)?;
        fs::rename(tmp_manifest, self.manifest_path()).map_err(RemoteError::Io)?;
        Ok(())
    }

    fn preserve_incoming(
        &self,
        local_db: &Path,
        revision: &Revision,
        device_id: &str,
    ) -> Result<PathBuf, RemoteError> {
        let device_dir = self.incoming_dir().join(device_id);
        fs::create_dir_all(&device_dir).map_err(RemoteError::Io)?;
        let incoming_path = device_dir.join(format!("{}.kdbx", safe_revision(revision)));
        let tmp_path = incoming_path.with_extension("kdbx.tmp");
        fs::copy(local_db, &tmp_path).map_err(RemoteError::Io)?;
        fs::rename(tmp_path, &incoming_path).map_err(RemoteError::Io)?;
        Ok(incoming_path)
    }

    fn archive_incoming(&self, incoming: &IncomingDatabase) -> Result<PathBuf, RemoteError> {
        let archive_dir = self
            .incoming_dir()
            .join(".archive")
            .join(&incoming.device_id);
        fs::create_dir_all(&archive_dir).map_err(RemoteError::Io)?;
        let name = incoming
            .path
            .file_name()
            .map(|name| name.to_owned())
            .unwrap_or_else(|| "incoming.kdbx".into());
        let archive_path = archive_dir.join(format!("{}-{}", timestamp(), name.to_string_lossy()));
        fs::rename(&incoming.path, &archive_path).map_err(RemoteError::Io)?;
        Ok(archive_path)
    }

    fn remote_revision(&self) -> Result<Option<Revision>, RemoteError> {
        if !self.manifest_path().exists() {
            return Ok(None);
        }
        if !self.canonical_db().exists() {
            return Err(RemoteError::NoCanonicalDatabase);
        }

        let manifest = Manifest::read(self.manifest_path())?;
        let actual = Revision::from_file(self.canonical_db())?;
        if manifest.revision != actual {
            return Err(RemoteError::ManifestHashMismatch {
                manifest: manifest.revision,
                actual,
            });
        }
        Ok(Some(manifest.revision))
    }

    fn backup_remote(&self, device_id: &str) -> Result<Option<PathBuf>, RemoteError> {
        if !self.canonical_db().exists() {
            return Ok(None);
        }

        fs::create_dir_all(self.backups_dir()).map_err(RemoteError::Io)?;
        let path = self
            .backups_dir()
            .join(format!("{}-{device_id}-remote.kdbx", timestamp()));
        fs::copy(self.canonical_db(), &path).map_err(RemoteError::Io)?;
        Ok(Some(path))
    }

    fn backup_local(&self, local_db: &Path, device_id: &str) -> Result<PathBuf, RemoteError> {
        fs::create_dir_all(self.backups_dir()).map_err(RemoteError::Io)?;
        let path = self
            .backups_dir()
            .join(format!("{}-{device_id}-local.kdbx", timestamp()));
        fs::copy(local_db, &path).map_err(RemoteError::Io)?;
        Ok(path)
    }

    fn ensure_layout(&self) -> Result<(), RemoteError> {
        fs::create_dir_all(self.canonical_dir()).map_err(RemoteError::Io)?;
        fs::create_dir_all(self.incoming_dir()).map_err(RemoteError::Io)?;
        fs::create_dir_all(self.backups_dir()).map_err(RemoteError::Io)?;
        Ok(())
    }

    fn with_lock<T>(
        &self,
        work: impl FnOnce() -> Result<T, RemoteError>,
    ) -> Result<T, RemoteError> {
        let lock = self.lock_dir();
        match fs::create_dir(&lock) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(RemoteError::LockHeld(lock));
            }
            Err(error) => return Err(RemoteError::Io(error)),
        }

        let result = work();
        let unlock_result = fs::remove_dir(&lock).map_err(RemoteError::Io);

        match (result, unlock_result) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }

    fn canonical_dir(&self) -> PathBuf {
        self.root.join("canonical")
    }

    fn canonical_db(&self) -> PathBuf {
        self.canonical_dir().join("passwords.kdbx")
    }

    fn manifest_path(&self) -> PathBuf {
        self.canonical_dir().join("manifest.json")
    }

    fn incoming_dir(&self) -> PathBuf {
        self.root.join("incoming")
    }

    fn backups_dir(&self) -> PathBuf {
        self.root.join("backups")
    }

    fn lock_dir(&self) -> PathBuf {
        self.root.join(".lock")
    }
}

impl From<io::Error> for RemoteError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<crate::manifest::ManifestError> for RemoteError {
    fn from(error: crate::manifest::ManifestError) -> Self {
        Self::Manifest(error)
    }
}

impl From<crate::local_state::LocalStateError> for RemoteError {
    fn from(error: crate::local_state::LocalStateError) -> Self {
        Self::LocalState(error)
    }
}

impl From<crate::sync::SyncProblem> for RemoteError {
    fn from(error: crate::sync::SyncProblem) -> Self {
        Self::Sync(error)
    }
}

impl From<KeepassxcError> for RemoteError {
    fn from(error: KeepassxcError) -> Self {
        Self::Keepassxc(error)
    }
}

impl std::fmt::Display for RemoteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "remote filesystem error: {error}"),
            Self::Manifest(error) => write!(formatter, "{error}"),
            Self::LocalState(error) => write!(formatter, "{error}"),
            Self::Sync(error) => write!(formatter, "{error}"),
            Self::Keepassxc(error) => write!(formatter, "{error}"),
            Self::ManifestHashMismatch { manifest, actual } => {
                write!(
                    formatter,
                    "manifest revision {manifest} does not match canonical database hash {actual}"
                )
            }
            Self::BaseRevisionMismatch { expected, actual } => {
                write!(
                    formatter,
                    "remote base revision changed: expected {}, got {}",
                    display_optional_revision(expected),
                    display_optional_revision(actual)
                )
            }
            Self::LockHeld(path) => {
                write!(formatter, "remote lock is already held: {}", path.display())
            }
            Self::NoCanonicalDatabase => {
                formatter.write_str("manifest exists but canonical database is missing")
            }
        }
    }
}

impl std::error::Error for RemoteError {}

fn timestamp() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}

fn safe_revision(revision: &Revision) -> String {
    revision.as_str().replace(':', "-")
}

fn sanitize_path_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    if sanitized.is_empty() {
        "unknown-device".to_string()
    } else {
        sanitized
    }
}

fn display_optional_revision(revision: &Option<Revision>) -> String {
    revision
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "none".to_string())
}

#[cfg(test)]
mod tests {
    use super::FilesystemRemote;
    use crate::{LocalState, Revision, SyncReportKind};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn initializes_remote_from_local_database() {
        let dir = tempdir().unwrap();
        let local = dir.path().join("local.kdbx");
        let state = dir.path().join("state.json");
        let remote_root = dir.path().join("remote");
        fs::write(&local, b"db-a").unwrap();

        let outcome = FilesystemRemote::new(remote_root.clone())
            .sync(&local, &state, "mac")
            .unwrap();

        assert_eq!(outcome.report.kind, SyncReportKind::Published);
        assert_eq!(
            Revision::from_file(remote_root.join("canonical/passwords.kdbx")).unwrap(),
            Revision::from_bytes(b"db-a")
        );
        assert_eq!(
            LocalState::read_or_new(&state, "mac")
                .unwrap()
                .base_revision,
            Some(Revision::from_bytes(b"db-a"))
        );
    }

    #[test]
    fn pulls_remote_when_local_did_not_change() {
        let dir = tempdir().unwrap();
        let local = dir.path().join("local.kdbx");
        let state = dir.path().join("state.json");
        let remote_root = dir.path().join("remote");
        let remote = FilesystemRemote::new(&remote_root);

        fs::write(&local, b"db-a").unwrap();
        remote.sync(&local, &state, "mac").unwrap();
        let other_db = dir.path().join("other.kdbx");
        let other_state = dir.path().join("other.json");
        fs::copy(&local, &other_db).unwrap();
        remote.sync(&other_db, &other_state, "android").unwrap();
        fs::write(&other_db, b"db-b").unwrap();
        remote.sync(&other_db, &other_state, "android").unwrap();

        let outcome = remote.sync(&local, &state, "mac").unwrap();

        assert_eq!(outcome.report.kind, SyncReportKind::Pulled);
        assert_eq!(fs::read(&local).unwrap(), b"db-b");
    }

    #[test]
    fn preserves_incoming_when_local_and_remote_changed() {
        let dir = tempdir().unwrap();
        let local = dir.path().join("local.kdbx");
        let state = dir.path().join("state.json");
        let remote_root = dir.path().join("remote");
        let remote = FilesystemRemote::new(&remote_root);

        fs::write(&local, b"base").unwrap();
        remote.sync(&local, &state, "mac").unwrap();

        let android_db = dir.path().join("android.kdbx");
        let android_state = dir.path().join("android.json");
        fs::copy(&local, &android_db).unwrap();
        remote.sync(&android_db, &android_state, "android").unwrap();

        fs::write(&local, b"mac-change").unwrap();
        fs::write(&android_db, b"android-change").unwrap();
        remote.sync(&android_db, &android_state, "android").unwrap();

        let outcome = remote.sync(&local, &state, "mac").unwrap();

        assert_eq!(outcome.report.kind, SyncReportKind::IncomingPreserved);
        assert_eq!(remote.incoming_databases().unwrap().len(), 1);
        assert_eq!(
            Revision::from_file(remote_root.join("canonical/passwords.kdbx")).unwrap(),
            Revision::from_bytes(b"android-change")
        );
    }

    #[test]
    fn publishes_bytes_when_base_matches() {
        let dir = tempdir().unwrap();
        let remote = FilesystemRemote::new(dir.path().join("remote"));
        let revision = Revision::from_bytes(b"db-a");

        let manifest = remote
            .publish_bytes_if_base_matches(b"db-a", None, &revision, "android")
            .unwrap();

        assert_eq!(manifest.revision, revision);
        assert_eq!(remote.current_revision().unwrap(), Some(revision));
        assert_eq!(remote.canonical_bytes().unwrap(), b"db-a");
    }

    #[test]
    fn rejects_byte_publish_when_base_is_stale() {
        let dir = tempdir().unwrap();
        let remote = FilesystemRemote::new(dir.path().join("remote"));
        let first = Revision::from_bytes(b"db-a");
        let second = Revision::from_bytes(b"db-b");
        let stale = Revision::from_bytes(b"stale");

        remote
            .publish_bytes_if_base_matches(b"db-a", None, &first, "mac")
            .unwrap();
        let error = remote
            .publish_bytes_if_base_matches(b"db-b", Some(&stale), &second, "android")
            .unwrap_err();

        assert!(matches!(
            error,
            super::RemoteError::BaseRevisionMismatch { .. }
        ));
        assert_eq!(remote.canonical_bytes().unwrap(), b"db-a");
    }

    #[test]
    fn preserves_incoming_bytes() {
        let dir = tempdir().unwrap();
        let remote = FilesystemRemote::new(dir.path().join("remote"));
        let revision = Revision::from_bytes(b"android-change");

        let path = remote
            .preserve_incoming_bytes(b"android-change", &revision, "Pixel 8")
            .unwrap();

        assert_eq!(fs::read(path).unwrap(), b"android-change");
        assert_eq!(remote.incoming_databases().unwrap().len(), 1);
    }
}
