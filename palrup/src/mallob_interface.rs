use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::process::Stdio;
use std::sync::LazyLock;

use anyhow::{anyhow, Context};
use regex::Regex;

static COMPUTE_TIME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| regex::Regex::new(r"RESPONSE_TIME\s+#\d+\s+(\d+(?:\.\d+)?)").unwrap());

pub(crate) struct MallobInvocationResult {
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) proof_directory: PathBuf,
    /// Time mallob took to sovle the problem, in seconds.
    pub(crate) compute_time: f64,
}

pub(crate) fn invoke_mallob(
    mallob_binary: impl AsRef<Path>,
    problem: impl AsRef<Path>,
    temp_dir: impl AsRef<Path>,
) -> anyhow::Result<MallobInvocationResult> {
    let num_threads = std::thread::available_parallelism()?.get();
    let num_procs = num_threads / 8;

    let mallob_binary = mallob_binary.as_ref();
    let problem = problem.as_ref();
    let temp_dir = temp_dir.as_ref();

    let mut command = process::Command::new("mpirun".to_owned());
    command
        .env("RDMAV_FORK_SAFE", "1")
        .env("NPROCS", num_procs.to_string())
        .args([
            "-np".to_string(),
            num_procs.to_string(),
            "--bind-to=core".to_string(),
            "--map-by".to_string(),
            format!("ppr:{num_procs}:node:pe=4"),
            format!("{}", mallob_binary.display()),
            "-t=4".to_string(),
            format!("-mono={}", problem.display()),
            "-satsolver=c".to_string(),
            "--palrup".to_string(),
            format!("-proof-dir={}", temp_dir.display()),
        ])
        .stdout(Stdio::piped());
    log::debug!("Invoking {command:?}");
    let child_handle = command.spawn()?;

    let output = child_handle
        .wait_with_output()
        .context("Waiting for mallob to complete")?;

    if !output.status.success() {
        log::error!(
            "Mallob invocation failed with exit code {:?}",
            output.status.code()
        );
    }

    // Find the directory containing the solver traces (no idea how mallob determines that)
    let mut proof_directory = None;
    for entry in fs::read_dir(temp_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            proof_directory = Some(entry.path());
        }
    }
    let Some(proof_directory) = proof_directory else {
        log::error!("Did not find any proof files");
        return Err(anyhow!("Did not find any proof files"));
    };
    log::debug!("Proof was stored in {}", proof_directory.display());

    // Find time to solve
    let stdout_string = String::from_utf8_lossy(&output.stdout);
    let compute_time = if let Some(caps) = COMPUTE_TIME_REGEX.captures(&stdout_string) {
        caps[1].parse().unwrap()
    } else {
        f64::NAN
    };

    Ok(MallobInvocationResult {
        stdout: output.stdout,
        stderr: output.stderr,
        proof_directory,
        compute_time,
    })
}
