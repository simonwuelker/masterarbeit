use anyhow::{Context, Result, anyhow};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

fn read_var_id<R>(mut reader: R) -> Result<Id>
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
fn read_var_int<R>(mut reader: R) -> Result<usize>
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

type Id = isize;

#[derive(Clone, Debug)]
struct ClauseAddition {
    id: Id,
    literals: Vec<usize>,
    hints: Vec<usize>,
}

#[derive(Clone, Debug)]
struct ClauseDeletion {
    deleted_clauses: Vec<Id>,
}

#[derive(Clone, Debug)]
enum Step {
    Add(ClauseAddition),
    Delete(ClauseDeletion),
}

impl ClauseAddition {
    fn read<R>(mut reader: R) -> Result<Self>
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
        let mut next = read_var_int(&mut reader)?;
        while next != 0 {
            hints.push(next);
            next = read_var_int(&mut reader)?;
        }

        Ok(ClauseAddition {
            id,
            literals,
            hints,
        })
    }
}

impl ClauseDeletion {
    fn read<R>(mut reader: R) -> Result<Self>
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

impl Step {
    fn read<R>(mut reader: R) -> Result<Self>
    where
        R: Read,
    {
        let mut buffer = [0];
        reader.read_exact(&mut buffer)?;

        match buffer[0] {
            b'a' => ClauseAddition::read(&mut reader).map(Self::Add),
            b'd' => ClauseDeletion::read(&mut reader).map(Self::Delete),
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

fn parse_palrup_file<P>(path: P) -> Result<()>
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    println!("Parse PalRUP file: {}", path.display());
    let file = File::open(&path)
        .with_context(|| format!("Failed to read proof from {}", path.display()))?;
    let mut reader = BufReader::new(file);

    let mut i = 0;
    while let Ok(step) = Step::read(&mut reader) {
        match step {
            Step::Add(add) => println!("Add clause {:?}", add.id),
            Step::Delete(delete) => println!("Delete clauses {:?}", delete.deleted_clauses),
        }
        i += 1;
        if i > 10 {
            break;
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let dir = PathBuf::from("/home/alaska/masterarbeit/mallob/proofs/proof#442/");

    // Walk dir for solver processes
    for entry in fs::read_dir(dir)? {
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
                            eprintln!(
                                "Found unexpected directory {:?} in proof directory ({:?})",
                                entry.file_name(),
                                solver_thread_directory.path().display()
                            );
                        } else {
                            parse_palrup_file(entry.path())?;
                        }
                    }
                } else {
                    eprintln!(
                        "Found unexpected file {:?} in proof directory",
                        solver_thread_directory.file_name()
                    );
                }
            }
        } else {
            eprintln!(
                "Found unexpected file {:?} in proof directory",
                solver_process_directory.file_name()
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // #[test]
    // fn parse_clause() {
    //     let source = &[0x61, 0xb4, 0x0e, 0x0b, 0x75, 0xf4, 0x02, 0xf8, 0x02, 0x80, 0x03, 0x82, 0x03, 0x86, 0x03, 0x90]
    //     61b4 0e0b 75f4 02f8 0280 0382 0386 0390  a...u...........
    //     00000010: 0300 f209 c208 8803 f404 b40c ee08 cc0a  ................
    //     00000020: fa0a be08 ce0b f008 8c07 9009 b206 f606  ................
    //     00000030: f002 ec03 00
    // }
}
