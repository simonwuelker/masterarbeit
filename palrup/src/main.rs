use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser, Subcommand};
use env_logger::Env;
use rayon::iter::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::Instant;
use std::{env, io, process};
use std::{fs, iter};

// mod edgelist;
mod evaluation;
mod palrup;
mod reverse_reader;
mod walker;

use crate::evaluation::histograms::HistogramSet;
use crate::evaluation::metrics::{metric_name_for, CovarianceSet, MetricSet, NUMBER_OF_METRICS};
use crate::evaluation::online_covariance::OnlineCovariance;
use crate::palrup::{Id, PalrupIterator, Step};
use crate::reverse_reader::{ReverseDAGInfo, ReverseDAGIterator};
use crate::walker::{Walker, TRACK_DERIVATIVES_UP_TO};

/// Transpiler from PalRup proof files to edge lists
#[derive(Parser, Debug)]
#[clap(version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Arguments {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Local(LocalCommandArgs),
    Server(ServerCommandArgs),
}

#[derive(Args, Debug)]
struct LocalCommandArgs {
    /// Path to proof directory
    #[clap(short, long)]
    proof_directory: PathBuf,
}

#[derive(Args, Debug)]
struct ServerCommandArgs {
    /// Path a directory containing problem instances.
    #[clap(long)]
    problem_directory: PathBuf,

    /// Path to the mallob binary that should be used for solving problems.
    #[clap(long)]
    mallob_binary: PathBuf,

    /// Path to a garbage directory that temporary proof files can be stored in.
    ///
    /// If this is not provided, then a appropriate directory will be inferred
    /// (eg `/tmp/palrup-proofs` on linux).
    #[clap(long)]
    temp_directory: Option<PathBuf>,
}

fn find_proof_files<P: AsRef<Path>>(proof_directory: P) -> io::Result<Vec<PathBuf>> {
    let mut proof_files = Vec::new();

    for entry in fs::read_dir(&proof_directory)? {
        let solver_process_directory = entry?;
        if solver_process_directory.file_type()?.is_dir() {
            // Walk dir for solver threads
            for entry in fs::read_dir(solver_process_directory.path())? {
                let solver_thread_directory = entry?;
                if solver_thread_directory.file_type()?.is_dir() {
                    // Walk proof files
                    for entry in fs::read_dir(solver_thread_directory.path())? {
                        let entry = entry?;
                        if entry.file_type()?.is_dir() {
                            log::warn!(
                                "Found unexpected directory {:?} in proof directory ({:?})",
                                entry.file_name(),
                                solver_thread_directory.path().display()
                            );
                        } else {
                            proof_files.push(entry.path());
                        }
                    }
                } else {
                    log::warn!(
                        "Found unexpected file {:?} in proof directory",
                        solver_thread_directory.file_name()
                    );
                }
            }
        } else {
            log::warn!(
                "Found unexpected file {:?} in proof directory",
                solver_process_directory.file_name()
            );
        }
    }

    Ok(proof_files)
}

#[derive(Debug, Default, Serialize)]
struct ResultData {
    per_file: HashMap<PathBuf, PerFileInfo>,
    unused_imports: Vec<Id>,
    histogram_set: Option<HistogramSet>,
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();

    let args = Arguments::parse();

    match args.command {
        Commands::Local(local_args) => {
            log::info!("Using local mode");
            let result = local_main(&local_args.proof_directory)?;

            let result_path = "out.json";
            if fs::exists(&result_path)? {
                fs::remove_file(&result_path)?;
            }
            let outfile = fs::File::create(&result_path)?;
            serde_json::to_writer(outfile, &result.result_data)?;
        }
        Commands::Server(server_args) => {
            log::info!("Using server mode");
            server_main(server_args)?;
        }
    }

    Ok(())
}

