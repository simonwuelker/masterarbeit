use std::fs::File;
use std::mem;
use std::path::{Path, PathBuf};
use std::{
    io::{self, BufReader, Read, Seek, SeekFrom},
    iter::Rev,
};

use crate::palrup::{self, Id, Step};
use anyhow::{Context, Result};
use rustc_hash::{FxHashMap, FxHashSet};

struct ChunkIterator<R: Read + Seek, const N: usize> {
    reader: R,
    /// This is not necessarily in sync with the position of `reader`.
    current_position: u64,
}

impl<R: Read + Seek, const N: usize> ChunkIterator<R, N> {
    pub(crate) fn new(mut reader: R) -> io::Result<Self> {
        let current_position = reader.seek(SeekFrom::End(0))?;
        Ok(Self {
            current_position,
            reader,
        })
    }

    pub(crate) fn next<'a>(&mut self, mut buffer: Vec<u8>) -> io::Result<Option<Vec<u8>>> {
        if self.current_position == 0 {
            return Ok(None);
        }
        if self.current_position < N as u64 {
            let position = self.current_position as usize;
            self.current_position = 0;
            self.reader.rewind()?;
            buffer.resize(position, 0);
            self.reader.read_exact(&mut buffer)?;
            return Ok(Some(buffer));
        }

        self.current_position -= N as u64;
        self.reader.seek(SeekFrom::Start(self.current_position))?;

        buffer.resize(N, 0);
        self.reader.read_exact(&mut buffer)?;

        Ok(Some(buffer))
    }
}

const CHUNK_SIZE: usize = 1 << 24;

pub(crate) struct ReversePalrupIterator<R: Read + Seek> {
    chunk_iterator: ChunkIterator<R, CHUNK_SIZE>,
    remaining_items_from_current_chunk: Option<Rev<<Vec<palrup::Step> as IntoIterator>::IntoIter>>,
    buffer: Vec<u8>,
    remaining_stuff_to_append_to_next_chunk: Vec<u8>,
}

impl<R: Read + Seek> ReversePalrupIterator<R> {
    pub(crate) fn next(&mut self) -> io::Result<Option<palrup::Step>> {
        if let Some(remaining_items) = &mut self.remaining_items_from_current_chunk {
            if let Some(next_item) = remaining_items.next() {
                return Ok(Some(next_item));
            } else {
                self.remaining_items_from_current_chunk = None;
            }
        }

        // There are no more items in the current chunk, fetch a new one
        let Some(mut next_chunk) = self.chunk_iterator.next(mem::take(&mut self.buffer))? else {
            return Ok(None);
        };
        next_chunk.extend_from_slice(&self.remaining_stuff_to_append_to_next_chunk);
        self.remaining_stuff_to_append_to_next_chunk.clear();

        let start_of_first_step = next_chunk
            .windows(2)
            .position(|window| window[0] == 0 && matches!(window[1], b'a' | b'i' | b'd'))
            .expect("no step in entire chunk?")
            + 1;
        self.remaining_stuff_to_append_to_next_chunk
            .extend_from_slice(&next_chunk[..start_of_first_step]);
        debug_assert!(self
            .remaining_stuff_to_append_to_next_chunk
            .last()
            .is_some_and(|byte| *byte == 0));

        let steps: Result<Vec<_>> =
            palrup::PalrupIterator::new(io::Cursor::new(&next_chunk[start_of_first_step..]))
                .collect();
        self.remaining_items_from_current_chunk = Some(steps.unwrap().into_iter().rev());

        self.next()
    }
}

impl<R: Read + Seek> ReversePalrupIterator<R> {
    pub(crate) fn new(reader: R) -> io::Result<Self> {
        Ok(Self {
            chunk_iterator: ChunkIterator::new(reader)?,
            remaining_items_from_current_chunk: None,
            buffer: Vec::default(),
            remaining_stuff_to_append_to_next_chunk: Vec::default(),
        })
    }
}

