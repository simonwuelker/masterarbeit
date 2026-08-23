use crate::palrup::{PalrupIterator, Step};
use crate::PrintCommandArgs;

pub(crate) fn print_command(args: &PrintCommandArgs) -> anyhow::Result<()> {
    let iterator = PalrupIterator::for_file(&args.file)?;
    let mut did_start_printing = args.from.is_none();
    let mut did_end_printing = false;
    let take_until = args.to.unwrap_or(usize::MAX);
    for (index, entry) in iterator.enumerate() {
        match entry? {
            Step::Add(add) => {
                if !did_start_printing {
                    if let Some(from) = args.from {
                        if add.id as usize >= from {
                            did_start_printing = true;
                        }
                    }
                }
                if add.id as usize > take_until {
                    break;
                }
                if did_start_printing {
                    println!("{index:0>10?}: ADD {:?}", add.id);
                }
            }
            Step::Import(import) => {
                if did_start_printing {
                    println!("{index:0>10?}: IMPORT {:?}", import.imported_clause);
                }
            }
            Step::Delete(deletion) => {
                if did_start_printing {
                    println!("{index:0>10?}: DELETE {:?}", deletion.deleted_clauses);
                }
            }
        }
    }

    Ok(())
}
