use std::collections::{
    HashMap,
    hash_map::{Entry, OccupiedEntry},
};

use serde::Serialize;

use crate::palrup::{ClauseAddition, ClauseImport, Id};

pub(crate) const TRACK_DERIVATIVES_UP_TO: u8 = 5;

#[derive(Default)]
pub(crate) struct Walker {
    /// For each import, stores the highest known derivation depth (so far) and the import generation that it
    /// was imported in.
    imports: HashMap<Id, (u8, u32)>,
    /// For each non-import clause, stores the depth that it was derived from from each import.
    derivatives: HashMap<Id, HashMap<Id, u8>>,
    last_was_import: bool,
    current_import_generation: u32,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub(crate) struct UsageStatistics {
    pub import_depth: [usize; TRACK_DERIVATIVES_UP_TO as usize],
    pub unused_imports_per_generation: Vec<usize>,
}

impl Walker {
    pub(crate) fn add_clause(&mut self, clause: &ClauseAddition) {
        if self.last_was_import {
            self.last_was_import = false;
            self.current_import_generation += 1;
        }

        let mut derived_by_imports = HashMap::new();
        for depends_on in &clause.hints {
            if self.imports.contains_key(depends_on) {
                derived_by_imports.entry(*depends_on).or_insert(1);
                continue;
            }
            if let Some(derived_by) = self.derivatives.get(depends_on) {
                for (derived_from, depth) in derived_by {
                    if *depth == TRACK_DERIVATIVES_UP_TO - 1 {
                        continue;
                    }
                    let new_depth = *depth + 1;
                    derived_by_imports
                        .entry(*derived_from)
                        .and_modify(|current| *current = (*current).max(new_depth))
                        .or_insert(new_depth);
                }
            }
        }

        if !derived_by_imports.is_empty() {
            let had_previous = self
                .derivatives
                .insert(clause.id, derived_by_imports)
                .is_some();
            debug_assert!(!had_previous, "Clause defined twice?");
        }
    }

    pub(crate) fn import_clause(&mut self, import: ClauseImport) {
        self.last_was_import = true;
        self.imports
            .insert(import.imported_clause, (0, self.current_import_generation));
    }

    pub(crate) fn forget_clause(&mut self, id: Id) {
        if self.last_was_import {
            self.last_was_import = false;
            self.current_import_generation += 1;
        }

        // We never forget imports
        let Entry::Occupied(occupied_entry) = self.derivatives.entry(id) else {
            return;
        };

        for (ancestor_import, depth) in occupied_entry.get() {
            let Some((previous_max_depth, _)) = self.imports.get_mut(ancestor_import) else {
                unreachable!();
            };
            if *depth > *previous_max_depth {
                *previous_max_depth = *depth;
            }
        }

        occupied_entry.remove();
    }

    pub(crate) fn finalize(mut self) -> UsageStatistics {
        for (_, info) in self.derivatives {
            for (ancestor_import, depth) in info {
                let Some((previous_max_depth, _)) = self.imports.get_mut(&ancestor_import) else {
                    unreachable!();
                };
                if depth > *previous_max_depth {
                    *previous_max_depth = depth;
                }
            }
        }

        let mut stats = [0; TRACK_DERIVATIVES_UP_TO as usize];
        let mut unused_imports_per_generation =
            vec![0; self.current_import_generation as usize + 1];
        for (_, (depth, generation)) in self.imports {
            stats[depth as usize] = stats[depth as usize] + 1;
            if depth == 0 {
                unused_imports_per_generation[generation as usize] += 1;
            }
        }

        UsageStatistics {
            import_depth: stats,
            unused_imports_per_generation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_sequence() {
        let stats = Walker::default().finalize();
        assert_eq!(
            stats,
            UsageStatistics {
                import_depth: [0; TRACK_DERIVATIVES_UP_TO as usize],
                unused_imports_per_generation: vec![0],
            }
        )
    }

    #[test]
    fn test_only_imports() {
        let mut walker = Walker::default();
        walker.import_clause(ClauseImport {
            imported_clause: 11,
        });
        walker.import_clause(ClauseImport {
            imported_clause: 32,
        });
        assert_eq!(
            walker.finalize(),
            UsageStatistics {
                import_depth: [2, 0, 0, 0, 0],
                unused_imports_per_generation: vec![2]
            }
        )
    }

    #[test]
    fn test_transitive_derivations() {
        let mut walker = Walker::default();
        walker.import_clause(ClauseImport { imported_clause: 0 });
        walker.add_clause(&ClauseAddition {
            id: 1,
            literals: vec![],
            hints: vec![0],
        });
        walker.import_clause(ClauseImport { imported_clause: 2 });
        walker.add_clause(&ClauseAddition {
            id: 3,
            literals: vec![],
            hints: vec![1, 2],
        });

        assert_eq!(
            walker.finalize(),
            UsageStatistics {
                import_depth: [0, 1, 1, 0, 0],
                unused_imports_per_generation: vec![0, 0, 0]
            }
        )
    }

    #[test]
    fn test_precedence_when_same_import_is_used_multiple_times() {
        let mut walker = Walker::default();
        walker.import_clause(ClauseImport { imported_clause: 0 });
        walker.add_clause(&ClauseAddition {
            id: 1,
            literals: vec![],
            hints: vec![0],
        });
        walker.add_clause(&ClauseAddition {
            id: 2,
            literals: vec![],
            hints: vec![1],
        });

        // This import depends on the import 0 through both of its dependents. We should
        // only store the longer chain.
        walker.add_clause(&ClauseAddition {
            id: 3,
            literals: vec![],
            hints: vec![1, 2],
        });

        assert_eq!(
            walker.finalize(),
            UsageStatistics {
                import_depth: [0, 0, 0, 1, 0],
                unused_imports_per_generation: vec![0, 0]
            }
        )
    }

    #[test]
    fn test_imports_use_longer_chain() {
        let mut walker = Walker::default();
        walker.import_clause(ClauseImport { imported_clause: 0 });
        walker.add_clause(&ClauseAddition {
            id: 1,
            literals: vec![],
            hints: vec![0],
        });
        walker.add_clause(&ClauseAddition {
            id: 2,
            literals: vec![],
            hints: vec![1],
        });

        // This import depends on the import 0 through both of its dependents. We should
        // only store the longer chain.
        walker.add_clause(&ClauseAddition {
            id: 3,
            literals: vec![],
            hints: vec![1, 2],
        });

        assert_eq!(
            walker.finalize(),
            UsageStatistics {
                import_depth: [0, 0, 0, 1, 0],
                unused_imports_per_generation: vec![0, 0]
            }
        )
    }
}
