use std::path::PathBuf;

use miette::{Result, miette};

use crate::core::scene;

use super::InfoArgs;

pub(crate) fn exec_info(args: &InfoArgs) -> Result<()> {
    let path = PathBuf::from(&args.file);
    if !path.exists() {
        return Err(miette!("File not found: {}", args.file));
    }

    let data = scene::parse_tres_file(&path)?;
    let json = serde_json::to_string_pretty(&data)
        .map_err(|e| miette!("Failed to serialize resource data: {e}"))?;

    println!("{json}");
    Ok(())
}
