use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct BucketStore<A> {
    bucket_size: usize,
    /// Per-thread, Per-Bucket list
    buckets: Vec<Vec<A>>,
}

pub(crate) trait Accumulator: Default + Clone + Serialize {
    type Value;

    fn add(&mut self, value: Self::Value);
}

#[derive(Clone, Default, Serialize)]
pub(crate) struct Sum(usize);

impl Accumulator for Sum {
    type Value = usize;

    fn add(&mut self, value: Self::Value) {
        self.0 += value;
    }
}

#[derive(Clone, Default, Serialize)]
#[serde(transparent)]
pub(crate) struct Average {
    #[serde(skip)]
    number_of_samples: usize,
    average: f64,
}

impl Accumulator for Average {
    type Value = f64;

    fn add(&mut self, value: Self::Value) {
        self.number_of_samples += 1;
        let number_of_samples = self.number_of_samples as f64;

        let delta = value - self.average;
        self.average += delta / number_of_samples;
    }
}

impl<A> BucketStore<A>
where
    A: Accumulator,
{
    pub(crate) fn new(number_of_proof_files: usize, bucket_size: usize) -> Self {
        Self {
            bucket_size,
            buckets: (0..number_of_proof_files).map(|_| Vec::default()).collect(),
        }
    }

    pub(crate) fn insert(&mut self, clause_id: usize, value: A::Value, number_of_proof_files: usize) {
        let thread_id = clause_id % number_of_proof_files;
        let index = clause_id / self.bucket_size;
        if self.buckets[thread_id].len() <= index {
            self.buckets[thread_id].resize(index + 1, Default::default());
        }
        self.buckets[thread_id][index].add(value);
    }
}
