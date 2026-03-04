/// A captured output line from Godot's `print()` / `push_error()` / `push_warning()`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CapturedOutput {
    pub message: String,
    /// `"log"`, `"error"`, `"warning"`, or `"log_rich"`
    pub r#type: String,
}
