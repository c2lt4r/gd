/// Clippy-style category for organizing lint rules.
/// Each rule belongs to exactly one category. Categories can be bulk-enabled
/// or disabled via `[lint]` in gd.toml.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LintCategory {
    /// Definite bugs
    Correctness,
    /// Likely bugs, may be intentional
    Suspicious,
    /// Naming and code style
    Style,
    /// Code size and complexity metrics
    Complexity,
    /// Godot runtime performance
    Performance,
    /// Godot engine best practices
    Godot,
    /// Type system strictness
    TypeSafety,
    /// Unused code and debug artifacts
    Maintenance,
}

impl std::fmt::Display for LintCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LintCategory::Correctness => write!(f, "correctness"),
            LintCategory::Suspicious => write!(f, "suspicious"),
            LintCategory::Style => write!(f, "style"),
            LintCategory::Complexity => write!(f, "complexity"),
            LintCategory::Performance => write!(f, "performance"),
            LintCategory::Godot => write!(f, "godot"),
            LintCategory::TypeSafety => write!(f, "type_safety"),
            LintCategory::Maintenance => write!(f, "maintenance"),
        }
    }
}

impl std::str::FromStr for LintCategory {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "correctness" => Ok(LintCategory::Correctness),
            "suspicious" => Ok(LintCategory::Suspicious),
            "style" => Ok(LintCategory::Style),
            "complexity" => Ok(LintCategory::Complexity),
            "performance" => Ok(LintCategory::Performance),
            "godot" => Ok(LintCategory::Godot),
            "type_safety" => Ok(LintCategory::TypeSafety),
            "maintenance" => Ok(LintCategory::Maintenance),
            _ => Err(format!(
                "invalid category '{s}': expected correctness, suspicious, style, complexity, performance, godot, type_safety, or maintenance"
            )),
        }
    }
}
