use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::core::scene;
use crate::cprintln;

use super::{
    BatchAddArgs, find_node, parent_attr_for_node, read_and_parse_scene, write_or_dry_run,
};

pub(crate) fn exec_batch_add(args: &BatchAddArgs) -> Result<()> {
    let path = PathBuf::from(&args.scene);
    if !path.exists() {
        return Err(miette!("Scene file not found: {}", args.scene));
    }

    if args.nodes.is_empty() {
        return Err(miette!(
            "No nodes specified — use --node Name:Type[:Parent]"
        ));
    }

    let (mut source, mut data) = read_and_parse_scene(&path)?;

    for spec in &args.nodes {
        let parent_attr = if let Some(ref parent_name) = spec.parent {
            find_node(&data, parent_name)?;
            parent_attr_for_node(parent_name, &data)?
        } else {
            ".".to_string()
        };

        source = super::add_node::insert_node(
            &source,
            &data,
            &spec.name,
            &spec.node_type,
            &parent_attr,
        )?;
        data = scene::parse_scene(&source)?;
    }

    write_or_dry_run(&path, &source, args.dry_run)?;

    if !args.dry_run {
        cprintln!(
            "{} Added {} nodes to {}",
            "✓".green(),
            args.nodes.len().bold(),
            args.scene,
        );
    }

    Ok(())
}
