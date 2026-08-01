use anyhow::{Context, Result};
use clap::Parser;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::{fs, iter};
use tabled::{builder::Builder, settings::Style};
use env_logger::Env;

mod edgelist;
mod evaluation;
mod palrup;
mod reverse_reader;
mod walker;

use crate::evaluation::histograms::HistogramSet;
use crate::evaluation::metrics::{metric_name_for, CovarianceSet, MetricSet, NUMBER_OF_METRICS};
use crate::palrup::{Id, PalrupIterator, Step};
use crate::reverse_reader::{ReverseDAGInfo, ReverseDAGIterator};
use crate::walker::{Walker, TRACK_DERIVATIVES_UP_TO};

/// Transpiler from PalRup proof files to edge lists
#[derive(Parser, Debug)]
#[clap(version, about, long_about = None)]
struct Args {
    /// Path to proof directory
    #[clap(short, long)]
    proof_directory: PathBuf,

    /// Path to write the output file to
    #[clap(short, long, default_value = "out.edgelist")]
    output_file: PathBuf,
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

    let args = Args::parse();
    let out_file = fs::File::create(&args.output_file).with_context(|| {
        format!(
            "Failed to create output graph file {}",
            args.output_file.display()
        )
    })?;

    let mut writer = edgelist::Writer::new(BufWriter::new(out_file));

    // Walk dir for solver processes
    let proof_files = find_proof_files(&args.proof_directory)?;

    let mut max = Id::MAX;
    let mut result_data = ResultData::default();
    let start = Instant::now();
    let mut id_of_unsat_clause = None;
    for (index, proof_file) in proof_files.iter().enumerate() {
        writer.add_comment(&format!("File {index}: {:?}", proof_file.display()))?;

        let iterator = PalrupIterator::for_file(proof_file)?;
        let mut unused_imports = HashSet::new();

        let mut walker = Walker::default();

        let mut n_imports = 0;
        let mut import_remapping = HashMap::new();

        let mut step_count = 0;
        for entry in iterator {
            match entry? {
                Step::Add(add) => {
                    walker.add_clause(&add);
                    if add.is_unsat_clause() {
                        id_of_unsat_clause = Some(add.id);
                    }
                    for derived_from in add.hints {
                        // if derived_from < *min_id {
                        //     continue;
                        // }
                        let remapped_node =
                            import_remapping.get(&derived_from).unwrap_or(&derived_from);
                        unused_imports.remove(remapped_node);
                        writer.add_connection(derived_from, add.id)?;
                    }
                }
                Step::Import(import) => {
                    n_imports += 1;
                    let import_node = max;
                    max -= 1;
                    import_remapping.insert(import.imported_clause, import_node);

                    writer.add_comment(&format!(
                        "Import {} as {import_node}",
                        import.imported_clause
                    ))?;
                    writer.add_connection(import.imported_clause, import_node)?;
                    unused_imports.insert(import_node);

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
            "File {index}: {:?} {step_count} steps, {:?} clauses added, {:?} clauses deleted, {:?} clauses imported", proof_file.display(),
            usage_stats.num_additions, usage_stats.num_deletions, usage_stats.num_imports
        );

        result_data.per_file.insert(
            proof_file.to_owned(),
            PerFileInfo {
                import_depths: usage_stats
                    .import_depth
                    .map(|depth| depth as f32 / n_imports as f32),
            },
        );
        result_data
            .unused_imports
            .extend_from_slice(&usage_stats.unused_imports);
    }
    log::debug!("Walking proof files took {:?}", start.elapsed());

    // Build the reverse tree
    if let Some(id_of_unsat_clause) = id_of_unsat_clause {
        log::info!("Constructing reverse DAG...");
        let info = ReverseDAGInfo::compute(&proof_files, id_of_unsat_clause);
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
                        outgoing_edges: add_step.hints.len(), // FIXME
                        id: add_step.id as usize,
                        lifetime: lifetime as usize,
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
            important_clauses, total_clauses
        );

        let sample_correlation = covariance_set.pearson_correlation().unwrap();
        let mut table_builder =
            Builder::with_capacity(NUMBER_OF_METRICS + 1, NUMBER_OF_METRICS + 1);
        table_builder.push_record(
            iter::once("Pearson").chain(
                (0..NUMBER_OF_METRICS)
                    .map(|index| metric_name_for(index))
                    .collect::<Vec<_>>(),
            ),
        );
        for row in 0..NUMBER_OF_METRICS {
            let mut row_data = Vec::with_capacity(NUMBER_OF_METRICS + 1);
            row_data.push(metric_name_for(row).to_string());
            for column in 0..NUMBER_OF_METRICS {
                if row > column {
                    row_data.push("".to_string());
                    continue;
                }

                row_data.push(format!("{:.5}", sample_correlation[row][column - row]));
            }

            table_builder.push_record(row_data);
        }

        let mut table = table_builder.build();
        table.with(Style::modern());
        println!("{table}");
    } else {
        println!("None of the proof files found a UNSAT clause, skipping reverse treebuilding...");
    }

    result_data.unused_imports.sort_unstable();

    let result_path = "out.json";
    if fs::exists(&result_path)? {
        fs::remove_file(&result_path)?;
    }
    let outfile = fs::File::create(&result_path)?;
    serde_json::to_writer(outfile, &result_data)?;

    Ok(())
}

#[derive(Debug, Default, Serialize)]
struct PerFileInfo {
    import_depths: [f32; TRACK_DERIVATIVES_UP_TO as usize],
}
