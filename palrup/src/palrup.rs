use std::fs;
use std::fs::File;
use std::io::{self, BufReader, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub(crate) type Id = isize;

fn read_var_id<R>(mut reader: R) -> io::Result<Id>
where
    R: Read,
{
    let x = read_var_int(&mut reader)?;
    // last bit encodes sign
    let id = if x % 2 == 0 {
        (x / 2) as isize
    } else {
        -1 * (x / 2) as isize
    };

    Ok(id)
}
fn read_var_int<R>(mut reader: R) -> io::Result<usize>
where
    R: Read,
{
    let mut current = 0;
    let mut buffer = [0];
    reader.read_exact(&mut buffer)?;

    let mut offset = 0;
    loop {
        current |= ((buffer[0] & 0x7F) as usize) << offset;
        if buffer[0] & !0x7F != 0 {
            offset += 7;
            reader.read_exact(&mut buffer)?;
            continue;
        }
        break;
    }

    Ok(current)
}

fn write_var_int<W: Write>(mut writer: W, mut value: usize) -> io::Result<()> {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            // More bytes will follow, set continuation bit
            byte |= 0x80;
            writer.write_all(&[byte])?;
        } else {
            // Last byte
            writer.write_all(&[byte])?;
            break;
        }
    }
    Ok(())
}

fn write_var_id<W: Write>(writer: W, id: Id) -> io::Result<()> {
    let x: usize = if id >= 0 {
        (id as usize) * 2
    } else {
        ((-id) as usize) * 2 + 1
    };
    write_var_int(writer, x)
}

#[derive(Clone, Debug)]
pub(crate) struct ClauseAddition {
    pub(crate) id: Id,
    pub(crate) literals: Vec<usize>,
    pub(crate) hints: Vec<Id>,
}

#[derive(Clone, Debug)]
pub(crate) struct ClauseDeletion {
    pub(crate) deleted_clauses: Vec<Id>,
}

#[derive(Clone, Debug)]
pub(crate) struct ClauseImport {
    pub(crate) imported_clause: Id,
}

#[derive(Clone, Debug)]
pub(crate) enum Step {
    Add(ClauseAddition),
    Delete(ClauseDeletion),
    Import(ClauseImport),
}

impl ClauseAddition {
    fn read<R>(mut reader: R) -> io::Result<Self>
    where
        R: Read,
    {
        let id = read_var_id(&mut reader)?;

        // Read clause literals
        let mut literals = Vec::new();
        let mut next = read_var_int(&mut reader)?;
        while next != 0 {
            literals.push(next);
            next = read_var_int(&mut reader)?;
        }

        // Read hints
        let mut hints = Vec::new();
        let mut next = read_var_id(&mut reader)?;
        while next != 0 {
            hints.push(next);
            next = read_var_id(&mut reader)?;
        }

        Ok(ClauseAddition {
            id,
            literals,
            hints,
        })
    }

    pub(crate) fn is_unsat_clause(&self) -> bool {
        self.literals.is_empty()
    }
}

impl ClauseDeletion {
    fn read<R>(mut reader: R) -> io::Result<Self>
    where
        R: Read,
    {
        let mut deleted_clauses = Vec::new();
        let mut next = read_var_id(&mut reader)?;
        while next != 0 {
            deleted_clauses.push(next);
            next = read_var_id(&mut reader)?;
        }

        Ok(ClauseDeletion { deleted_clauses })
    }
}

impl ClauseImport {
    fn read<R>(mut reader: R) -> io::Result<Self>
    where
        R: Read,
    {
        let imported_clause = read_var_id(&mut reader)?;

        // Read clause literals
        let mut next = read_var_int(&mut reader)?;
        while next != 0 {
            next = read_var_int(&mut reader)?;
        }

        Ok(ClauseImport { imported_clause })
    }
}

