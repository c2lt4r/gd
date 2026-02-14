use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::args::{
    LiveCreateNodeArgs, LiveDuplicateArgs, LiveInstantiateArgs, LiveNodeCallArgs, LiveNodePropArgs,
    LivePathArgs, LivePropResArgs, LiveRemoveKeepArgs, LiveRemoveNodeArgs, LiveReparentArgs,
    LiveRestoreArgs, LiveSetRootArgs, OutputFormat,
};
use super::{daemon_cmd, ensure_binary_debug};

// ── One-shot: live editing ──────────────────────────────────────────

pub(crate) fn cmd_live_set_root(args: &LiveSetRootArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
        "debug_live_set_root",
        serde_json::json!({"scene_path": args.path, "scene_file": args.file}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "root": args.path,
                    "file": args.file,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} live root to {} {}",
                "Set".green(),
                args.path.cyan(),
                format!("({})", args.file).dimmed(),
            );
        }
    }
    Ok(())
}

pub(crate) fn cmd_live_create_node(args: &LiveCreateNodeArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
        "debug_live_create_node",
        serde_json::json!({
            "parent": args.parent,
            "class": args.class_name,
            "name": args.name,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "created": true,
                    "name": args.name,
                    "class": args.class_name,
                    "parent": args.parent,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {} {}",
                "Created".green(),
                args.name.cyan(),
                format!("({})", args.class_name).dimmed(),
            );
        }
    }
    Ok(())
}

pub(crate) fn cmd_live_instantiate(args: &LiveInstantiateArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
        "debug_live_instantiate_node",
        serde_json::json!({
            "parent": args.parent,
            "scene": args.scene,
            "name": args.name,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "instantiated": true,
                    "name": args.name,
                    "scene": args.scene,
                    "parent": args.parent,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {} {}",
                "Instantiated".green(),
                args.name.cyan(),
                format!("({})", args.scene).dimmed(),
            );
        }
    }
    Ok(())
}

pub(crate) fn cmd_live_remove_node(args: &LiveRemoveNodeArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
        "debug_live_remove_node",
        serde_json::json!({"path": args.path}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "removed": true,
                    "path": args.path,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!("{} {}", "Removed".green(), args.path.cyan());
        }
    }
    Ok(())
}

pub(crate) fn cmd_live_duplicate(args: &LiveDuplicateArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
        "debug_live_duplicate_node",
        serde_json::json!({"path": args.path, "new_name": args.name}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "duplicated": true,
                    "source": args.path,
                    "name": args.name,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {} as {}",
                "Duplicated".green(),
                args.path.cyan(),
                args.name.cyan(),
            );
        }
    }
    Ok(())
}

pub(crate) fn cmd_live_reparent(args: &LiveReparentArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
        "debug_live_reparent_node",
        serde_json::json!({
            "path": args.path,
            "new_parent": args.new_parent,
            "new_name": args.name,
            "pos": args.pos,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "reparented": true,
                    "path": args.path,
                    "new_parent": args.new_parent,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {} to {}",
                "Reparented".green(),
                args.path.cyan(),
                args.new_parent.cyan(),
            );
        }
    }
    Ok(())
}

pub(crate) fn cmd_live_node_prop(args: &LiveNodePropArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_value: serde_json::Value = serde_json::from_str(&args.value)
        .unwrap_or_else(|_| serde_json::Value::String(args.value.clone()));

    daemon_cmd(
        "debug_live_node_prop",
        serde_json::json!({
            "id": args.id,
            "property": args.property,
            "value": json_value,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": args.id,
                    "property": args.property,
                    "value": json_value,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {}.{} = {}",
                "Set".green(),
                format!("[{}]", args.id).dimmed(),
                args.property.cyan(),
                args.value.green(),
            );
        }
    }
    Ok(())
}

pub(crate) fn cmd_live_node_call(args: &LiveNodeCallArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_args: serde_json::Value =
        serde_json::from_str(&args.args).unwrap_or_else(|_| serde_json::json!([]));

    daemon_cmd(
        "debug_live_node_call",
        serde_json::json!({
            "id": args.id,
            "method": args.method,
            "args": json_args,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": args.id,
                    "method": args.method,
                    "args": json_args,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {}.{}({})",
                "Called".green(),
                format!("[{}]", args.id).dimmed(),
                args.method.cyan(),
                args.args.dimmed(),
            );
        }
    }
    Ok(())
}

