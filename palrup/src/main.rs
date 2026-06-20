use anyhow::{Context, Result};
use clap::Parser;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};

mod edgelist;
mod palrup;
mod walker;

use crate::palrup::{Id, PalrupIterator, Step};
use crate::walker::Walker;

/// Transpiler from PalRup proof files to edge lists
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to proof directory
    #[arg(short, long)]
    proof_directory: PathBuf,

    /// Path to write the output file to
    #[arg(short, long, default_value = "out.edgelist")]
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
    for (index, proof_file) in proof_files.iter().enumerate().skip(30).take(1) {
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
        println!("{}/{n_imports} unused imports", unused_imports.len());
        if unused_imports.contains(&3688) {
            println!("3688 was unused i think!");
        }
        println!("Walker stats: {:?}", walker.finalize());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // #[test]
    // fn parse_clause() {
    //     let source = &[0x61, 0xb4, 0x0e, 0x0b, 0x75, 0xf4, 0x02, 0xf8, 0x02, 0x80, 0x03, 0x82, 0x03, 0x86, 0x03, 0x90]
    //     61b4 0e0b 75f4 02f8 0280 0382 0386 0390  a...u...........
    //     00000010: 0300 f209 c208 8803 f404 b40c ee08 cc0a  ................
    //     00000020: fa0a be08 ce0b f008 8c07 9009 b206 f606  ................
    //     00000030: f002 ec03 00
    // }
}
