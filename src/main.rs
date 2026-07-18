use keepass_sync::{
    FilesystemRemote, HttpServerConfig, IncomingFile, Keepassxc, Manifest, Revision, SyncInputs,
    decide_sync, serve,
};
use serde::Deserialize;
use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::Duration;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let Some(command) = args.first().map(String::as_str) else {
        print_help();
        return Ok(());
    };

    match command {
        "hash" => hash_command(&args[1..]),
        "decide" => decide_command(&args[1..]),
        "sync" => sync_command(&args[1..]),
        "watch" => watch_command(&args[1..]),
        "serve" => serve_command(&args[1..]),
        "pull-incoming" => pull_incoming_command(&args[1..]),
        "merge-incoming" => merge_incoming_command(&args[1..]),
        "manifest" => manifest_command(&args[1..]),
        "doctor" => doctor_command(),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        unknown => Err(format!("unknown command: {unknown}")),
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct Config {
    local: Option<String>,
    remote_root: Option<String>,
    state: Option<String>,
    conflict_dir: Option<String>,
    device: Option<String>,
    endpoint: Option<String>,
    token_file: Option<String>,
    bind: Option<String>,
    watch_interval: Option<String>,
    pull_interval: Option<String>,
}

impl Config {
    fn from_args(args: &[String]) -> Result<Self, String> {
        let Some(path) = optional_value(args, "--config").or_else(default_config_path) else {
            return Ok(Self::default());
        };
        let contents = std::fs::read_to_string(&path)
            .map_err(|error| format!("failed to read config {path}: {error}"))?;
        serde_json::from_str(&contents)
            .map_err(|error| format!("failed to parse config {path}: {error}"))
    }

    fn value(&self, args: &[String], flag: &str, field: Option<&String>) -> Option<String> {
        optional_value(args, flag).or_else(|| field.cloned())
    }

    fn required_value(
        &self,
        args: &[String],
        flag: &str,
        field: Option<&String>,
    ) -> Result<String, String> {
        self.value(args, flag, field)
            .ok_or_else(|| format!("missing required flag or config field: {flag}"))
    }
}

#[derive(Debug, Clone)]
struct SyncOptions {
    local: String,
    remote_root: String,
    state: String,
    device: String,
}

impl SyncOptions {
    fn from_args(args: &[String]) -> Result<Self, String> {
        let config = Config::from_args(args)?;
        Ok(Self {
            local: config.required_value(args, "--local", config.local.as_ref())?,
            remote_root: config.required_value(
                args,
                "--remote-root",
                config.remote_root.as_ref(),
            )?,
            state: config.required_value(args, "--state", config.state.as_ref())?,
            device: config.required_value(args, "--device", config.device.as_ref())?,
        })
    }
}

fn sync_command(args: &[String]) -> Result<(), String> {
    let options = SyncOptions::from_args(args)?;
    run_sync(&options)
}

fn run_sync(options: &SyncOptions) -> Result<(), String> {
    let outcome = FilesystemRemote::new(&options.remote_root)
        .sync(&options.local, &options.state, &options.device)
        .map_err(|error| error.to_string())?;

    println!("action: {}", outcome.report.action.as_str());
    println!("status: {:?}", outcome.report.kind);
    println!("message: {}", outcome.report.message);
    if let Some(path) = outcome.report.path {
        println!("path: {}", path.display());
    }
    Ok(())
}

fn watch_command(args: &[String]) -> Result<(), String> {
    let config = Config::from_args(args)?;
    let options = SyncOptions::from_args(args)?;
    let interval_seconds = optional_value(args, "--interval-seconds")
        .or(config.watch_interval.clone())
        .map(parse_duration_seconds)
        .transpose()
        .map_err(|error| format!("invalid --interval-seconds: {error}"))?
        .unwrap_or(30);
    let interval = Duration::from_secs(interval_seconds);
    let pull_interval = optional_value(args, "--pull-interval")
        .or(config.pull_interval.clone())
        .map(parse_duration_seconds)
        .transpose()
        .map_err(|error| format!("invalid --pull-interval: {error}"))?
        .map(Duration::from_secs);
    let local = PathBuf::from(&options.local);
    let mut last_seen = std::fs::metadata(&local)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| format!("failed to stat local database: {error}"))?;
    let mut last_pull = std::time::Instant::now()
        .checked_sub(pull_interval.unwrap_or(Duration::ZERO))
        .unwrap_or_else(std::time::Instant::now);

    run_sync(&options)?;

    loop {
        thread::sleep(interval);
        if let Some(pull_interval) = pull_interval
            && last_pull.elapsed() >= pull_interval
        {
            last_pull = std::time::Instant::now();
            if let Err(error) = run_pull_incoming(args, &config) {
                eprintln!("watch: pull-incoming failed: {error}");
            }
        }
        let modified = match std::fs::metadata(&local).and_then(|metadata| metadata.modified()) {
            Ok(modified) => modified,
            Err(error) => {
                eprintln!("watch: failed to stat local database: {error}");
                continue;
            }
        };

        if modified > last_seen {
            last_seen = modified;
            if let Err(error) = run_sync(&options) {
                eprintln!("watch: sync failed: {error}");
            }
        }
    }
}

