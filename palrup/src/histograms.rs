use rustc_hash::FxHashMap;
use serde::Serialize;

use crate::metrics::MetricSet;

#[derive(Debug, Serialize)]
pub(crate) struct HistogramSet {
    pub(crate) incoming_edges: Histogram,
    pub(crate) outgoing_edges: Histogram,
    pub(crate) number_of_literals: Histogram,
    pub(crate) id: Histogram,
    pub(crate) lifetime: Histogram,
}

impl Default for HistogramSet {
    fn default() -> Self {
        Self {
            incoming_edges: Histogram::new(1),
            outgoing_edges: Histogram::new(1),
            number_of_literals: Histogram::new(1),
            id: Histogram::new(4096),
            lifetime: Histogram::new(128),
        }
    }
}
#[derive(Debug, Serialize)]
pub struct Histogram {
    pub(crate) bucket_size: usize,
    pub(crate) buckets: FxHashMap<usize, usize>,
}

impl Histogram {
    fn new(bucket_size: usize) -> Self {
        Self {
            bucket_size,
            buckets: Default::default(),
        }
    }
    fn add_sample(&mut self, value: usize) {
        let bucket = value / self.bucket_size;
        *self.buckets.entry(bucket).or_default() += 1;
    }
}

impl HistogramSet {
    pub(crate) fn add_sample(&mut self, sample: MetricSet) {
        self.incoming_edges.add_sample(sample.incoming_edges);
        self.outgoing_edges.add_sample(sample.outgoing_edges);
        self.number_of_literals
            .add_sample(sample.number_of_literals);
        self.id.add_sample(sample.id);
        self.lifetime.add_sample(sample.lifetime);
    }
}
