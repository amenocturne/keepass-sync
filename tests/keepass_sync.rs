use keepass_sync::{FilesystemRemote, Keepassxc, LocalState, Revision, SyncReportKind};
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::tempdir;

const PASSWORD: &str = "test-password";

#[test]
fn syncs_real_kdbx_changes_between_two_devices() -> Result<(), Box<dyn Error>> {
    if !keepass_available() {
        return Ok(());
    }

    let dir = tempdir()?;
    let remote = FilesystemRemote::new(dir.path().join("remote"));
    let mac_db = dir.path().join("mac.kdbx");
    let mac_state = dir.path().join("mac-state.json");
    let android_db = dir.path().join("android.kdbx");
    let android_state = dir.path().join("android-state.json");

    create_db(&mac_db)?;
    add_entry(&mac_db, "initial")?;
    remote.sync(&mac_db, &mac_state, "mac")?;

    fs::copy(&mac_db, &android_db)?;
    let android_adopt = remote.sync(&android_db, &android_state, "android")?;
    assert_eq!(android_adopt.report.kind, SyncReportKind::Adopted);

    add_entry(&android_db, "android-entry")?;
    let android_publish = remote.sync(&android_db, &android_state, "android")?;
    assert_eq!(android_publish.report.kind, SyncReportKind::Published);

    let mac_pull = remote.sync(&mac_db, &mac_state, "mac")?;
    assert_eq!(mac_pull.report.kind, SyncReportKind::Pulled);
    assert!(list_entries(&mac_db)?.contains(&"android-entry".to_string()));

    Ok(())
}

#[test]
fn preserves_divergent_real_kdbx_for_desktop_merge() -> Result<(), Box<dyn Error>> {
    if !keepass_available() {
        return Ok(());
    }

    let dir = tempdir()?;
    let remote = FilesystemRemote::new(dir.path().join("remote"));
    let mac_db = dir.path().join("mac.kdbx");
    let mac_state = dir.path().join("mac-state.json");
    let android_db = dir.path().join("android.kdbx");
    let android_state = dir.path().join("android-state.json");

    create_db(&mac_db)?;
    add_entry(&mac_db, "base-entry")?;
    remote.sync(&mac_db, &mac_state, "mac")?;
    fs::copy(&mac_db, &android_db)?;
    remote.sync(&android_db, &android_state, "android")?;

    add_entry(&mac_db, "mac-entry")?;
    add_entry(&android_db, "android-entry")?;

    let android_publish = remote.sync(&android_db, &android_state, "android")?;
    assert_eq!(android_publish.report.kind, SyncReportKind::Published);

    let mac_preserve = remote.sync(&mac_db, &mac_state, "mac")?;
    assert_eq!(mac_preserve.report.kind, SyncReportKind::IncomingPreserved);
    assert_eq!(remote.incoming_databases()?.len(), 1);

    let password_file = dir.path().join("password.txt");
    fs::write(&password_file, PASSWORD)?;
    let archived = remote.merge_incoming(
        "mac",
        &Keepassxc::default(),
        Some(fs::read_to_string(password_file)?),
    )?;

    assert_eq!(archived.len(), 1);
    assert!(remote.incoming_databases()?.is_empty());

    let canonical = dir.path().join("remote/canonical/passwords.kdbx");
    let entries = list_entries(canonical)?;
    assert!(entries.contains(&"mac-entry".to_string()));
    assert!(entries.contains(&"android-entry".to_string()));

    Ok(())
}

#[test]
fn records_base_revision_after_real_kdbx_publish() -> Result<(), Box<dyn Error>> {
    if !keepass_available() {
        return Ok(());
    }

    let dir = tempdir()?;
    let remote = FilesystemRemote::new(dir.path().join("remote"));
    let db = dir.path().join("db.kdbx");
    let state_path = dir.path().join("state.json");

    create_db(&db)?;
    add_entry(&db, "entry")?;
    remote.sync(&db, &state_path, "mac")?;

    let state = LocalState::read_or_new(state_path, "mac")?;
    assert_eq!(state.base_revision, Some(Revision::from_file(db)?));

    Ok(())
}

#[test]
fn cli_sync_publishes_real_kdbx() -> Result<(), Box<dyn Error>> {
    if !keepass_available() {
        return Ok(());
    }

    let dir = tempdir()?;
    let db = dir.path().join("db.kdbx");
    let state = dir.path().join("state.json");
    let remote = dir.path().join("remote");
    create_db(&db)?;
    add_entry(&db, "cli-entry")?;

    let output = Command::new(env!("CARGO_BIN_EXE_keepass-sync"))
        .arg("sync")
        .arg("--local")
        .arg(&db)
        .arg("--remote-root")
        .arg(&remote)
        .arg("--state")
        .arg(&state)
        .arg("--device")
        .arg("mac")
        .output()?;

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("action: initialize-remote"));
    assert!(remote.join("canonical/passwords.kdbx").exists());

    Ok(())
}

fn keepass_available() -> bool {
    let available = Keepassxc::default().is_available();
    if !available {
        eprintln!("skipping integration test: keepassxc-cli is not available");
    }
    available
}

fn create_db(path: &Path) -> Result<(), Box<dyn Error>> {
    run_keepass(
        Command::new("keepassxc-cli")
            .arg("db-create")
            .arg("-q")
            .arg("--set-password")
            .arg(path),
        &format!("{PASSWORD}\n{PASSWORD}\n"),
    )
}

fn add_entry(path: &Path, entry: &str) -> Result<(), Box<dyn Error>> {
    run_keepass(
        Command::new("keepassxc-cli")
            .arg("add")
            .arg("-q")
            .arg("--username")
            .arg("user")
            .arg("--password-prompt")
            .arg(path)
            .arg(entry),
        &format!("{PASSWORD}\nentry-password-{entry}\n"),
    )
}

fn list_entries(path: impl AsRef<Path>) -> Result<Vec<String>, Box<dyn Error>> {
    let output = keepass_output(
        Command::new("keepassxc-cli")
            .arg("ls")
            .arg("-q")
            .arg(path.as_ref()),
        &format!("{PASSWORD}\n"),
    )?;
    Ok(output.lines().map(str::to_string).collect())
}

fn run_keepass(command: &mut Command, stdin: &str) -> Result<(), Box<dyn Error>> {
    let output = keepass_output(command, stdin)?;
    drop(output);
    Ok(())
}

fn keepass_output(command: &mut Command, stdin: &str) -> Result<String, Box<dyn Error>> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(stdin.as_bytes())?;

    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!(
            "keepassxc-cli failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.starts_with("Enter password"))
        .collect::<Vec<_>>()
        .join("\n"))
}