fn serve_command(args: &[String]) -> Result<(), String> {
    let config = Config::from_args(args)?;
    let remote_root = config.required_value(args, "--remote-root", config.remote_root.as_ref())?;
    let bind = config
        .value(args, "--bind", config.bind.as_ref())
        .unwrap_or_else(|| "127.0.0.1:8787".to_string());
    let token = read_token(args, &config)?;

    serve(HttpServerConfig {
        bind,
        remote_root: PathBuf::from(remote_root),
        token,
    })
    .map_err(|error| error.to_string())
}

#[derive(Debug, Deserialize)]
struct IncomingListResponse {
    incoming: Vec<IncomingListEntry>,
}

#[derive(Debug, Deserialize)]
struct IncomingListEntry {
    device_id: String,
    revision: Revision,
    size: u64,
}

fn pull_incoming_command(args: &[String]) -> Result<(), String> {
    let config = Config::from_args(args)?;
    run_pull_incoming(args, &config)
}

fn run_pull_incoming(args: &[String], config: &Config) -> Result<(), String> {
    let endpoint = config.required_value(args, "--endpoint", config.endpoint.as_ref())?;
    let token = read_token(args, config)?;
    let endpoint = endpoint.trim().trim_end_matches('/');
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        return Err("sync endpoint must start with http:// or https://".to_string());
    }

    let list_url = format!("{endpoint}/incoming");
    let response = http_get(&list_url, &token)?;
    let incoming: IncomingListResponse = serde_json::from_slice(&response)
        .map_err(|error| format!("failed to parse incoming list: {error}"))?;

    let mut files = Vec::new();
    for entry in incoming.incoming {
        let file_url = format!(
            "{endpoint}/incoming/{}/{}",
            url_encode(&entry.device_id),
            url_encode(entry.revision.as_str())
        );
        let bytes = http_get(&file_url, &token)?;
        let actual = Revision::from_bytes(&bytes);
        if actual != entry.revision {
            return Err(format!(
                "downloaded incoming revision mismatch for {}: expected {}, got {}",
                entry.device_id, entry.revision, actual
            ));
        }
        if bytes.len() as u64 != entry.size {
            return Err(format!(
                "downloaded incoming size mismatch for {} {}: expected {}, got {}",
                entry.device_id,
                entry.revision,
                entry.size,
                bytes.len()
            ));
        }
        files.push(IncomingFile {
            device_id: entry.device_id,
            revision: entry.revision,
            bytes,
        });
    }

    let conflict_dir = conflict_dir(args, config)?;
    let mirrored = write_conflict_files(&conflict_dir, &files)?;

    println!("pulled: {}", mirrored.len());
    for path in mirrored {
        println!("incoming: {}", path.display());
    }
    Ok(())
}

fn conflict_dir(args: &[String], config: &Config) -> Result<PathBuf, String> {
    if let Some(path) = config.value(args, "--conflict-dir", config.conflict_dir.as_ref()) {
        return Ok(PathBuf::from(path));
    }

    let local = config.required_value(args, "--local", config.local.as_ref())?;
    PathBuf::from(local)
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| "local database path has no parent directory".to_string())
}

