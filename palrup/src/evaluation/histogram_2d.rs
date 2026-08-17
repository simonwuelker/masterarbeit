use rustc_hash::FxHashMap;
use serde::Serialize;

use crate::evaluation::{histograms::Histogram, metrics::MetricSet};

#[derive(Debug, Serialize)]
pub(crate) struct Histogram2D {
    pub(crate) bucket_size: usize,
    pub(crate) inner_size: usize,
    pub(crate) buckets: FxHashMap<usize, Histogram>,
}

impl Histogram2D {
    fn new(bucket_size: usize, inner_size: usize) -> Self {
        Self {
            bucket_size,
            inner_size,
            buckets: Default::default(),
        }
    }

    fn add_sample(&mut self, value: usize, inner: usize) {
        let bucket = value / self.bucket_size;
        self.buckets
            .entry(bucket)
            .or_insert_with(|| Histogram::new(self.inner_size))
            .add_sample(inner);
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct Histogram2DSet {
    number_of_literals_over_clause_id: Histogram2D,
    lifetime_over_clause_id: Histogram2D,
    minimum_lifetime_over_clause_id: Histogram2D,
}

impl Default for Histogram2DSet {
    fn default() -> Self {
        Self {
            number_of_literals_over_clause_id: Histogram2D::new(1024, 1),
            lifetime_over_clause_id: Histogram2D::new(1024, 512),
            minimum_lifetime_over_clause_id: Histogram2D::new(1024, 512),
        }
    }
}
impl Histogram2DSet {
    pub(crate) fn add_sample(&mut self, metrics: MetricSet) {
        self.number_of_literals_over_clause_id
            .add_sample(metrics.id, metrics.number_of_literals);
        self.lifetime_over_clause_id
            .add_sample(metrics.id, metrics.lifetime);
        self.minimum_lifetime_over_clause_id
            .add_sample(metrics.id, metrics.minimum_lifetime);
    }
}
