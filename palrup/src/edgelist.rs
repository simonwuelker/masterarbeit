use anyhow::Result;
use std::io::Write;

use crate::palrup::Id;

pub(crate) struct Writer<W: Write> {
    writer: W,
}

impl<W: Write> Writer<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub(crate) fn add_connection(&mut self, from: Id, to: Id) -> Result<()> {
        writeln!(self.writer, "{from}\t{to}")?;
        Ok(())
    }
}
