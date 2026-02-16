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

    if args.format.as_deref() == Some("json") {
        let json = serde_json::to_string_pretty(&data)
            .map_err(|e| miette!("Failed to serialize resource data: {e}"))?;
        println!("{json}");
    } else {
        print_resource_human(&data, &args.file);
    }
    Ok(())
}

fn print_resource_human(data: &scene::ResourceData, file: &str) {
    use owo_colors::OwoColorize;
    println!("{}", file.bold());
    println!("  {}: {}", "Type".dimmed(), data.type_name.green());

    if !data.ext_resources.is_empty() {
        println!("\n  {}:", "External Resources".bold());
        for r in &data.ext_resources {
            println!("    {} {} ({})", r.id.dimmed(), r.path.cyan(), r.type_name);
        }
    }

    if !data.sub_resources.is_empty() {
        println!("\n  {}:", "Sub Resources".bold());
        for r in &data.sub_resources {
            println!("    {} ({})", r.id.dimmed(), r.type_name);
            for (k, v) in &r.properties {
                println!("      {} = {}", k, v.dimmed());
            }
        }
    }

    if !data.properties.is_empty() {
        println!("\n  {}:", "Properties".bold());
        for (k, v) in &data.properties {
            println!("    {} = {}", k, v.dimmed());
        }
    }
}
