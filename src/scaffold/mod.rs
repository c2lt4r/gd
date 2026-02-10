pub mod templates;

use miette::Result;

/// Create a new Godot project with the given name and template.
pub fn create_project(name: &str, template: &str) -> Result<()> {
    let _ = (name, template);
    todo!("Scaffold not yet implemented")
}