fn local_main(proof_directory: &Path) -> Result<SingleAnalysisResult> {
    // Walk dir for solver processes
    let mut proof_files = find_proof_files(proof_directory)?;
    proof_files.sort_unstable();

    log::info!("Parsing proof files");
    let mut result_data = ResultData::default();
    let start = Instant::now();
    let mut per_file_forward_info = Vec::with_capacity(proof_files.len());
    proof_files
        .par_iter()
        .map(|proof_file| forward_parse_single_file(proof_file))
        .collect_into_vec(&mut per_file_forward_info);

    // Coalesce results that we collected in parallel
    let mut id_of_unsat_clause = None;
    let smallest_derived_id = per_file_forward_info
        .first()
        .and_then(|info| info.smallest_id)
        .unwrap();
    for (index, file_forward_info) in per_file_forward_info.into_iter().enumerate() {
        if let Some(unsat_clause) = file_forward_info.id_of_unsat_clause {
            // It appears that sometimes multiple threads find the unsat clause at the same
            // time. We only consider the last thread to find it.
            id_of_unsat_clause = Some(unsat_clause);
        }
        result_data
            .per_file
            .insert(proof_files[index].to_owned(), file_forward_info);
    }
    log::debug!("Walking proof files took {:?}", start.elapsed());

    // Build the reverse tree
    let Some(id_of_unsat_clause) = id_of_unsat_clause else {
        log::error!("None of the proof files found a UNSAT clause");
        return Err(anyhow!("No UNSAT clause found"));
    };

    log::info!("Constructing reverse DAG...");
    let info = ReverseDAGInfo::compute(&proof_files, id_of_unsat_clause, smallest_derived_id);
    let mut reverse_dag_iterator = ReverseDAGIterator::new(&info, &proof_files);

    let mut covariance_set = CovarianceSet::default();
    let mut histogram_set = HistogramSet::default();

    let mut important_clauses = 0;
    let mut total_clauses = 0;
    let mut clause_gets_deleted_at = FxHashMap::default();
    let mut last_id = Id::MAX;
    while let Some(next) = reverse_dag_iterator.next()? {
        match &next.step {
            Step::Add(add_step) => {
                if next.is_critical {
                    important_clauses += 1;
                }

                last_id = add_step.id;
                total_clauses += 1;

                let lifetime = clause_gets_deleted_at
                    .remove(&add_step.id)
                    .unwrap_or(id_of_unsat_clause)
                    - add_step.id;

                let metrics = MetricSet {
                    is_critical: next.is_critical,
                    number_of_literals: add_step.literals.len(),
                    incoming_edges: add_step.hints.len(),
                    outgoing_edges: next.outgoing_edges,
                    id: add_step.id as usize,
                    lifetime: lifetime as usize,
                    minimum_lifetime: next.minimum_lifetime,
                };
                covariance_set.add_sample(metrics);
                histogram_set.add_sample(metrics);
            }
            Step::Delete(delete_step) => {
                for deleted_clause in &delete_step.deleted_clauses {
                    clause_gets_deleted_at.insert(*deleted_clause, last_id);
                }
            }
            _ => {}
        }
    }
    result_data.histogram_set = Some(histogram_set);
    log::info!(
        "{:?}/{:?} clauses important",
        important_clauses,
        total_clauses
    );

    covariance_set.pearson_correlation().unwrap().debug_print();

    result_data.unused_imports.sort_unstable();

    Ok(SingleAnalysisResult {
        covariance_set,
        result_data,
    })
}

struct SingleAnalysisResult {
    covariance_set: CovarianceSet,
    result_data: ResultData,
}

const NUM_PROBLEMS_TO_ANALYZE: usize = 10;