fn write_conflict_files(
    conflict_dir: &std::path::Path,
    files: &[IncomingFile],
) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(conflict_dir)
        .map_err(|error| format!("failed to create conflict dir: {error}"))?;
    let mut written = Vec::new();

    for file in files {
        let path = conflict_dir.join(format!(
            "sync-conflict-{}.kdbx",
            safe_revision(&file.revision)
        ));
        let tmp_path = path.with_extension("kdbx.tmp");
        std::fs::write(&tmp_path, &file.bytes).map_err(|error| {
            format!(
                "failed to write conflict file {}: {error}",
                tmp_path.display()
            )
        })?;
        let actual = Revision::from_file(&tmp_path).map_err(|error| {
            format!(
                "failed to hash conflict file {}: {error}",
                tmp_path.display()
            )
        })?;
        if actual != file.revision {
            return Err(format!(
                "conflict file revision mismatch for {}: expected {}, got {}",
                file.device_id, file.revision, actual
            ));
        }
        std::fs::rename(&tmp_path, &path).map_err(|error| {
            format!(
                "failed to publish conflict file {}: {error}",
                path.display()
            )
        })?;
        written.push(path);
    }

    Ok(written)
}

fn merge_incoming_command(args: &[String]) -> Result<(), String> {
    let config = Config::from_args(args)?;
    let remote_root = config.required_value(args, "--remote-root", config.remote_root.as_ref())?;
    let device = config.required_value(args, "--device", config.device.as_ref())?;
    let password = optional_value(args, "--password-file")
        .map(std::fs::read_to_string)
        .transpose()
        .map_err(|error| format!("failed to read password file: {error}"))?
        .map(|password| password.trim_end_matches(['\r', '\n']).to_string());

    let archived = FilesystemRemote::new(remote_root)
        .merge_incoming(&device, &Keepassxc::default(), password)
        .map_err(|error| error.to_string())?;

    println!("merged: {}", archived.len());
    for path in archived {
        println!("archived: {}", path.display());
    }
    Ok(())
}

fn hash_command(args: &[String]) -> Result<(), String> {
    let [path] = args else {
        return Err("usage: keepass-sync hash <path>".to_string());
    };

    let revision =
        Revision::from_file(path).map_err(|error| format!("failed to hash file: {error}"))?;
    println!("{revision}");
    Ok(())
}

fn decide_command(args: &[String]) -> Result<(), String> {
    let local = required_value(args, "--local")?;
    let base = optional_value(args, "--base")
        .map(Revision::parse)
        .transpose()
        .map_err(|error| format!("invalid --base revision: {error}"))?;
    let remote = optional_value(args, "--remote")
        .map(Revision::parse)
        .transpose()
        .map_err(|error| format!("invalid --remote revision: {error}"))?;

    let inputs = SyncInputs {
        local_revision: Revision::parse(local)
            .map_err(|error| format!("invalid --local revision: {error}"))?,
        base_revision: base,
        remote_revision: remote,
    };

    let action = decide_sync(&inputs).map_err(|error| error.to_string())?;
    println!("{}", action.as_str());
    Ok(())
}

fn manifest_command(args: &[String]) -> Result<(), String> {
    let [subcommand, path] = args else {
        return Err("usage: keepass-sync manifest read <path>".to_string());
    };

    match subcommand.as_str() {
        "read" => {
            let manifest =
                Manifest::read(PathBuf::from(path)).map_err(|error| error.to_string())?;
            println!("revision: {}", manifest.revision);
            println!("updated_at: {}", manifest.updated_at);
            println!("updated_by: {}", manifest.updated_by);
            Ok(())
        }
        unknown => Err(format!("unknown manifest command: {unknown}")),
    }
}

fn doctor_command() -> Result<(), String> {
    let keepassxc = Keepassxc::default();
    match Command::new(keepassxc.binary()).arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            println!("keepassxc-cli path: {}", keepassxc.binary().display());
            println!("keepassxc-cli: {}", version.trim());
            Ok(())
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("keepassxc-cli failed: {}", stderr.trim()))
        }
        Err(error) => Err(format!(
            "keepassxc-cli not available at {}: {error}",
            keepassxc.binary().display()
        )),
    }
}

fn required_value(args: &[String], flag: &str) -> Result<String, String> {
    optional_value(args, flag).ok_or_else(|| format!("missing required flag: {flag}"))
}

fn optional_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}

