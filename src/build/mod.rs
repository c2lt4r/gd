use miette::Result;

/// Run the Godot project.
pub fn run_project(
    scene: Option<&str>,
    debug: bool,
    verbose: bool,
    extra: &[String],
) -> Result<()> {
    let _ = (scene, debug, verbose, extra);
    todo!("Run not yet implemented")
}

/// Export/build the Godot project.
pub fn export_project(
    preset: Option<&str>,
    output: Option<&str>,
    release: bool,
) -> Result<()> {
    let _ = (preset, output, release);
    todo!("Export not yet implemented")
}
