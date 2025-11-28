use anyhow::Result;

use crate::ExitStatus;
use crate::args::SymbolsCommand;

pub(crate) fn symbols(_args: SymbolsCommand) -> Result<ExitStatus> {
    println!("Running symbol analysis...");
    Ok(ExitStatus::Success)
}