fn read_token(args: &[String], config: &Config) -> Result<String, String> {
    if let Some(token) = optional_value(args, "--token") {
        return Ok(token);
    }

    if let Some(path) = optional_value(args, "--token-file").or_else(|| config.token_file.clone()) {
        return std::fs::read_to_string(path)
            .map(|token| token.trim_end_matches(['\r', '\n']).to_string())
            .map_err(|error| format!("failed to read token file: {error}"));
    }

    env::var("KEEPASS_SYNC_TOKEN")
        .map(|token| token.trim_end_matches(['\r', '\n']).to_string())
        .map_err(|_| "missing token; pass --token, --token-file, or KEEPASS_SYNC_TOKEN".to_string())
}

fn default_config_path() -> Option<String> {
    env::var("KEEPASS_SYNC_CONFIG").ok().or_else(|| {
        let path = env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| env::var("HOME").map(|home| PathBuf::from(home).join(".config")))
            .ok()?
            .join("keepass-sync/config.json");
        path.exists().then(|| path.display().to_string())
    })
}

fn parse_duration_seconds(value: String) -> Result<u64, String> {
    let trimmed = value.trim();
    let (number, multiplier) = if let Some(number) = trimmed.strip_suffix("ms") {
        let millis = number
            .parse::<u64>()
            .map_err(|error| format!("{trimmed}: {error}"))?;
        return Ok((millis / 1000).max(1));
    } else if let Some(number) = trimmed.strip_suffix('s') {
        (number, 1)
    } else if let Some(number) = trimmed.strip_suffix('m') {
        (number, 60)
    } else if let Some(number) = trimmed.strip_suffix('h') {
        (number, 60 * 60)
    } else {
        (trimmed, 1)
    };
    let amount = number
        .parse::<u64>()
        .map_err(|error| format!("{trimmed}: {error}"))?;
    Ok(amount.saturating_mul(multiplier))
}

fn http_get(url: &str, token: &str) -> Result<Vec<u8>, String> {
    let config = format!(
        "url = \"{}\"\nheader = \"Authorization: Bearer {}\"\nfail-with-body\nsilent\nshow-error\n",
        curl_config_escape(url),
        curl_config_escape(token)
    );
    let mut child = Command::new("curl")
        .arg("--config")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to run curl: {error}"))?;

    child
        .stdin
        .take()
        .expect("curl stdin is piped")
        .write_all(config.as_bytes())
        .map_err(|error| format!("failed to write curl config: {error}"))?;

    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed to read curl output: {error}"))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "HTTP request failed for {url}: {}{}",
            stderr.trim(),
            stdout.trim()
        ))
    }
}

fn curl_config_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn url_encode(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

fn safe_revision(revision: &Revision) -> String {
    revision.as_str().replace(':', "-")
}

fn print_help() {
    println!(
        "keepass-sync\n\nCommands:\n  hash <path>\n  decide --local REV [--base REV] [--remote REV]\n  sync [--config PATH] [--local DB --remote-root DIR --state STATE --device ID]\n  watch [--config PATH] [--local DB --remote-root DIR --state STATE --device ID] [--interval-seconds N] [--pull-interval N]\n  serve [--config PATH] [--remote-root DIR] [--bind HOST:PORT] [--token TOKEN | --token-file FILE]\n  pull-incoming [--config PATH] [--endpoint URL] [--local DB | --conflict-dir DIR] [--token TOKEN | --token-file FILE]\n  merge-incoming [--config PATH] [--remote-root DIR --device ID] [--password-file FILE]\n  manifest read <path>\n  doctor"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn conflict_dir_defaults_to_local_database_parent() {
        let config = Config {
            local: Some("/vault/passwords/Main.kdbx".to_string()),
            ..Config::default()
        };

        assert_eq!(
            conflict_dir(&[], &config).unwrap(),
            PathBuf::from("/vault/passwords")
        );
    }

    #[test]
    fn writes_conflict_files_beside_local_database_with_safe_revision() {
        let dir = tempdir().unwrap();
        let bytes = b"phone database bytes".to_vec();
        let revision = Revision::from_bytes(&bytes);

        let written = write_conflict_files(
            dir.path(),
            &[IncomingFile {
                device_id: "pixel-7".to_string(),
                revision: revision.clone(),
                bytes: bytes.clone(),
            }],
        )
        .unwrap();

        let expected = dir.path().join(format!(
            "sync-conflict-{}.kdbx",
            revision.as_str().replace(':', "-")
        ));
        assert_eq!(written, vec![expected.clone()]);
        assert_eq!(std::fs::read(expected).unwrap(), bytes);
    }
}
