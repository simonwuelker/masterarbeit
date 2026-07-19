use std::collections::hash_map::Entry;

use rustc_hash::FxHashMap;
use serde::Serialize;

use crate::palrup::{ClauseAddition, ClauseImport, Id};

pub(crate) const TRACK_DERIVATIVES_UP_TO: u8 = 5;

#[derive(Default)]
pub(crate) struct Walker {
    /// For each import, stores the highest known derivation depth (so far) and the import generation that it
    /// was imported in.
    imports: FxHashMap<Id, u8>,
    /// For each non-import clause, stores the depth that it was derived from from each import.
    derivatives: FxHashMap<Id, FxHashMap<Id, u8>>,

    num_additions: usize,
    num_imports: usize,
    num_deletions: usize,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub(crate) struct UsageStatistics {
    pub import_depth: [usize; TRACK_DERIVATIVES_UP_TO as usize],
    pub unused_imports: Vec<Id>,
    pub num_additions: usize,
    pub num_imports: usize,
    pub num_deletions: usize,
}

impl Walker {
    pub(crate) fn add_clause(&mut self, clause: &ClauseAddition) {
        self.num_additions += 1;

        let mut derived_by_imports = FxHashMap::default();
        for depends_on in &clause.hints {
            if self.imports.contains_key(depends_on) {
                // This clause is an import! Lets mark it as having a derivative.
                derived_by_imports.entry(*depends_on).or_insert(1);
                continue;
            }

            if let Some(ancestor_imports) = self.derivatives.get(depends_on) {
                // One of the clauses that this clause was derived from was in turn derived from an import.
                for (ancestor_import, depth) in ancestor_imports {
                    if *depth == TRACK_DERIVATIVES_UP_TO - 1 {
                        continue;
                    }
                    let new_depth = if clause.is_unsat_clause() {
                        // This clause solved the instance - even if its not a particular
                        // deep import depth, treat it as if it was very useful.
                        TRACK_DERIVATIVES_UP_TO - 1
                    } else {
                        depth + 1
                    };
                    derived_by_imports
                        .entry(*ancestor_import)
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
        self.num_imports += 1;

        self.imports.insert(import.imported_clause, 0);
    }

    pub(crate) fn forget_clause(&mut self, id: Id) {
        self.num_deletions += 1;

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
        let mut unused_imports = vec![];
        for (id, depth) in self.imports {
            stats[depth as usize] = stats[depth as usize] + 1;
            if depth == 0 {
                unused_imports.push(id);
            }
        }

        UsageStatistics {
            import_depth: stats,
            unused_imports,
            num_additions: self.num_additions,
            num_deletions: self.num_deletions,
            num_imports: self.num_imports,
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
                unused_imports: vec![],
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
                unused_imports: vec![11, 32]
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
                unused_imports: vec![]
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
                unused_imports: vec![]
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
                unused_imports: vec![]
            }
        )
    }
}
