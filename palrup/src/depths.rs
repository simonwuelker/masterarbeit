use std::path::Path;
use std::fs;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::palrup::{find_proof_files, ClauseAddition, Id, PalrupIterator, Step};

#[derive(Default)]
struct State {
    num_threads: usize,
    depth_map: FxHashMap<Id, usize>,
    smallest_derived_id: Option<Id>,
    // Maps from iterator index to the add step that was blocking it.
    blocked_iterators: FxHashMap<usize, ClauseAddition>,
    finished_iterators: FxHashSet<usize>,
    depths_count: Vec<usize>,
}

impl State {
    fn notify_depth(&mut self, depth: usize) {
        if self.depths_count.len() <= depth {
            self.depths_count.resize(depth + 1, 0);
        }
        self.depths_count[depth] += 1;
    }

    fn find_iterator_that_can_continue(&mut self) -> Option<usize> {
        let Some(smallest_derived_id) = self.smallest_derived_id else {
            // Clearly we haven't done any work yet, so take the first one.
            return Some(0);
        };

        if self.finished_iterators.len() == self.num_threads {
            // Every iterator has finished now.
            return None;
        }

        for index in 0..self.num_threads {
            if self.finished_iterators.contains(&index) {
                continue;
            }

            if let Some(blocked_on) = self.blocked_iterators.get(&index) {
                // See if this iterator was unblocked since it was inserted.
                let did_get_unblocked = blocked_on
                    .hints
                    .iter()
                    .filter(|id| **id >= smallest_derived_id)
                    .all(|id| self.depth_map.contains_key(&id));

                if did_get_unblocked {
                    // Great news, this is no longer blocked.
                    let correct_depth = blocked_on
                        .hints
                        .iter()
                        .filter(|id| **id >= smallest_derived_id)
                        .filter_map(|id| self.depth_map.get(&id).copied())
                        .max()
                        .unwrap_or_default() + 1;

                    self.depth_map.insert(blocked_on.id, correct_depth);
                    self.notify_depth(correct_depth);
                    self.blocked_iterators.remove(&index);
                    return Some(index);
                }
            } else {
                // We're not blocked. Nice.
                return Some(index);
            }
        }

        unreachable!()
    }
}

pub(crate) fn depths(proof_directory: impl AsRef<Path>) -> anyhow::Result<()> {
    let proof_files = find_proof_files(proof_directory)?;

    let mut iterators = Vec::with_capacity(proof_files.len());
    for proof_file in proof_files.iter() {
        iterators.push(PalrupIterator::for_file(proof_file)?);
    }

    let mut state = State {
        num_threads: proof_files.len(),
        depth_map: Default::default(),
        smallest_derived_id: Default::default(),
        blocked_iterators: Default::default(),
        finished_iterators: Default::default(),
        depths_count: Default::default(),
    };

    // Run an iterator until it needs an import that we do not yet have info for.
    while let Some(usable_iterator) = state.find_iterator_that_can_continue() {
        log::info!("Continuing with Nr. {usable_iterator}");
        let mut did_get_blocked = false;
        for step in &mut iterators[usable_iterator] {
            let step = step?;
            match step {
                Step::Add(add_step) => {
                    let smallest_derived_id = *state.smallest_derived_id.get_or_insert(add_step.id);

                    let missing_info_for_clause = add_step
                        .hints
                        .iter()
                        .filter(|id| **id >= smallest_derived_id)
                        .find(|id| !state.depth_map.contains_key(&id))
                        .copied();

                    if let Some(missing_info_for_clause) = missing_info_for_clause {
                        log::info!(
                            "Nr. {usable_iterator} got blocked on {} due to {} from Nr. {}",
                            add_step.id,
                            missing_info_for_clause,
                            missing_info_for_clause as usize % state.num_threads
                        );
                        state.blocked_iterators.insert(usable_iterator, add_step);
                        did_get_blocked = true;
                        break;
                    }

                    let max_depth_of_hints = add_step
                        .hints
                        .iter()
                        .filter_map(|id| state.depth_map.get(&id).copied())
                        .max()
                        .unwrap_or_default();
                    let depth_of_this_clause = max_depth_of_hints + 1;
                    state.depth_map.insert(add_step.id, depth_of_this_clause);
                    state.notify_depth(depth_of_this_clause);
                }
                _ => {}
            }
        }
        if !did_get_blocked {
            log::info!("Nr. {usable_iterator} finished.");
            state.finished_iterators.insert(usable_iterator);
        }
    }

    let result_path = "out.json";
    if fs::exists(&result_path)? {
        fs::remove_file(&result_path)?;
    }
    let outfile = fs::File::create(&result_path)?;
    serde_json::to_writer(outfile, &state.depths_count)?;

    Ok(())
}
