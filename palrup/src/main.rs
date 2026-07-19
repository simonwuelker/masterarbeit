use anyhow::{Context, Result};
use clap::Parser;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::{fs, iter};

mod edgelist;
mod metrics;
mod online_covariance;
mod palrup;
mod reverse_reader;
mod walker;

use crate::metrics::{metric_name_for, CovarianceSet, MetricSet, NUMBER_OF_METRICS};
use crate::palrup::{Id, PalrupIterator, Step};
use crate::reverse_reader::ReversePalrupIterator;
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
                            eprintln!(
                                "Found unexpected directory {:?} in proof directory ({:?})",
                                entry.file_name(),
                                solver_thread_directory.path().display()
                            );
                        } else {
                            proof_files.push(entry.path());
                        }
                    }
                } else {
                    eprintln!(
                        "Found unexpected file {:?} in proof directory",
                        solver_thread_directory.file_name()
                    );
                }
            }
        } else {
            eprintln!(
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
}

fn main() -> Result<()> {
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
    let mut index_of_unsat_clause = None;
    for (index, proof_file) in proof_files.iter().enumerate() {
        println!("File {index}: {:?}", proof_file.display());
        writer.add_comment(&format!("File {index}: {:?}", proof_file.display()))?;

        let iterator = PalrupIterator::for_file(proof_file)?;
        let mut min_id = None;
        let mut unused_imports = HashSet::new();

        let mut walker = Walker::default();

        let mut n_imports = 0;
        let mut import_remapping = HashMap::new();

        let mut count = 0;
        for entry in iterator {
            match entry? {
                Step::Add(add) => {
                    walker.add_clause(&add);
                    let min_id = min_id.get_or_insert(add.id);
                    if add.is_unsat_clause() {
                        index_of_unsat_clause = Some(index);
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
            count += 1;
        }
        println!("count {count:?}");

        let usage_stats = walker.finalize();

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
    println!("Walking proof files took {:?}", start.elapsed());

    // Build the reverse tree
    if let Some(index_of_unsat_clause) = index_of_unsat_clause {
        let file_with_unsat_clause = &proof_files[index_of_unsat_clause];
        println!(
            "Walking {:?} backwards because it contains UNSAT clause...",
            file_with_unsat_clause.display()
        );
        let mut covariance_set = CovarianceSet::default();
        let mut reverse_iterator = ReversePalrupIterator::for_file(file_with_unsat_clause)
            .context("Failed to create reverse palrup iterator")?;
        let mut important_clauses = 0;
        let mut total_clauses = 0;
        let mut current_important_clauses = FxHashMap::default();
        loop {
            let Some(next) = reverse_iterator.next().unwrap() else {
                break;
            };
            if let Step::Add(add) = &next {
                total_clauses += 1;

                let incoming_edges = current_important_clauses.remove(&add.id);

                let is_critical = add.is_unsat_clause() || incoming_edges.is_some();
                if is_critical {
                    for ancestor in &add.hints {
                        *current_important_clauses.entry(*ancestor).or_default() += 1;
                    }
                    important_clauses += 1;
                };

                let metrics = MetricSet {
                    is_critical,
                    number_of_literals: add.literals.len(),
                    incoming_edges: add.hints.len(),
                    outgoing_edges: incoming_edges.unwrap_or_default(),
                };
                covariance_set.add_sample(metrics);
            }
        }
        println!(
            "{:?}/{:?} clauses important",
            important_clauses, total_clauses
        );

        use tabled::assert::assert_table;
        use tabled::{builder::Builder, settings::Style};

        let sample_covariance = covariance_set.sample_covariance().unwrap();
        let mut table_builder =
            Builder::with_capacity(NUMBER_OF_METRICS + 1, NUMBER_OF_METRICS + 1);
        table_builder.push_record(
            iter::once("Covariance").chain(
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

                row_data.push(sample_covariance[row][column - row].to_string())
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
