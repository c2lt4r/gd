use gd_core::cprintln;
use miette::{Result, miette};
use owo_colors::OwoColorize;

pub fn exec() -> Result<()> {
    // Try via daemon first (preferred — also clears debug server state)
    if let Some(result) =
        gd_lsp::daemon_client::query_daemon("debug_stop_game", serde_json::json!({}), None)
        && result.get("error").is_none()
    {
        let pid = result
            .get("pid")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        cprintln!("{} Game stopped (pid {pid})", "✓".green());
        return Ok(());
    }

    // Fallback: read game_pid from state file (daemon may have died)
    let cwd = std::env::current_dir().unwrap_or_default();
    let root = gd_core::config::find_project_root(&cwd)
        .ok_or_else(|| miette!("Not in a Godot project"))?;
    if let Some(state) = gd_lsp::daemon::read_state_file(&root)
        && let Some(pid) = state.game_pid
    {
        kill_pid(pid);
        // Clear the game_pid from state file
        gd_lsp::daemon::clear_game_pid_in_state(&root);
        cprintln!("{} Game stopped (pid {pid})", "✓".green());
        return Ok(());
    }

    Err(miette!(
        "No game process tracked — was the game launched with `gd run`?"
    ))
}

pub use gd_core::process::kill_game_process;

fn kill_pid(pid: u32) {
    kill_game_process(pid);
}
