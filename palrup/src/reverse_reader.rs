use std::fs::File;
use std::mem;
use std::path::Path;
use std::{
    io::{self, BufReader, Read, Seek, SeekFrom},
    iter::Rev,
};

use crate::palrup;
use anyhow::{Context, Result};

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

const CHUNK_SIZE: usize = 1 << 20;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::*;

    #[test]
    fn test_chunk_iterator() {
        let mut iterator: ChunkIterator<_, 5> =
            ChunkIterator::new(Cursor::new(b"opqrstuvwxyz")).unwrap();
        let mut buffer = vec![0; 5];
        assert_eq!(
            iterator.next(&mut buffer).unwrap(),
            Some(b"vwxyz".as_slice())
        );
        assert_eq!(
            iterator.next(&mut buffer).unwrap(),
            Some(b"qrstu".as_slice())
        );
        assert_eq!(iterator.next(&mut buffer).unwrap(), Some(b"op".as_slice()));
        assert_eq!(iterator.next(&mut buffer).unwrap(), None);
    }
}
