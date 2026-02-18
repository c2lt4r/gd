use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::{MeshState, ShadingMode};

use super::{AutoSmoothArgs, OutputFormat, ShadingArgs, project_root, run_eval};

pub fn cmd_shade_smooth(args: &ShadingArgs) -> Result<()> {
    apply_shading(ShadingMode::Smooth, args.part.as_deref(), args.all, &args.format)
}

pub fn cmd_shade_flat(args: &ShadingArgs) -> Result<()> {
    apply_shading(ShadingMode::Flat, args.part.as_deref(), args.all, &args.format)
}

pub fn cmd_auto_smooth(args: &AutoSmoothArgs) -> Result<()> {
    apply_shading(
        ShadingMode::AutoSmooth(args.angle),
        args.part.as_deref(),
        args.all,
        &args.format,
    )
}

fn apply_shading(
    mode: ShadingMode,
    part: Option<&str>,
    all: bool,
    format: &OutputFormat,
) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let label = match mode {
        ShadingMode::Smooth => "smooth".to_string(),
        ShadingMode::Flat => "flat".to_string(),
        ShadingMode::AutoSmooth(a) => format!("auto-smooth ({a}deg)"),
    };

    let parts: Vec<String> = if all {
        state.parts.keys().cloned().collect()
    } else {
        let name = part.unwrap_or(&state.active).to_string();
        vec![name]
    };

    for name in &parts {
        let p = state
            .parts
            .get_mut(name)
            .ok_or_else(|| miette::miette!("Part '{name}' not found"))?;
        p.shading = mode;
    }

    state.save(&root)?;

    // Re-push affected parts (normals change with shading mode)
    for name in &parts {
        let push = state.generate_push_script(name)?;
        let _ = run_eval(&push)?;
    }

    let result = serde_json::json!({
        "shading": label,
        "parts": parts,
    });

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            println!(
                "Shading set to {} on {} part(s)",
                label.cyan(),
                parts.len().to_string().green().bold()
            );
        }
    }

    Ok(())
}
