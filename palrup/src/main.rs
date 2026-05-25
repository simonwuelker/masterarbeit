use anyhow::{Context, Result};
use clap::Parser;
use std::fs;
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};

mod edgelist;
mod palrup;

use crate::palrup::{PalrupIterator, Step};

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

    for (index, proof_file) in proof_files.iter().enumerate() {
        println!("File {index}: {:?}", proof_file.display());

        let iterator = PalrupIterator::for_file(proof_file)?;
        let mut min_id = None;
        for entry in iterator {
            let step = entry?;
            if let Step::Add(add) = step {
                let min_id = min_id.get_or_insert(add.id);
                for derived_from in add.hints {
                    if derived_from < *min_id {
                        continue;
                    }
                    writer.add_connection(derived_from, add.id)?;
                }
            }
        }
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
