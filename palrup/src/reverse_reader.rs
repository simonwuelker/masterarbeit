use std::io::{self, Read, Seek, SeekFrom};

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

    pub(crate) fn next<'a>(&mut self, buffer: &'a mut [u8]) -> io::Result<Option<&'a [u8]>> {
        debug_assert_eq!(buffer.len(), N);

        if self.current_position == 0 {
            return Ok(None);
        }
        if self.current_position < N as u64 {
            let position = self.current_position as usize;
            self.current_position = 0;
            self.reader.rewind()?;
            self.reader.read_exact(&mut buffer[..position])?;
            return Ok(Some(&buffer[..position]));
        }

        self.current_position -= N as u64;
        self.reader.seek(SeekFrom::Start(self.current_position))?;

        self.reader.read_exact(buffer)?;

        Ok(Some(buffer))
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
