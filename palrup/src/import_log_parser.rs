use regex::Regex;
use serde::Serialize;
use std::fs;
use std::mem;
use std::path::Path;

#[derive(Debug, Serialize)]
pub(crate) struct ImportStep {
    lrat_ids: Vec<usize>,
}

impl ImportStep {
    fn with_size(size: usize) -> Self {
        Self {
            lrat_ids: vec![0; size],
        }
    }
}

pub(crate) fn parse(mallob_logs: &str) -> Vec<ImportStep> {
    let re =
        Regex::new(r"I am ID\s+(\d+)\s+and im doing an import\.\s+My next lrat ID is\s+(\d+)\.$")
            .unwrap();

    // First pass: Find max id(nprocs * solvers per proc)
    let mut max_id = usize::MIN;
    for line in mallob_logs.lines() {
        if let Some(captures) = re.captures(line) {
            let id: usize = captures[1].parse().unwrap();
            max_id = id.max(max_id);
        }
    }

    println!("max id={max_id}");
    let mut steps = Vec::default();
    let mut current_step = ImportStep::with_size(max_id + 1);
    let mut count = 0;
    for line in mallob_logs.lines() {
        if let Some(captures) = re.captures(line) {
            let id: usize = captures[1].parse().unwrap();
            let lrat_id: usize = captures[2].parse().unwrap();
            current_step.lrat_ids[id] = lrat_id;

            if count == max_id {
                let mut next_step = ImportStep::with_size(max_id + 1);
                mem::swap(&mut current_step, &mut next_step);
                steps.push(next_step);
                count = 0;
            } else {
                count += 1;
            }
        }
    }
    log::info!("Found {} import epochs", steps.len());

    steps
}
