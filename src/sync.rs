use crate::Revision;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncInputs {
    pub local_revision: Revision,
    pub base_revision: Option<Revision>,
    pub remote_revision: Option<Revision>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAction {
    InitializeRemote,
    AdoptRemote,
    Noop,
    PullRemote,
    PublishLocal,
    PreserveIncoming,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncExecutionReport {
    pub kind: SyncReportKind,
    pub action: SyncAction,
    pub message: String,
    pub path: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncReportKind {
    Adopted,
    IncomingPreserved,
    Noop,
    Published,
    Pulled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncProblem {
    RemoteMissingAfterInitialization,
}

pub fn decide_sync(inputs: &SyncInputs) -> Result<SyncAction, SyncProblem> {
    match (&inputs.base_revision, &inputs.remote_revision) {
        (None, None) => Ok(SyncAction::InitializeRemote),
        (None, Some(remote)) if *remote == inputs.local_revision => Ok(SyncAction::AdoptRemote),
        (None, Some(_)) => Ok(SyncAction::PreserveIncoming),
        (Some(_), None) => Err(SyncProblem::RemoteMissingAfterInitialization),
        (Some(base), Some(remote)) => {
            let local_changed = inputs.local_revision != *base;
            let remote_changed = remote != base;

            match (local_changed, remote_changed) {
                (false, false) => Ok(SyncAction::Noop),
                (false, true) => Ok(SyncAction::PullRemote),
                (true, false) => Ok(SyncAction::PublishLocal),
                (true, true) => Ok(SyncAction::PreserveIncoming),
            }
        }
    }
}

impl SyncAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InitializeRemote => "initialize-remote",
            Self::AdoptRemote => "adopt-remote",
            Self::Noop => "noop",
            Self::PullRemote => "pull-remote",
            Self::PublishLocal => "publish-local",
            Self::PreserveIncoming => "preserve-incoming",
        }
    }
}

impl SyncExecutionReport {
    pub fn adopted() -> Self {
        Self {
            kind: SyncReportKind::Adopted,
            action: SyncAction::AdoptRemote,
            message: "local state adopted existing remote canonical revision".to_string(),
            path: None,
        }
    }

    pub fn noop() -> Self {
        Self {
            kind: SyncReportKind::Noop,
            action: SyncAction::Noop,
            message: "local and remote are already synced".to_string(),
            path: None,
        }
    }

    pub fn pulled() -> Self {
        Self {
            kind: SyncReportKind::Pulled,
            action: SyncAction::PullRemote,
            message: "pulled remote canonical database into local file".to_string(),
            path: None,
        }
    }

    pub fn published(action: SyncAction) -> Self {
        Self {
            kind: SyncReportKind::Published,
            action,
            message: "published local database as remote canonical".to_string(),
            path: None,
        }
    }

    pub fn preserved_incoming(path: std::path::PathBuf) -> Self {
        Self {
            kind: SyncReportKind::IncomingPreserved,
            action: SyncAction::PreserveIncoming,
            message: "preserved divergent local database for desktop merge".to_string(),
            path: Some(path),
        }
    }
}

impl std::fmt::Display for SyncProblem {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RemoteMissingAfterInitialization => {
                formatter.write_str("remote canonical database is missing after initialization")
            }
        }
    }
}

impl std::error::Error for SyncProblem {}

#[cfg(test)]
mod tests {
    use super::{SyncAction, SyncInputs, decide_sync};
    use crate::Revision;

    fn rev(value: &[u8]) -> Revision {
        Revision::from_bytes(value)
    }

    #[test]
    fn initializes_remote_when_no_base_or_remote_exists() {
        let inputs = SyncInputs {
            local_revision: rev(b"local"),
            base_revision: None,
            remote_revision: None,
        };

        assert_eq!(decide_sync(&inputs).unwrap(), SyncAction::InitializeRemote);
    }

    #[test]
    fn adopts_remote_when_fresh_device_matches_canonical() {
        let revision = rev(b"same");
        let inputs = SyncInputs {
            local_revision: revision.clone(),
            base_revision: None,
            remote_revision: Some(revision),
        };

        assert_eq!(decide_sync(&inputs).unwrap(), SyncAction::AdoptRemote);
    }

    #[test]
    fn preserves_incoming_when_fresh_device_differs_from_canonical() {
        let inputs = SyncInputs {
            local_revision: rev(b"local"),
            base_revision: None,
            remote_revision: Some(rev(b"remote")),
        };

        assert_eq!(decide_sync(&inputs).unwrap(), SyncAction::PreserveIncoming);
    }

    #[test]
    fn does_nothing_when_local_and_remote_match_base() {
        let revision = rev(b"base");
        let inputs = SyncInputs {
            local_revision: revision.clone(),
            base_revision: Some(revision.clone()),
            remote_revision: Some(revision),
        };

        assert_eq!(decide_sync(&inputs).unwrap(), SyncAction::Noop);
    }

    #[test]
    fn pulls_when_only_remote_changed() {
        let base = rev(b"base");
        let inputs = SyncInputs {
            local_revision: base.clone(),
            base_revision: Some(base),
            remote_revision: Some(rev(b"remote")),
        };

        assert_eq!(decide_sync(&inputs).unwrap(), SyncAction::PullRemote);
    }

    #[test]
    fn publishes_when_only_local_changed() {
        let base = rev(b"base");
        let inputs = SyncInputs {
            local_revision: rev(b"local"),
            base_revision: Some(base.clone()),
            remote_revision: Some(base),
        };

        assert_eq!(decide_sync(&inputs).unwrap(), SyncAction::PublishLocal);
    }

    #[test]
    fn preserves_incoming_when_both_changed() {
        let inputs = SyncInputs {
            local_revision: rev(b"local"),
            base_revision: Some(rev(b"base")),
            remote_revision: Some(rev(b"remote")),
        };

        assert_eq!(decide_sync(&inputs).unwrap(), SyncAction::PreserveIncoming);
    }
}