fn server_main(args: ServerCommandArgs) -> Result<()> {
    // Find all problem files
    let mut problem_files = Vec::with_capacity(NUM_PROBLEMS_TO_ANALYZE);
    for entry in fs::read_dir(&args.problem_directory)
        .with_context(|| {
            format!(
                "Reading problem directory ({})",
                args.problem_directory.display()
            )
        })?
        .take(NUM_PROBLEMS_TO_ANALYZE)
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

    let temp_dir = args
        .temp_directory
        .unwrap_or_else(|| env::temp_dir().join("/palrup-proofs"));
    let temp_dir = &temp_dir;
    log::debug!("Using {} to temporarily store proofs", temp_dir.display());
    if !fs::exists(temp_dir)? {
        // Ensure temporary directory exists
        fs::create_dir(temp_dir).context("Creating temporary directory")?;
    } else {
        if fs::read_dir(temp_dir)?.next().is_none() {
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

    let num_threads = std::thread::available_parallelism()?.get();
    let num_procs = num_threads / 8;
    let mut covariance_set = CovarianceSet::default();
    log::debug!("Using {num_threads} mallob solver threads");
    for problem in &problem_files {
        log::info!("Solving {}", problem.display());

        // Ensure temporary directory exists
        if !fs::exists(temp_dir)? {
            fs::create_dir(temp_dir).context("Creating temporary directory")?;
        }

        // Run mallob on that problem
        let child_handle = process::Command::new("mpirun")
            .env("RDMAV_FORK_SAFE", "1")
            .env("NPROCS", num_procs.to_string())
            .args([
                "-np".to_string(),
                num_procs.to_string(),
                "--bind-to-core".to_string(),
                "--map-by ppr:${NPROCS}:node:pe=4".to_string(),
                format!("{}", args.mallob_binary.display()),
                "-t=4".to_string(),
                format!("-mono={}", problem.display()),
                "-satsolver=c".to_string(),
                "--palrup".to_string(),
                format!("-proof-dir={}", temp_dir.display()),
            ])
            .spawn()?;

        let output = child_handle
            .wait_with_output()
            .context("Waiting for mallob to complete")?;
        if output.status.success() {
            log::error!(
                "Mallob invocation failed with exit code {:?}",
                output.status.code()
            );
            return Err(anyhow!("Mallob invocation failed"));
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

        let result = local_main(&proof_directory).context("Analyzing proof files")?;
        log::info!("result:");
        result
            .covariance_set
            .pearson_correlation()
            .unwrap()
            .debug_print();
        covariance_set = CovarianceSet::combine(covariance_set, result.covariance_set);

        // Clear temporary directory
        fs::remove_dir_all(temp_dir).context("Clearing temporary directory")?;
    }

    log::info!("Pearson correlation over all files:");
    covariance_set.pearson_correlation().unwrap().debug_print();

    Ok(())
}

#[derive(Debug, Default, Serialize)]
struct PerFileInfo {
    smallest_id: Option<Id>,
    id_of_unsat_clause: Option<Id>,
    import_depths: [f32; TRACK_DERIVATIVES_UP_TO as usize],
}

fn forward_parse_single_file(proof_file: impl AsRef<Path>) -> PerFileInfo {
    let proof_file = proof_file.as_ref();

    let iterator = PalrupIterator::for_file(proof_file).unwrap();
    let mut unused_imports = FxHashSet::default();
    let mut walker = Walker::default();

    let mut id_of_unsat_clause = None;
    let mut step_count = 0;
    let mut smallest_id = None;
    for entry in iterator {
        match entry.unwrap() {
            Step::Add(add) => {
                if smallest_id.is_none() {
                    smallest_id = Some(add.id);
                }
                walker.add_clause(&add);
                if add.is_unsat_clause() {
                    id_of_unsat_clause = Some(add.id);
                }
                for derived_from in add.hints {
                    unused_imports.remove(&derived_from);
                }
            }
            Step::Import(import) => {
                unused_imports.insert(import.imported_clause);

                walker.import_clause(import);
            }
            Step::Delete(deletion) => {
                for clause in deletion.deleted_clauses {
                    walker.forget_clause(clause);
                }
            }
        }
        step_count += 1;
    }

    let usage_stats = walker.finalize();
    log::info!(
        "{:?}: {step_count} steps, {:?} clauses added, {:?} clauses deleted, {:?} clauses imported, has UNSAT clause: {:?}", proof_file.display(),
        usage_stats.num_additions, usage_stats.num_deletions, usage_stats.num_imports, id_of_unsat_clause.is_some()
    );

    PerFileInfo {
        smallest_id,
        id_of_unsat_clause,
        import_depths: usage_stats
            .import_depth
            .map(|depth| depth as f32 / usage_stats.num_imports as f32),
    }
}