impl ReversePalrupIterator<BufReader<File>> {
    pub(crate) fn for_file<P: AsRef<Path>>(file: P) -> Result<Self> {
        let file = File::open(&file)
            .with_context(|| format!("Failed to read proof from {}", file.as_ref().display()))?;
        let reader = BufReader::new(file);

        Ok(Self::new(reader)?)
    }
}

pub(crate) struct ReverseDAGIterator<'a> {
    important_roots: std::collections::hash_map::Iter<'a, usize, FxHashSet<Id>>,
    /// Maps from clause ID to statistics on how that clause has been used.
    clause_stats: FxHashMap<Id, ClauseStats>,
    palrup_files: &'a [PathBuf],
    current_file: Option<(ReversePalrupIterator<BufReader<File>>, FxHashSet<Id>)>,
}

#[derive(Clone, Copy, Debug)]
struct ClauseStats {
    number_of_outgoing_edges: usize,
    first_used_at: Id,
}

pub(crate) struct ReverseDAGInfo {
    /// Maps from solver id to the roots of the important-dag-subtrees in that solvers file.
    important_roots: FxHashMap<usize, FxHashSet<Id>>,
}

pub(crate) struct StepInfo {
    pub(crate) is_critical: bool,
    pub(crate) outgoing_edges: usize,
    pub(crate) minimum_lifetime: usize,
    pub(crate) step: Step,
}

impl<'a> ReverseDAGIterator<'a> {
    pub(crate) fn new(info: &'a ReverseDAGInfo, palrup_files: &'a [PathBuf]) -> Self {
        Self {
            important_roots: info.important_roots.iter(),
            palrup_files,
            current_file: None,
            clause_stats: Default::default(),
        }
    }

    pub(crate) fn next(&mut self) -> Result<Option<StepInfo>> {
        if let Some((current_file_iterator, important_ids)) = self.current_file.as_mut() {
            if let Some(next_step_in_current_file) = current_file_iterator.next()? {
                let mut clause_stats = None;
                let mut minimum_lifetime = 0;
                let is_critical = match &next_step_in_current_file {
                    Step::Add(add) => {
                        let is_important = important_ids.remove(&add.id);
                        if is_important {
                            for derived_from in &add.hints {
                                important_ids.insert(*derived_from);
                            }
                        }
                        for derived_from in &add.hints {
                            self.clause_stats
                                .entry(*derived_from)
                                .or_insert(ClauseStats {
                                    number_of_outgoing_edges: 0,
                                    first_used_at: add.id,
                                })
                                .number_of_outgoing_edges += 1;
                        }
                        clause_stats = self.clause_stats.remove(&add.id);

                        minimum_lifetime = (clause_stats
                            .map(|stats| stats.first_used_at)
                            .unwrap_or(add.id)
                            - add.id) as usize;
                        is_important
                    }
                    Step::Import(import) => important_ids.remove(&import.imported_clause),
                    Step::Delete(_) => false,
                };
                return Ok(Some(StepInfo {
                    is_critical,
                    outgoing_edges: clause_stats
                        .as_ref()
                        .map(|stats| stats.number_of_outgoing_edges)
                        .unwrap_or_default(),
                    minimum_lifetime,
                    step: next_step_in_current_file,
                }));
            }
        }
        // If we get here then either there is no current file or the current file is exhausted.
        let Some((index_of_next_file, important_roots)) = self.important_roots.next() else {
            return Ok(None);
        };
        let filename = &self.palrup_files[*index_of_next_file];
        log::debug!("Reverse iterator is walking {} next", filename.display());

        self.current_file = Some((
            ReversePalrupIterator::for_file(filename)?,
            important_roots.clone(),
        ));
        self.next()
    }
}

