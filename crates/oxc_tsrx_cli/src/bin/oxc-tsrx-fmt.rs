use std::{
    collections::HashSet,
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use rayon::prelude::*;
use tsrx_format::{FormatOutput, FormatSession};

const HELP: &str = "\
OXC for TSRX formatter

Usage: oxc-tsrx-fmt [--write | --check] [--threads=INT] PATH...
       oxc-tsrx-fmt [--config=PATH] --stdin-filepath=PATH

Mode options:
    --write                 Format and write explicit files (default for files)
    --check                 Exit 1 and list files that differ; never write
    --stdin-filepath=PATH   Read stdin, infer the source type, and print formatted source
    -c, --config=PATH       Use an explicit JSON/JSONC Oxfmt configuration
    --threads=INT           Worker count for explicit multi-file formatting
    -h, --help              Show this help
    -V, --version           Show the package and canonical OXC revision

The current TSRX grammar slice supports @{, if/else, for/empty, switch/case/default,
try/pending/catch controls, dynamic JSX tags, and lowercase raw <style> elements.
CSS payload bytes are preserved rather than CSS-formatted. Unsupported or malformed
custom forms fail before any requested file is changed.
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileMode {
    Write,
    Check,
}

#[derive(Debug)]
struct Args {
    file_mode: FileMode,
    stdin_filepath: Option<PathBuf>,
    files: Vec<PathBuf>,
    threads: Option<usize>,
    config_path: Option<PathBuf>,
    config_base: Option<PathBuf>,
}

#[derive(Debug)]
struct FormattedFile {
    path: PathBuf,
    output: FormatOutput,
}

#[derive(Debug)]
struct StagedFile {
    path: PathBuf,
    backup: PathBuf,
    temporary: PathBuf,
}

fn main() -> ExitCode {
    match run(env::args().skip(1)) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("oxc-tsrx-fmt: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(arguments: impl Iterator<Item = String>) -> Result<u8, String> {
    let arguments = arguments.collect::<Vec<_>>();
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "-h" | "--help"))
    {
        print!("{HELP}");
        return Ok(0);
    }
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "-V" | "--version"))
    {
        println!(
            "oxc-tsrx-fmt {} (OXC {})",
            env!("CARGO_PKG_VERSION"),
            tsrx_format::OXC_REVISION
        );
        return Ok(0);
    }
    let args = parse_args(arguments.into_iter())?;
    let cwd =
        env::current_dir().map_err(|error| format!("unable to read current directory: {error}"))?;
    let session = FormatSession::new_with_config_base(
        &cwd,
        args.config_path.as_deref(),
        args.config_base.as_deref(),
    )?;
    if let Some(path) = args.stdin_filepath {
        return run_stdin(&session, &path);
    }

    let formatted = format_files(&session, args.files, args.threads)?;
    match args.file_mode {
        FileMode::Check => {
            let mut different = false;
            for file in &formatted {
                if file.output.changed {
                    println!("{}", file.path.display());
                    different = true;
                }
            }
            Ok(u8::from(different))
        }
        FileMode::Write => {
            commit_all(&formatted)?;
            Ok(0)
        }
    }
}