// ── Live editing: resource operations (binary protocol) ─────────────

pub(crate) fn cmd_live_node_path(args: &LivePathArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
        "debug_live_node_path",
        serde_json::json!({"path": args.path, "id": args.id}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "path": args.path,
                    "id": args.id,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} for {}",
                "Live node path set".green(),
                format!("[{}]", args.id).dimmed(),
            );
        }
    }
    Ok(())
}

pub(crate) fn cmd_live_res_path(args: &LivePathArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
        "debug_live_res_path",
        serde_json::json!({"path": args.path, "id": args.id}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "path": args.path,
                    "id": args.id,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} for {}",
                "Live resource path set".green(),
                format!("[{}]", args.id).dimmed(),
            );
        }
    }
    Ok(())
}

pub(crate) fn cmd_live_res_prop(args: &LiveNodePropArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_value: serde_json::Value = serde_json::from_str(&args.value)
        .unwrap_or_else(|_| serde_json::Value::String(args.value.clone()));

    daemon_cmd(
        "debug_live_res_prop",
        serde_json::json!({
            "id": args.id,
            "property": args.property,
            "value": json_value,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": args.id,
                    "property": args.property,
                    "value": json_value,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {}.{} = {}",
                "Set".green(),
                format!("[{}]", args.id).dimmed(),
                args.property.cyan(),
                args.value.green(),
            );
        }
    }
    Ok(())
}

pub(crate) fn cmd_live_node_prop_res(args: &LivePropResArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
        "debug_live_node_prop_res",
        serde_json::json!({
            "id": args.id,
            "property": args.property,
            "res_path": args.res_path,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": args.id,
                    "property": args.property,
                    "res_path": args.res_path,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {}.{} = {}",
                "Set".green(),
                format!("[{}]", args.id).dimmed(),
                args.property.cyan(),
                args.res_path.cyan(),
            );
        }
    }
    Ok(())
}

pub(crate) fn cmd_live_res_prop_res(args: &LivePropResArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
        "debug_live_res_prop_res",
        serde_json::json!({
            "id": args.id,
            "property": args.property,
            "res_path": args.res_path,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": args.id,
                    "property": args.property,
                    "res_path": args.res_path,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {}.{} = {}",
                "Set".green(),
                format!("[{}]", args.id).dimmed(),
                args.property.cyan(),
                args.res_path.cyan(),
            );
        }
    }
    Ok(())
}

pub(crate) fn cmd_live_res_call(args: &LiveNodeCallArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_args: serde_json::Value =
        serde_json::from_str(&args.args).unwrap_or_else(|_| serde_json::json!([]));

    daemon_cmd(
        "debug_live_res_call",
        serde_json::json!({
            "id": args.id,
            "method": args.method,
            "args": json_args,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": args.id,
                    "method": args.method,
                    "args": json_args,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!(
                "{} {}.{}({})",
                "Called".green(),
                format!("[{}]", args.id).dimmed(),
                args.method.cyan(),
                args.args.dimmed(),
            );
        }
    }
    Ok(())
}

// ── Live editing: advanced node operations (binary protocol) ────────

pub(crate) fn cmd_live_remove_keep(args: &LiveRemoveKeepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
        "debug_live_remove_and_keep_node",
        serde_json::json!({"path": args.path, "object_id": args.object_id}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "path": args.path,
                    "object_id": args.object_id,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!("{} {}", "Removed (kept)".green(), args.path.cyan(),);
        }
    }
    Ok(())
}

pub(crate) fn cmd_live_restore(args: &LiveRestoreArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
        "debug_live_restore_node",
        serde_json::json!({
            "object_id": args.object_id,
            "path": args.path,
            "pos": args.pos,
        }),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "object_id": args.object_id,
                    "path": args.path,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            println!("{} at {}", "Restored node".green(), args.path.cyan(),);
        }
    }
    Ok(())
}
