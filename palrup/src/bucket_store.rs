use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct BucketStore<A> {
    bucket_size: usize,
    /// Per-thread, Per-Bucket list
    buckets: Vec<Vec<A>>,
}

trait Accumulator: Default + Clone + Serialize {
    fn add(&mut self, value: usize);
}

#[derive(Clone, Default, Serialize)]
pub(crate) struct Sum(usize);

impl Accumulator for Sum {
    fn add(&mut self, value: usize) {
        self.0 += value;
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

    pub(crate) fn insert(&mut self, clause_id: usize, value: usize, number_of_proof_files: usize) {
        let thread_id = clause_id % number_of_proof_files;
        let index = clause_id / self.bucket_size;
        if self.buckets[thread_id].len() <= index {
            self.buckets[thread_id].resize(index + 1, Default::default());
        }
        self.buckets[thread_id][index].add(value);
    }
}