#[allow(clippy::too_many_lines)]
fn parse_args(arguments: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut arguments = arguments.peekable();
    let mut file_mode = None;
    let mut stdin_filepath = None;
    let mut files = Vec::new();
    let mut threads = None;
    let mut config_path = None;
    let mut config_base = None;

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--write" => set_mode(&mut file_mode, FileMode::Write)?,
            "--check" | "--list-different" => set_mode(&mut file_mode, FileMode::Check)?,
            "--stdin-filepath" => {
                let value = arguments.next().ok_or("--stdin-filepath requires a path")?;
                set_once_path(&mut stdin_filepath, value, "--stdin-filepath")?;
            }
            "--threads" => {
                let value = arguments.next().ok_or("--threads requires a value")?;
                set_threads(&mut threads, &value)?;
            }
            "--config" | "-c" => {
                let value = arguments.next().ok_or("--config requires a path")?;
                set_once_path(&mut config_path, value, "--config")?;
            }
            "--config-base" => {
                let value = arguments.next().ok_or("--config-base requires a path")?;
                set_once_path(&mut config_base, value, "--config-base")?;
            }
            "-h" | "--help" | "-V" | "--version" => unreachable!("handled before parsing"),
            value if value.starts_with("--stdin-filepath=") => {
                let path = value.trim_start_matches("--stdin-filepath=");
                if path.is_empty() {
                    return Err("--stdin-filepath requires a path".to_string());
                }
                set_once_path(&mut stdin_filepath, path.to_string(), "--stdin-filepath")?;
            }
            value if value.starts_with("--threads=") => {
                set_threads(&mut threads, value.trim_start_matches("--threads="))?;
            }
            value if value.starts_with("--config=") => {
                let path = value.trim_start_matches("--config=");
                if path.is_empty() {
                    return Err("--config requires a path".to_string());
                }
                set_once_path(&mut config_path, path.to_string(), "--config")?;
            }
            value if value.starts_with("--config-base=") => {
                let path = value.trim_start_matches("--config-base=");
                if path.is_empty() {
                    return Err("--config-base requires a path".to_string());
                }
                set_once_path(&mut config_base, path.to_string(), "--config-base")?;
            }
            value if value.starts_with('-') => {
                return Err(format!("unsupported option: {value}"));
            }
            value => files.push(PathBuf::from(value)),
        }
    }

    if stdin_filepath.is_some() {
        if !files.is_empty() {
            return Err("stdin mode cannot be combined with file paths".to_string());
        }
        if file_mode.is_some() {
            return Err("stdin mode cannot be combined with --write or --check".to_string());
        }
    } else if files.is_empty() {
        return Err("at least one explicit source file is required".to_string());
    }

    Ok(Args {
        file_mode: file_mode.unwrap_or(FileMode::Write),
        stdin_filepath,
        files,
        threads,
        config_path,
        config_base,
    })
}

fn set_mode(mode: &mut Option<FileMode>, value: FileMode) -> Result<(), String> {
    if mode.is_some_and(|current| current != value) {
        return Err("--write and --check are mutually exclusive".to_string());
    }
    *mode = Some(value);
    Ok(())
}

fn set_once_path(target: &mut Option<PathBuf>, value: String, option: &str) -> Result<(), String> {
    if target.is_some() {
        return Err(format!("{option} may be specified only once"));
    }
    *target = Some(PathBuf::from(value));
    Ok(())
}

fn set_threads(target: &mut Option<usize>, value: &str) -> Result<(), String> {
    if target.is_some() {
        return Err("--threads may be specified only once".to_string());
    }
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("invalid --threads value: {value}"))?;
    if parsed == 0 {
        return Err("--threads must be at least 1".to_string());
    }
    *target = Some(parsed);
    Ok(())
}

fn run_stdin(session: &FormatSession, path: &Path) -> Result<u8, String> {
    let mut source = String::new();
    io::stdin()
        .read_to_string(&mut source)
        .map_err(|error| format!("unable to read stdin: {error}"))?;
    let output = session.format_text(path, &source)?;
    io::stdout()
        .write_all(output.code.as_bytes())
        .and_then(|()| io::stdout().flush())
        .map_err(|error| format!("unable to write stdout: {error}"))?;
    Ok(0)
}

fn validate_unique_paths(paths: &[PathBuf]) -> Result<(), String> {
    let mut seen = HashSet::with_capacity(paths.len());
    for path in paths {
        if !seen.insert(path) {
            return Err(format!("duplicate source path: {}", path.display()));
        }
    }
    Ok(())
}

