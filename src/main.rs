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

#[derive(Debug, Clone)]
struct SyncOptions {
    local: String,
    remote_root: String,
    state: String,
    device: String,
}

impl SyncOptions {
    fn from_args(args: &[String]) -> Result<Self, String> {
        Ok(Self {
            local: required_value(args, "--local")?,
            remote_root: required_value(args, "--remote-root")?,
            state: required_value(args, "--state")?,
            device: required_value(args, "--device")?,
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
    let options = SyncOptions::from_args(args)?;
    let interval_seconds = optional_value(args, "--interval-seconds")
        .map(|value| value.parse::<u64>())
        .transpose()
        .map_err(|error| format!("invalid --interval-seconds: {error}"))?
        .unwrap_or(30);
    let interval = Duration::from_secs(interval_seconds);
    let local = PathBuf::from(&options.local);
    let mut last_seen = std::fs::metadata(&local)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| format!("failed to stat local database: {error}"))?;

    run_sync(&options)?;

    loop {
        thread::sleep(interval);
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
    let remote_root = required_value(args, "--remote-root")?;
    let bind = optional_value(args, "--bind").unwrap_or_else(|| "127.0.0.1:8787".to_string());
    let token = read_token(args)?;

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
    let remote_root = required_value(args, "--remote-root")?;
    let endpoint = required_value(args, "--endpoint")?;
    let token = read_token(args)?;
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

    let mirrored = FilesystemRemote::new(remote_root)
        .mirror_incoming_files(&files)
        .map_err(|error| error.to_string())?;

    println!("pulled: {}", mirrored.len());
    for path in mirrored {
        println!("incoming: {}", path.display());
    }
    Ok(())
}

fn merge_incoming_command(args: &[String]) -> Result<(), String> {
    let remote_root = required_value(args, "--remote-root")?;
    let device = required_value(args, "--device")?;
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

fn read_token(args: &[String]) -> Result<String, String> {
    if let Some(token) = optional_value(args, "--token") {
        return Ok(token);
    }

    if let Some(path) = optional_value(args, "--token-file") {
        return std::fs::read_to_string(path)
            .map(|token| token.trim_end_matches(['\r', '\n']).to_string())
            .map_err(|error| format!("failed to read token file: {error}"));
    }

    env::var("KEEPASS_SYNC_TOKEN")
        .map(|token| token.trim_end_matches(['\r', '\n']).to_string())
        .map_err(|_| "missing token; pass --token, --token-file, or KEEPASS_SYNC_TOKEN".to_string())
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

fn print_help() {
    println!(
        "keepass-sync\n\nCommands:\n  hash <path>\n  decide --local REV [--base REV] [--remote REV]\n  sync --local DB --remote-root DIR --state STATE --device ID\n  watch --local DB --remote-root DIR --state STATE --device ID [--interval-seconds N]\n  serve --remote-root DIR [--bind HOST:PORT] [--token TOKEN | --token-file FILE]\n  pull-incoming --remote-root DIR --endpoint URL [--token TOKEN | --token-file FILE]\n  merge-incoming --remote-root DIR --device ID [--password-file FILE]\n  manifest read <path>\n  doctor"
    );
}
