//! The `oxc-tsrx` linter. Selected by default, or by the `lint` subcommand.

use std::{env, path::PathBuf, process::ExitCode};

use oxc_adapter::OXC_REVISION;
use tsrx_lint::{ConfigRuleFilter, ConfigRuleSeverity, LintSession};

const HELP: &str = "\
OXC for TSRX linter

Usage: oxc-tsrx-lint [--fix] [--format=json] [-D RULE] PATH...

Options:
    --fix                   Apply the safe fixes and write the changed files
    -A, --allow RULE        Report RULE at the allow severity
    -W, --warn RULE         Report RULE at the warn severity
    -D, --deny RULE         Report RULE at the deny severity
    -c, --config PATH       Use an explicit JSON/JSONC Oxlint configuration
    --type-aware            Enable the type-aware rules
    --type-check            Enable the type-aware rules and type checking
    --format json           Output format; json is the only value, and the default
    -h, --help              Show this help
    -V, --version           Show the package and canonical OXC revision

This is the internal capability target the oxc.provider metadata names for
linting .tsrx files. It takes explicit file paths only, never a directory or a
glob, and always prints one JSON report to stdout. Run `oxlint` instead for the
drop-in command a project installs: it discovers files, honours ignore files,
prints human-readable diagnostics, and covers .tsrx and ordinary files in one
run.
";

pub fn run_cli(arguments: Vec<String>) -> ExitCode {
    match run(arguments.into_iter()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("oxc-tsrx: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(arguments: impl Iterator<Item = String>) -> Result<u8, String> {
    let arguments = arguments.collect::<Vec<_>>();
    if arguments.iter().any(|argument| matches!(argument.as_str(), "-h" | "--help")) {
        print!("{HELP}");
        return Ok(0);
    }
    if arguments.iter().any(|argument| matches!(argument.as_str(), "-V" | "--version")) {
        println!("oxc-tsrx {} (OXC {OXC_REVISION})", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }
    let ParsedArguments { filters, files, fix, config_path, config_base, type_aware, type_check } =
        parse_arguments(arguments.into_iter())?;

    let cwd =
        env::current_dir().map_err(|error| format!("unable to read current directory: {error}"))?;
    let session = if type_aware {
        LintSession::new_type_aware_with_config_base(
            &cwd,
            config_path.as_deref(),
            config_base.as_deref(),
            &filters,
            fix,
            type_check,
        )?
    } else {
        LintSession::new_with_config_base(
            &cwd,
            config_path.as_deref(),
            config_base.as_deref(),
            &filters,
            fix,
        )?
    };
    let files = files
        .into_iter()
        .map(|path| if path.is_absolute() { path } else { cwd.join(path) })
        .filter(|path| !session.should_ignore(path))
        .collect::<Vec<_>>();
    let output = session.aggregate(session.lint_files(&files)?);
    let errors =
        output.diagnostics.iter().filter(|diagnostic| diagnostic.severity == "error").count();
    let warnings =
        output.diagnostics.iter().filter(|diagnostic| diagnostic.severity == "warning").count();
    println!(
        "{}",
        serde_json::to_string(&output).map_err(|error| format!("JSON output failed: {error}"))?
    );
    let warnings_fail = session.deny_warnings() && warnings > 0;
    let max_warnings_fail = session.max_warnings().is_some_and(|maximum| warnings > maximum);
    Ok(u8::from(errors > 0 || warnings_fail || max_warnings_fail))
}

struct ParsedArguments {
    filters: Vec<ConfigRuleFilter>,
    files: Vec<PathBuf>,
    fix: bool,
    config_path: Option<PathBuf>,
    config_base: Option<PathBuf>,
    type_aware: bool,
    type_check: bool,
}

fn parse_arguments(arguments: impl Iterator<Item = String>) -> Result<ParsedArguments, String> {
    let mut arguments = arguments.peekable();
    let mut filters = Vec::new();
    let mut files = Vec::new();
    let mut fix = false;
    let mut config_path = None;
    let mut config_base = None;
    let mut type_aware = false;
    let mut type_check = false;

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--format=json" => {}
            "--format" => {
                let format = arguments.next().ok_or("--format requires a value")?;
                if format != "json" {
                    return Err("the current native CLI supports --format=json only".to_string());
                }
            }
            "--allow" | "-A" => filters.push(ConfigRuleFilter {
                severity: ConfigRuleSeverity::Allow,
                name: arguments.next().ok_or("--allow requires a rule")?,
            }),
            "--warn" | "-W" => filters.push(ConfigRuleFilter {
                severity: ConfigRuleSeverity::Warn,
                name: arguments.next().ok_or("--warn requires a rule")?,
            }),
            "--deny" | "-D" => filters.push(ConfigRuleFilter {
                severity: ConfigRuleSeverity::Deny,
                name: arguments.next().ok_or("--deny requires a rule")?,
            }),
            "--config" | "-c" => {
                if config_path.is_some() {
                    return Err("--config may be specified only once".to_string());
                }
                config_path =
                    Some(PathBuf::from(arguments.next().ok_or("--config requires a path")?));
            }
            "--config-base" => {
                if config_base.is_some() {
                    return Err("--config-base may be specified only once".to_string());
                }
                config_base =
                    Some(PathBuf::from(arguments.next().ok_or("--config-base requires a path")?));
            }
            "--fix" => fix = true,
            "--type-aware" => type_aware = true,
            "--type-check" => {
                type_aware = true;
                type_check = true;
            }
            "-h" | "--help" | "-V" | "--version" => unreachable!("handled before parsing"),
            value if value.starts_with('-') => {
                return Err(format!("unsupported option in the current native CLI: {value}"));
            }
            value => files.push(PathBuf::from(value)),
        }
    }

    if files.is_empty() {
        return Err("at least one explicit source file is required".to_string());
    }

    Ok(ParsedArguments { filters, files, fix, config_path, config_base, type_aware, type_check })
}
