use std::collections::{
    HashMap,
    hash_map::{Entry, OccupiedEntry},
};

use crate::palrup::{ClauseAddition, ClauseImport, Id};

const TRACK_DERIVATIVES_UP_TO: u8 = 5;

#[derive(Default)]
pub(crate) struct Walker {
    /// For each import, stores the highest known derivation depth (so far).
    imports: HashMap<Id, u8>,
    /// For each non-import clause, stores the depth that it was derived from from each import.
    derivatives: HashMap<Id, HashMap<Id, u8>>,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct UsageStatistics {
    import_depth: [usize; TRACK_DERIVATIVES_UP_TO as usize],
}

impl Walker {
    pub(crate) fn add_clause(&mut self, clause: &ClauseAddition) {
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
        self.imports.insert(import.imported_clause, 0);
    }

    pub(crate) fn forget_clause(&mut self, id: Id) {
        // We never forget imports
        let Entry::Occupied(occupied_entry) = self.derivatives.entry(id) else {
            return;
        };

        for (ancestor_import, depth) in occupied_entry.get() {
            let Some(previous_max_depth) = self.imports.get_mut(ancestor_import) else {
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
                let Some(previous_max_depth) = self.imports.get_mut(&ancestor_import) else {
                    unreachable!();
                };
                if depth > *previous_max_depth {
                    *previous_max_depth = depth;
                }
            }
        }

        let mut stats = [0; TRACK_DERIVATIVES_UP_TO as usize];
        for (key, depth) in self.imports {
            if depth == 1 && key == 3688 {
                println!("We think {key:?} is used once at least.");
            }
            stats[depth as usize] = stats[depth as usize] + 1;
        }

        UsageStatistics {
            import_depth: stats,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_sequence() {
        let mut stats = Walker::default().finalize();
        assert_eq!(
            stats,
            UsageStatistics {
                import_depth: [0; TRACK_DERIVATIVES_UP_TO as usize]
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
                import_depth: [2, 0, 0, 0, 0]
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
                import_depth: [0, 1, 1, 0, 0]
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
                import_depth: [0, 0, 0, 1, 0]
            }
        )
    }

    #[test]
    fn test_imports_with_many() {
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
                import_depth: [0, 0, 0, 1, 0]
            }
        )
    }
}