impl Step {
    fn read<R>(mut reader: R) -> Option<io::Result<Self>>
    where
        R: Read,
    {
        let mut buffer = [0];
        if let Err(e) = reader.read_exact(&mut buffer) {
            if e.kind() == ErrorKind::UnexpectedEof {
                // Thats okay, we've reached the end of the file.
                return None;
            } else {
                return Some(Err(e));
            }
        }

        let step = match buffer[0] {
            b'a' => ClauseAddition::read(&mut reader).map(Self::Add),
            b'd' => ClauseDeletion::read(&mut reader).map(Self::Delete),
            b'i' => ClauseImport::read(&mut reader).map(Self::Import),
            other => {
                panic!(
                    "Unknown step type: {:0>2x} ({:?})",
                    other,
                    char::from_u32(other as u32)
                )
            }
        };
        if let Err(e) = &step {
            log::error!("Encountered error while parsing: {e:?}");
            if e.kind() == ErrorKind::UnexpectedEof {
                log::warn!("FIXME: Ignoring garbage data near EOF");
                return None;
            }
        }
        Some(step)
    }
}

pub(crate) struct PalrupIterator<R: Read> {
    reader: R,
}

impl<R: Read> PalrupIterator<R> {
    pub(crate) fn new(reader: R) -> Self {
        Self { reader }
    }
}

impl PalrupIterator<BufReader<File>> {
    pub(crate) fn for_file<P: AsRef<Path>>(file: P) -> Result<Self> {
        let file = File::open(&file)
            .with_context(|| format!("Failed to read proof from {}", file.as_ref().display()))?;
        let reader = BufReader::new(file);

        Ok(Self::new(reader))
    }
}

impl<R: Read> Iterator for PalrupIterator<R> {
    type Item = Result<Step>;

    fn next(&mut self) -> Option<Self::Item> {
        match Step::read(&mut self.reader)? {
            Ok(step) => Some(Ok(step)),
            Err(other) => Some(Err(other.into())),
        }
    }
}

pub(crate) fn find_proof_files<P: AsRef<Path>>(proof_directory: P) -> io::Result<Vec<PathBuf>> {
    let mut proof_files = Vec::new();

    for entry in fs::read_dir(&proof_directory)? {
        let solver_process_directory = entry?;
        if solver_process_directory.file_type()?.is_dir() {
            // Walk dir for solver threads
            for entry in fs::read_dir(solver_process_directory.path())? {
                let solver_thread_directory = entry?;
                if solver_thread_directory.file_type()?.is_dir() {
                    // Walk proof files
                    for entry in fs::read_dir(solver_thread_directory.path())? {
                        let entry = entry?;
                        if entry.file_type()?.is_dir() {
                            log::warn!(
                                "Found unexpected directory {:?} in proof directory ({:?})",
                                entry.file_name(),
                                solver_thread_directory.path().display()
                            );
                        } else {
                            proof_files.push(entry.path());
                        }
                    }
                } else {
                    log::warn!(
                        "Found unexpected file {:?} in proof directory",
                        solver_thread_directory.file_name()
                    );
                }
            }
        } else {
            log::warn!(
                "Found unexpected file {:?} in proof directory",
                solver_process_directory.file_name()
            );
        }
    }
    proof_files.sort_unstable();

    Ok(proof_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn read_write_var_integer() {
        fn assert_for_value(value: usize) {
            let mut buffer: Vec<u8> = vec![];
            write_var_int(&mut buffer, value).unwrap();
            assert_eq!(read_var_int(io::Cursor::new(buffer)).unwrap(), value);
        }

        assert_for_value(0);
        assert_for_value(42);
        assert_for_value(usize::MAX);
        assert_for_value(usize::MAX / 2);
        assert_for_value(123456);
    }

    #[test]
    fn read_write_var_id() {
        fn assert_for_value(value: Id) {
            let mut buffer: Vec<u8> = vec![];
            write_var_id(&mut buffer, value).unwrap();
            assert_eq!(read_var_id(io::Cursor::new(buffer)).unwrap(), value);
        }

        assert_for_value(0);
        assert_for_value(-1);
        assert_for_value(42);
        assert_for_value(Id::MAX);
        assert_for_value(Id::MAX / 2);
        assert_for_value(Id::MIN / 2);
        assert_for_value(123456);
    }
}