fn format_files(
    session: &FormatSession,
    mut paths: Vec<PathBuf>,
    threads: Option<usize>,
) -> Result<Vec<FormattedFile>, String> {
    paths.retain(|path| !session.should_ignore(path));
    validate_unique_paths(&paths)?;
    let operation = || {
        paths
            .into_par_iter()
            .map(|path| {
                let source = fs::read_to_string(&path)
                    .map_err(|error| format!("unable to read {}: {error}", path.display()))?;
                let output = session
                    .format_text(&path, &source)
                    .map_err(|error| format!("{}: {error}", path.display()))?;
                Ok(FormattedFile { path, output })
            })
            .collect::<Result<Vec<_>, String>>()
    };

    if let Some(threads) = threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|error| format!("unable to build formatter thread pool: {error}"))?
            .install(operation)
    } else {
        operation()
    }
}

/// Stages every changed output next to its source, then swaps all originals through backups.
/// Any recoverable staging or rename error restores the original bytes before returning.
fn commit_all(files: &[FormattedFile]) -> Result<(), String> {
    let changed = files
        .iter()
        .filter(|file| file.output.changed)
        .collect::<Vec<_>>();
    if changed.is_empty() {
        return Ok(());
    }

    let mut staged = Vec::with_capacity(changed.len());
    for (index, file) in changed.iter().enumerate() {
        if fs::symlink_metadata(&file.path)
            .map_err(|error| format!("unable to inspect {}: {error}", file.path.display()))?
            .file_type()
            .is_symlink()
        {
            cleanup_staged(&staged);
            return Err(format!(
                "refusing to replace symbolic link {}",
                file.path.display()
            ));
        }
        match stage_file(file, index) {
            Ok(value) => staged.push(value),
            Err(error) => {
                cleanup_staged(&staged);
                return Err(error);
            }
        }
    }

    for (backed_up, item) in staged.iter().enumerate() {
        if let Err(error) = fs::rename(&item.path, &item.backup) {
            restore_backups(&staged[..backed_up]);
            cleanup_staged(&staged);
            return Err(format!(
                "unable to stage original {}: {error}",
                item.path.display()
            ));
        }
    }

    for (installed, item) in staged.iter().enumerate() {
        if let Err(error) = fs::rename(&item.temporary, &item.path) {
            for installed_item in &staged[..installed] {
                let _ = fs::remove_file(&installed_item.path);
            }
            restore_backups(&staged);
            cleanup_staged(&staged);
            return Err(format!(
                "unable to install formatted {}: {error}",
                item.path.display()
            ));
        }
    }

    for item in &staged {
        if let Err(error) = fs::remove_file(&item.backup) {
            eprintln!(
                "oxc-tsrx-fmt: warning: unable to remove backup {}: {error}",
                item.backup.display()
            );
        }
    }
    Ok(())
}

fn stage_file(file: &FormattedFile, index: usize) -> Result<StagedFile, String> {
    let parent = file.path.parent().unwrap_or_else(|| Path::new("."));
    let name = file
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            format!(
                "source path has no UTF-8 file name: {}",
                file.path.display()
            )
        })?;
    let identity = format!("{}-{index}", std::process::id());
    let temporary = parent.join(format!(".{name}.oxc-tsrx-{identity}.tmp"));
    let backup = parent.join(format!(".{name}.oxc-tsrx-{identity}.bak"));
    if temporary.exists() || backup.exists() {
        return Err(format!(
            "formatter transaction path already exists beside {}",
            file.path.display()
        ));
    }

    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("unable to stage {}: {error}", file.path.display()))?;
    let result = output
        .write_all(file.output.code.as_bytes())
        .and_then(|()| output.flush())
        .and_then(|()| {
            let permissions = fs::metadata(&file.path)?.permissions();
            fs::set_permissions(&temporary, permissions)
        });
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(format!("unable to stage {}: {error}", file.path.display()));
    }

    Ok(StagedFile {
        path: file.path.clone(),
        backup,
        temporary,
    })
}

fn restore_backups(items: &[StagedFile]) {
    for item in items.iter().rev() {
        let _ = fs::rename(&item.backup, &item.path);
    }
}

fn cleanup_staged(items: &[StagedFile]) {
    for item in items {
        let _ = fs::remove_file(&item.temporary);
        if item.backup.exists() && !item.path.exists() {
            let _ = fs::rename(&item.backup, &item.path);
        }
    }
}
