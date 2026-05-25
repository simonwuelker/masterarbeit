use std::fs::File;
use std::io::{self, BufReader, ErrorKind, Read};
use std::path::Path;

use anyhow::{Context, Result};

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

pub(crate) type Id = isize;

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
    pub(crate) imported_clauses: Vec<Id>,
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
        let mut imported_clauses = Vec::new();
        let mut next = read_var_id(&mut reader)?;
        while next != 0 {
            imported_clauses.push(next);
            next = read_var_id(&mut reader)?;
        }

        Ok(ClauseImport { imported_clauses })
    }
}

impl Step {
    fn read<R>(mut reader: R) -> io::Result<Self>
    where
        R: Read,
    {
        let mut buffer = [0];
        reader.read_exact(&mut buffer)?;

        match buffer[0] {
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
        }
    }
}

pub(crate) struct PalrupIterator<R: Read> {
    reader: R,
}

impl PalrupIterator<BufReader<File>> {
    pub(crate) fn for_file<P: AsRef<Path>>(file: P) -> Result<Self> {
        let file = File::open(&file)
            .with_context(|| format!("Failed to read proof from {}", file.as_ref().display()))?;
        let reader = BufReader::new(file);

        Ok(Self { reader })
    }
}

impl<R: Read> Iterator for PalrupIterator<R> {
    type Item = Result<Step>;

    fn next(&mut self) -> Option<Self::Item> {
        match Step::read(&mut self.reader) {
            Ok(step) => Some(Ok(step)),
            Err(error) if error.kind() == ErrorKind::UnexpectedEof => None,
            Err(other) => Some(Err(other.into())),
        }
    }
}
