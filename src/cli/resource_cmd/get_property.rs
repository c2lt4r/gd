use std::path::PathBuf;

use miette::{Result, miette};

use super::{GetPropertyArgs, read_and_parse_resource};

pub(crate) fn exec_get_property(args: &GetPropertyArgs) -> Result<()> {
    let path = PathBuf::from(&args.file);
    if !path.exists() {
        return Err(miette!("File not found: {}", args.file));
    }

    let (_source, data) = read_and_parse_resource(&path)?;

    let value = data
        .properties
        .iter()
        .find(|(k, _)| k == &args.key)
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| miette!("Property '{}' not found in resource", args.key))?;

    println!("{value}");
    Ok(())
}
