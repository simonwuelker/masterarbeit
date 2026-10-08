use crate::depths;
use crate::depths::DepthResult;
use crate::mallob_interface::invoke_mallob;
use crate::strip;
use crate::DepthVolumeTimeCommandArgs;
use crate::StripCommandArgs;

use std::fs;

use anyhow::anyhow;
use anyhow::Context;
use serde::Serialize;

pub(crate) fn depthvolumetimemain(
    args: DepthVolumeTimeCommandArgs,
) -> anyhow::Result<Vec<SingleAnalysisResult>> {
    // Find all problem files
    let mut problem_files = vec![];
    for entry in fs::read_dir(&args.problem_directory)
        .with_context(|| {
            format!(
                "Reading problem directory ({})",
                args.problem_directory.display()
            )
        })?
        .take(10)
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
    for (index, problem) in problem_files.iter().enumerate() {
        log::info!(
            "Handling {} ({}/{})",
            problem.display(),
            index + 1,
            problem_files.len()
        );

        // Ensure temporary directory exists
        if !fs::exists(temp_dir).context("Checking if temp dir exists")? {
            fs::create_dir(temp_dir).context("Creating temporary directory")?;
        }

        let result = invoke_mallob(&args.mallob_binary, problem, temp_dir)?;

        // Strip the resulting proof
        log::info!("Stripping proof in {}", result.proof_directory.display());
        strip::strip_command(&StripCommandArgs {
            proof_directory: result.proof_directory.clone(),
            stripped_directory: stripped_directory.clone(),
            error_probability: 0.01,
        })?;

        // Compute depth
        let depth = depths::depths(&stripped_directory)?;

        results.push(SingleAnalysisResult {
            problem: problem.display().to_string(),
            time: result.compute_time,
            depth_result: depth,
        });

        // Clear temporary directory
        log::debug!("Clearing temporary directory");
        fs::remove_dir_all(temp_dir).context("Clearing temporary directory")?;
    }

    Ok(results)
}

#[derive(Serialize)]
pub(crate) struct SingleAnalysisResult {
    problem: String,
    time: f64,
    #[serde(flatten)]
    depth_result: DepthResult,
}
