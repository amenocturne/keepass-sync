use keepass_sync::{Manifest, Revision, SyncInputs, decide_sync};
use std::env;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

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
        "manifest" => manifest_command(&args[1..]),
        "doctor" => doctor_command(),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        unknown => Err(format!("unknown command: {unknown}")),
    }
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
    match Command::new("keepassxc-cli").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            println!("keepassxc-cli: {}", version.trim());
            Ok(())
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("keepassxc-cli failed: {}", stderr.trim()))
        }
        Err(error) => Err(format!("keepassxc-cli not available: {error}")),
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

fn print_help() {
    println!(
        "keepass-sync\n\nCommands:\n  hash <path>\n  decide --local REV [--base REV] [--remote REV]\n  manifest read <path>\n  doctor"
    );
}
