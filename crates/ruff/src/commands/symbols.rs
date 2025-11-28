use anyhow::Result;
use std::sync::Arc;

use ruff_db::system::{OsSystem, SystemPathBuf};
use ruff_python_ast::name::Name;
use ty_project::{ProjectDatabase, ProjectMetadata};

use crate::{
    ExitStatus,
    args::{ConfigArguments, SymbolsArgs},
    resolve::resolve,
};

pub(crate) fn symbols(
    _cli: &SymbolsArgs,
    config_arguments: &ConfigArguments,
) -> Result<ExitStatus> {
    let pyproject_config = resolve(config_arguments, None)?;
    let workspace_root =
        SystemPathBuf::from_path_buf(pyproject_config.settings.file_resolver.project_root.clone())
            .expect("project root is not a valid path");

    let project_metadata = ProjectMetadata::new(Name::new("ruff"), workspace_root.clone());
    let system = OsSystem::new(&workspace_root);
    let db = ProjectDatabase::new(project_metadata, system)?;

    let symbols = ty_ide::workspace_symbols(&db, "def");

    for symbol in symbols {
        println!(
            "{:?} {} at {}:{}",
            symbol.symbol.kind,
            symbol.symbol.name,
            symbol.file.path(&db).to_string(),
            symbol.symbol.name_range.start().to_u32()
        );
    }

    Ok(ExitStatus::Success)
}
