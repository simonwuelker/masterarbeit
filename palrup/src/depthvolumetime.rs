use crate::depths;
use crate::depths::DepthResult;
use crate::mallob_interface::invoke_mallob;
use crate::mallob_interface::InvocationMode;
use crate::mallob_interface::TimeoutOrResult;
use crate::strip;
use crate::strip::StripResult;
use crate::DepthVolumeTimeCommandArgs;
use crate::StripCommandArgs;

use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use anyhow::anyhow;
use anyhow::Context;
use serde::Serialize;

pub(crate) fn depthvolumetimemain(args: DepthVolumeTimeCommandArgs) -> anyhow::Result<Result> {
    // Find all problem files
    let mut problem_files = vec![];
    for entry in fs::read_dir(&args.problem_directory)
        .with_context(|| {
            format!(
                "Reading problem directory ({})",
                args.problem_directory.display()
            )
        })?
    {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            log::warn!(
                "Expected no directories in {}",
                args.problem_directory.display()
            );
            continue;
        }
        problem_files.push(entry.path());
    }

    let temp_dir = &args.temp_directory;
    log::debug!("Using {} to temporarily store proofs", temp_dir.display());

    if fs::exists(temp_dir).context("Checking if temp dir exists")? {
        if fs::read_dir(temp_dir)?.next().is_some() {
            log::error!(
                "{} is not empty, refusing to put palrup proofs in there",
                temp_dir.display()
            );
            return Err(anyhow!(
                "{} is not empty, refusing to put palrup proofs in there",
                temp_dir.display()
            ));
        }
    }
    let stripped_directory = temp_dir.join("stripped");

    let mut results = vec![];
    let checkpoint_file = args.checkpoints.join(format!("checkpoint.json"));
    for (index, problem) in problem_files.iter().enumerate() {
        log::info!(
            "Handling {} ({}/{})",
            problem.display(),
            index + 1,
            problem_files.len()
        );
        let problem_name = String::from_utf8_lossy(problem.as_os_str().as_bytes()).to_string();

        // Ensure temporary directory exists and is empty
        if fs::exists(temp_dir).context("Checking if temp dir exists")? {
            // Clear temporary directory
            log::debug!("Clearing temporary directory");
            fs::remove_dir_all(temp_dir).context("Clearing temporary directory")?;
        }
        fs::create_dir(temp_dir).context("Creating temporary directory")?;

        log::debug!("Solving sequentially...");
        let result = match invoke_mallob(
            &args.mallob,
            problem,
            temp_dir,
            InvocationMode::Sequential,
            None,
        )? {
            TimeoutOrResult::Success(result) => result,
            TimeoutOrResult::Timeout => {
                log::error!("Timed out.");
                results.push(ProblemResult {
                    problem: problem_name,
                    analysis_or_error: AnalysisOrError::TimedOut,
                });
                continue;
            }
        };

        let Ok(sequential_analysis) = analyze_proof_certificate(
            result.proof_directory,
            &stripped_directory,
            result.compute_time,
        ) else {
            log::error!("Internal error");
            results.push(ProblemResult {
                problem: problem_name,
                analysis_or_error: AnalysisOrError::InternalError,
            });
            continue;
        };

        // Ensure temporary directory exists and is empty
        if fs::exists(temp_dir).context("Checking if temp dir exists")? {
            // Clear temporary directory
            log::debug!("Clearing temporary directory");
            fs::remove_dir_all(temp_dir).context("Clearing temporary directory")?;
        }
        fs::create_dir(temp_dir).context("Creating temporary directory")?;

        log::debug!("Running in parallel...");
        let result = match invoke_mallob(
            &args.mallob,
            problem,
            temp_dir,
            InvocationMode::Parallel,
            None,
        )? {
            TimeoutOrResult::Success(result) => result,
            TimeoutOrResult::Timeout => {
                log::error!("Timed out.");
                results.push(ProblemResult {
                    problem: problem_name,
                    analysis_or_error: AnalysisOrError::TimedOut,
                });
                continue;
            }
        };

        let Ok(parallel_analysis) = analyze_proof_certificate(
            result.proof_directory,
            &stripped_directory,
            result.compute_time,
        ) else {
            log::error!("Internal error");
            results.push(ProblemResult {
                problem: problem_name,
                analysis_or_error: AnalysisOrError::InternalError,
            });
            continue;
        };

        results.push(ProblemResult {
            problem: problem_name,
            analysis_or_error: AnalysisOrError::Success(ParallelAndSequential {
                sequential: sequential_analysis,
                parallel: parallel_analysis,
            }),
        });

        log::info!("Saving checkpoint to {}", checkpoint_file.display());
        if fs::exists(&checkpoint_file)? {
            fs::remove_file(&checkpoint_file)?;
        }
        let checkpoint_file = fs::File::create_new(&checkpoint_file)?;
        serde_json::to_writer(checkpoint_file, &results)?;

        // Clear temporary directory
        log::debug!("Clearing temporary directory");
        fs::remove_dir_all(temp_dir).context("Clearing temporary directory")?;
    }

    Ok(Result {
        hostname: String::from_utf8_lossy(gethostname::gethostname().as_encoded_bytes())
            .to_string(),
        results,
    })
}

fn analyze_proof_certificate(
    proof_directory: impl AsRef<Path>,
    stripped_directory: impl AsRef<Path>,
    time: f64,
) -> anyhow::Result<SingleAnalysisResult> {
    // Strip the resulting proof
    log::info!("Stripping proof in {}", proof_directory.as_ref().display());
    let strip_result = strip::strip_command(&StripCommandArgs {
        proof_directory: proof_directory.as_ref().to_owned(),
        stripped_directory: stripped_directory.as_ref().to_owned(),
        error_probability: 0.01,
    })?;

    // Compute depth
    let depth = depths::depths(&stripped_directory)?;

    Ok(SingleAnalysisResult {
        time,
        depth_result: depth,
        strip_result,
    })
}

#[derive(Serialize)]
pub(crate) struct Result {
    hostname: String,
    results: Vec<ProblemResult>,
}

#[derive(Serialize)]
pub(crate) struct ProblemResult {
    problem: String,
    analysis_or_error: AnalysisOrError,
}

#[derive(Serialize)]
pub(crate) enum AnalysisOrError {
    InternalError,
    TimedOut,
    Success(ParallelAndSequential),
}

#[derive(Serialize)]
pub(crate) struct ParallelAndSequential {
    sequential: SingleAnalysisResult,
    parallel: SingleAnalysisResult,
}

#[derive(Serialize)]
pub(crate) struct SingleAnalysisResult {
    time: f64,
    #[serde(flatten)]
    depth_result: DepthResult,
    #[serde(flatten)]
    strip_result: StripResult,
}
