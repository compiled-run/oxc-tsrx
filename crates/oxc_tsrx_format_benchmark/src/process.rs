use std::{
    env, fs,
    hint::black_box,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

use memory_stats::memory_stats;
use tsrx_format::format_text;
use tsrx_syntax::{project, scan};

use crate::{budgets::Budgets, fixtures::build_corpus, stats::elapsed_ns};

#[derive(Debug)]
pub(crate) struct ProcessMeasurements {
    pub(crate) batch_bytes: usize,
    pub(crate) batch_ns: Vec<u64>,
    pub(crate) candidate_stdin_ns: Vec<u64>,
    pub(crate) stock_stdin_ns: Vec<u64>,
    pub(crate) tsrx_rss_bytes: Vec<u64>,
    pub(crate) tsx_rss_bytes: Vec<u64>,
    pub(crate) complete_output: bool,
}

pub(crate) fn run_process_measurements(
    temporary: &Path,
    tsrx: &str,
    tsx: &str,
    budgets: &Budgets,
) -> Result<ProcessMeasurements, String> {
    let batch_directory = temporary.join("batch");
    fs::create_dir(&batch_directory)
        .map_err(|error| format!("unable to create {}: {error}", batch_directory.display()))?;
    let per_file_target = 256 * 1024;
    let per_file = build_corpus(per_file_target);
    let file_count = budgets.batch_corpus_target_bytes.div_ceil(per_file.len()).max(2);
    let mut paths = Vec::with_capacity(file_count);
    for index in 0..file_count {
        let path = batch_directory.join(format!("batch-{index:04}.tsrx"));
        fs::write(&path, &per_file)
            .map_err(|error| format!("unable to write {}: {error}", path.display()))?;
        paths.push(path);
    }
    let batch_bytes = per_file.len() * paths.len();
    for _ in 0..budgets.batch_warmups {
        black_box(measure_batch(&budgets.candidate_binary, &paths)?);
    }
    let batch_ns = std::iter::repeat_with(|| measure_batch(&budgets.candidate_binary, &paths))
        .take(budgets.batch_samples)
        .collect::<Result<Vec<_>, _>>()?;

    let stdin_tsrx = build_corpus(10 * 1024);
    let stdin_overlay = scan(&stdin_tsrx).map_err(|error| error.to_string())?;
    let stdin_tsx = project(&stdin_tsrx, &stdin_overlay).map_err(|error| error.to_string())?;
    let mut candidate_stdin_ns = Vec::with_capacity(budgets.cold_process_samples);
    let mut stock_stdin_ns = Vec::with_capacity(budgets.cold_process_samples);
    let mut complete_output = true;
    for _ in 0..budgets.cold_process_samples {
        let candidate = measure_stdin_process(&budgets.candidate_binary, "cold.tsrx", &stdin_tsrx)?;
        let stock = measure_stdin_process(&budgets.stock_oxfmt_binary, "cold.tsx", &stdin_tsx)?;
        complete_output &= !candidate.1.is_empty() && !stock.1.is_empty();
        candidate_stdin_ns.push(candidate.0);
        stock_stdin_ns.push(stock.0);
    }

    let memory_tsrx = build_corpus(budgets.memory_corpus_target_bytes);
    let memory_overlay = scan(&memory_tsrx).map_err(|error| error.to_string())?;
    let memory_tsx = project(&memory_tsrx, &memory_overlay).map_err(|error| error.to_string())?;
    let memory_tsrx_path = temporary.join("memory.tsrx");
    let memory_tsx_path = temporary.join("memory.tsx");
    fs::write(&memory_tsrx_path, memory_tsrx)
        .map_err(|error| format!("unable to write {}: {error}", memory_tsrx_path.display()))?;
    fs::write(&memory_tsx_path, memory_tsx)
        .map_err(|error| format!("unable to write {}: {error}", memory_tsx_path.display()))?;
    let current = env::current_exe().map_err(|error| error.to_string())?;
    let mut tsrx_rss_bytes = Vec::with_capacity(budgets.rss_process_samples);
    let mut tsx_rss_bytes = Vec::with_capacity(budgets.rss_process_samples);
    for _ in 0..budgets.rss_process_samples {
        tsrx_rss_bytes.push(measure_memory_child(&current, &memory_tsrx_path)?);
        tsx_rss_bytes.push(measure_memory_child(&current, &memory_tsx_path)?);
    }

    // Keep the primary corpus arguments used so the function's comparison boundary stays clear.
    black_box((tsrx.len(), tsx.len()));
    Ok(ProcessMeasurements {
        batch_bytes,
        batch_ns,
        candidate_stdin_ns,
        stock_stdin_ns,
        tsrx_rss_bytes,
        tsx_rss_bytes,
        complete_output,
    })
}

pub(crate) fn measure_batch(binary: &Path, paths: &[PathBuf]) -> Result<u64, String> {
    let started = Instant::now();
    let output = Command::new(binary)
        .arg("--check")
        .args(paths)
        .output()
        .map_err(|error| format!("unable to run {}: {error}", binary.display()))?;
    let elapsed = elapsed_ns(started);
    if output.status.code() != Some(1) {
        return Err(format!(
            "batch formatter exited {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(elapsed)
}

pub(crate) fn measure_stdin_process(
    binary: &Path,
    filepath: &str,
    source: &str,
) -> Result<(u64, Vec<u8>), String> {
    let started = Instant::now();
    let mut child = Command::new(binary)
        .arg(format!("--stdin-filepath={filepath}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("unable to spawn {}: {error}", binary.display()))?;
    child
        .stdin
        .take()
        .ok_or("formatter child has no stdin")?
        .write_all(source.as_bytes())
        .map_err(|error| format!("unable to write formatter stdin: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("unable to wait for {}: {error}", binary.display()))?;
    let elapsed = elapsed_ns(started);
    if !output.status.success() {
        return Err(format!(
            "stdin formatter {} failed: {}",
            binary.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok((elapsed, output.stdout))
}

#[expect(
    clippy::print_stdout,
    reason = "the memory child process hands its measurement back to the parent on stdout"
)]
pub(crate) fn run_memory_child(arguments: &[String]) -> Result<(), String> {
    if arguments.len() != 2 {
        return Err("--memory-child requires one source path".to_string());
    }
    let path = Path::new(&arguments[1]);
    let source = fs::read_to_string(path)
        .map_err(|error| format!("unable to read {}: {error}", path.display()))?;
    let output = format_text(path, &source)?;
    black_box(&output);
    let memory = memory_stats().ok_or("current-process RSS is unavailable")?;
    println!("{}", memory.physical_mem);
    black_box(output);
    Ok(())
}

pub(crate) fn measure_memory_child(binary: &Path, source: &Path) -> Result<u64, String> {
    let output = Command::new(binary)
        .arg("--memory-child")
        .arg(source)
        .output()
        .map_err(|error| format!("unable to run memory child: {error}"))?;
    if !output.status.success() {
        return Err(format!("memory child failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| error.to_string())?
        .trim()
        .parse::<u64>()
        .map_err(|error| format!("invalid memory child output: {error}"))
}
