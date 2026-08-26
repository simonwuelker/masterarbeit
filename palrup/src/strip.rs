use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use growable_bloom_filter::GrowableBloom;
use rayon::iter::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rustc_hash::FxHashSet;

use crate::palrup::Step::Import;
use crate::palrup::{find_proof_files, Id, PalrupIterator, Step};
use crate::reverse_reader::{ReverseDAGInfo, ReverseDAGIterator};
use crate::StripCommandArgs;

pub(crate) fn strip_command(args: &StripCommandArgs) -> anyhow::Result<()> {
    let proof_files =
        find_proof_files(&args.proof_directory).context("Failed to enumerate proof files")?;

    let mut per_file_forward_info = Vec::with_capacity(proof_files.len());
    proof_files
        .par_iter()
        .map(|proof_file| forward_parse_single_file(proof_file))
        .collect_into_vec(&mut per_file_forward_info);

    // Coalesce the results into one
    let smallest_derived_id = per_file_forward_info
        .first()
        .and_then(|info| info.smallest_id)
        .unwrap();
    let largest_derived_id = per_file_forward_info
        .last()
        .and_then(|info| info.largest_id)
        .unwrap();
    let mut id_of_unsat_clause = None;
    for (index, file_forward_info) in per_file_forward_info.into_iter().enumerate() {
        if let Some(unsat_clause) = file_forward_info.id_of_unsat_clause {
            // It appears that sometimes multiple threads find the unsat clause at the same
            // time. We only consider the last thread to find it.
            id_of_unsat_clause = Some(unsat_clause);
        }
    }
    let Some(id_of_unsat_clause) = id_of_unsat_clause else {
        return Err(anyhow!("Did not find UNSAT clause in any proof file"));
    };

    let info = ReverseDAGInfo::compute(&proof_files, id_of_unsat_clause, smallest_derived_id);
    let mut reverse_dag_iterator = ReverseDAGIterator::new(&info, &proof_files);

    let mut bloom_filter = GrowableBloom::new(0.01, largest_derived_id as usize);
    while let Some(next) = reverse_dag_iterator.next()? {
        match &next.step {
            Step::Add(add_step) => {
                if next.is_critical {
                    bloom_filter.insert(add_step.id);
                }
            }
            Step::Import(import_step) => {
                if next.is_critical {
                    bloom_filter.insert(import_step.imported_clause);
                }
            }
            _ => {}
        }
    }

    proof_files
        .par_iter()
        .map(|proof_file| {
            let relevant_part: PathBuf = proof_file.components().rev().take(3).collect();
            let mut stripped_proof_path = args.stripped_directory.clone();
            stripped_proof_path.extend(relevant_part.components().rev());
            (proof_file, stripped_proof_path)
        })
        .for_each(|(proof_file, stripped_proof_path)| {
            strip_single_file(
                proof_file,
                stripped_proof_path,
                &bloom_filter,
                smallest_derived_id + proof_files.len() as isize,
            )
            .unwrap();
        });

    Ok(())
}

fn strip_single_file<P1, P2>(
    input_file: P1,
    output_file: P2,
    important_filter: &GrowableBloom,
    smallest_reliable_id: isize,
) -> anyhow::Result<()>
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
{
    println!(
        "Moving {} to {}",
        input_file.as_ref().display(),
        output_file.as_ref().display()
    );

    fs::create_dir_all(output_file.as_ref().parent().unwrap())?;
    let mut iterator = PalrupIterator::for_file(input_file)?;
    let mut output_file = BufWriter::new(fs::File::create_new(output_file)?);

    let mut have_written = FxHashSet::default();
    for entry in iterator {
        let mut step = entry.unwrap();
        match &mut step {
            Step::Add(add) => {
                if add.id < smallest_reliable_id {
                    // There's a bug where we don't know about the first clause in each thread.
                    // Let's just assume its always necessary.
                } else {
                    if !important_filter.contains(add.id) {
                        continue;
                    }
                    // If we haven't included all their dependants then we can't write the clause. Its
                    // a false positive anyways.
                    if !add.hints.iter().all(|clause| {
                        *clause < smallest_reliable_id || have_written.contains(clause)
                    }) {
                        continue;
                    }
                }
                have_written.insert(add.id);
            }
            Step::Import(import) => {
                if !important_filter.contains(import.imported_clause) {
                    continue;
                }
                have_written.insert(import.imported_clause);
            }
            Step::Delete(delete) => {
                delete
                    .deleted_clauses
                    .retain(|clause| have_written.remove(clause));
                if delete.deleted_clauses.is_empty() {
                    continue;
                }
            }
        }
        step.write(&mut output_file)?;
    }

    Ok(())
}

#[derive(Debug)]
struct PerFileInfo {
    smallest_id: Option<Id>,
    id_of_unsat_clause: Option<Id>,
    largest_id: Option<Id>,
}

fn forward_parse_single_file(proof_file: impl AsRef<Path>) -> PerFileInfo {
    let proof_file = proof_file.as_ref();

    let iterator = PalrupIterator::for_file(proof_file).unwrap();
    let mut id_of_unsat_clause = None;
    let mut smallest_id = None;
    let mut last_id = None;
    for entry in iterator {
        match entry.unwrap() {
            Step::Add(add) => {
                if smallest_id.is_none() {
                    smallest_id = Some(add.id);
                }
                if add.is_unsat_clause() {
                    id_of_unsat_clause = Some(add.id);
                }

                last_id = Some(add.id);
            }
            _ => {}
        }
    }

    PerFileInfo {
        smallest_id,
        id_of_unsat_clause,
        largest_id: last_id,
    }
}
