use anyhow::{Context, Result};
use clap::Parser;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};
use std::time::Instant;

mod edgelist;
mod palrup;
mod reverse_reader;
mod walker;

use crate::palrup::{Id, PalrupIterator, Step};
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
    for (index, proof_file) in proof_files.iter().enumerate() {
        println!("File {index}: {:?}", proof_file.display());
        writer.add_comment(&format!("File {index}: {:?}", proof_file.display()))?;

        let iterator = PalrupIterator::for_file(proof_file)?;
        let mut min_id = None;
        let mut unused_imports = HashSet::new();

        let mut walker = Walker::default();

        let mut n_imports = 0;
        let mut import_remapping = HashMap::new();

        for entry in iterator {
            match entry? {
                Step::Add(add) => {
                    walker.add_clause(&add);
                    let min_id = min_id.get_or_insert(add.id);
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
        }

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