impl ReverseDAGInfo {
    pub(crate) fn compute(
        palrup_files: &[PathBuf],
        id_of_unsat_clause: Id,
        first_derived_id: Id,
    ) -> Self {
        let solver_that_derived_clause =
            |clause_id: Id| -> usize { (clause_id as usize) % palrup_files.len() };

        // Contains all imports which extend the reverse DAG into another file AND that we have not looked at.
        // Maps from the solver ID to the list of such imports for that solver.
        let mut unprocessed_imports: FxHashMap<usize, FxHashSet<Id>> = FxHashMap::default();
        let solver_that_derived_unsat_clause = solver_that_derived_clause(id_of_unsat_clause);
        unprocessed_imports
            .entry(solver_that_derived_unsat_clause)
            .or_default()
            .insert(id_of_unsat_clause as Id);

        // For each solver, contains all clause IDs that will later be imported by another solver thread, causing them
        // to contribute to solving the problem. In other words, these clauses are critical even if they are not used
        // by the creator solver thread at all.
        let mut critical_clause_roots: FxHashMap<usize, FxHashSet<Id>> = Default::default();
        critical_clause_roots
            .entry(solver_that_derived_unsat_clause)
            .or_default()
            .insert(id_of_unsat_clause);

        for i in 0.. {
            // Find next solver thread that still has work to do.
            let Some(thread_id) = unprocessed_imports.keys().next().copied() else {
                log::debug!("No more work to do");
                break;
            };
            let unprocessed_imports_for_thread = unprocessed_imports.remove(&thread_id).unwrap();
            debug_assert!(
                !unprocessed_imports_for_thread.is_empty(),
                "Got empty work chunk?"
            );

            log::debug!(
                "Iteration {i}: Found {} unprocessed DAG roots for thread {}",
                unprocessed_imports_for_thread.len(),
                thread_id
            );

            let mut reverse_iterator =
                ReversePalrupIterator::for_file(&palrup_files[thread_id]).unwrap();
            let mut current_important_clauses: FxHashSet<Id> = unprocessed_imports_for_thread;
            loop {
                let Some(next) = reverse_iterator.next().unwrap() else {
                    break;
                };
                match &next {
                    Step::Add(add_step) => {
                        // This will only consider clauses critical that we have not looked at, or that derive an ancestor that
                        // we have not looked at.
                        let is_critical = current_important_clauses.remove(&add_step.id);
                        if is_critical {
                            for ancestor in &add_step.hints {
                                current_important_clauses.insert(*ancestor);
                            }
                        };
                    }
                    _ => {}
                }
            }

            let mut new_roots = 0;
            for imported_clause_that_is_important in current_important_clauses.drain() {
                // FIXME: The reverse iterator seems to miss the first clause in each file.
                if imported_clause_that_is_important
                    < first_derived_id + palrup_files.len() as isize
                {
                    // This clause comes from the problem definition
                    continue;
                }
                let imported_from = solver_that_derived_clause(imported_clause_that_is_important);
                debug_assert_ne!(
                    imported_from, thread_id,
                    "Clause {} was detected to be imported but comes from same thread",
                    imported_clause_that_is_important
                );
                if critical_clause_roots
                    .entry(imported_from)
                    .or_default()
                    .insert(imported_clause_that_is_important)
                {
                    // This clause is an important root AND we did not know about it previously
                    unprocessed_imports
                        .entry(imported_from)
                        .or_default()
                        .insert(imported_clause_that_is_important);
                    new_roots += 1;
                }
            }
            log::debug!(
                "Found {} new roots that need further processing...",
                new_roots
            );
        }

        log::info!(
            "Reverse DAG has {} roots from {} different threads",
            critical_clause_roots
                .values()
                .map(|clauses| clauses.len())
                .sum::<usize>(),
            critical_clause_roots.len()
        );

        Self {
            important_roots: critical_clause_roots,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::*;

    #[test]
    fn test_chunk_iterator() {
        let mut iterator: ChunkIterator<_, 5> =
            ChunkIterator::new(Cursor::new(b"opqrstuvwxyz")).unwrap();
        assert_eq!(iterator.next(vec![0; 5]).unwrap(), Some(b"vwxyz".to_vec()));
        assert_eq!(iterator.next(vec![0; 5]).unwrap(), Some(b"qrstu".to_vec()));
        assert_eq!(iterator.next(vec![0; 5]).unwrap(), Some(b"op".to_vec()));
        assert_eq!(iterator.next(vec![0; 5]).unwrap(), None);
    }
}
