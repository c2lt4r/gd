use std::collections::HashMap;

use miette::{Result, miette};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = "gd.toml";

/// Root configuration loaded from gd.toml.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub fmt: FmtConfig,
    pub lint: LintConfig,
    pub build: BuildConfig,
    pub run: RunConfig,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct FmtConfig {
    /// Use tabs instead of spaces.
    pub use_tabs: bool,
    /// Number of spaces per indentation level (if not using tabs).
    pub indent_size: usize,
    /// Maximum line length before wrapping.
    pub max_line_length: usize,
    /// Number of blank lines around top-level function definitions.
    pub blank_lines_around_functions: usize,
    /// Number of blank lines around top-level class definitions.
    pub blank_lines_around_classes: usize,
    /// Ensure file ends with exactly one newline.
    pub trailing_newline: bool,
}

impl Default for FmtConfig {
    fn default() -> Self {
        Self {
            use_tabs: true,
            indent_size: 4,
            max_line_length: 100,
            blank_lines_around_functions: 2,
            blank_lines_around_classes: 2,
            trailing_newline: true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LintConfig {
    /// Lint rules to disable.
    pub disabled_rules: Vec<String>,
    /// Maximum number of lines in a function before long-function warns.
    pub max_function_length: usize,
    /// Maximum number of function parameters before too-many-parameters warns.
    pub max_function_params: usize,
    /// Maximum cyclomatic complexity before cyclomatic-complexity warns.
    pub max_cyclomatic_complexity: usize,
    /// Maximum nesting depth before deeply-nested-code warns.
    pub max_nesting_depth: usize,
    /// Maximum line length before max-line-length warns.
    pub max_line_length: usize,
    /// Maximum file lines before max-file-lines warns.
    pub max_file_lines: usize,
    /// Maximum public methods per class before max-public-methods warns.
    pub max_public_methods: usize,
    /// Maximum total functions per class/script before god-object warns.
    pub max_god_object_functions: usize,
    /// Maximum member variables per class/script before god-object warns.
    pub max_god_object_members: usize,
    /// Maximum lines per class/script before god-object warns.
    pub max_god_object_lines: usize,
    /// Per-rule severity overrides.
    #[serde(default)]
    pub rules: HashMap<String, RuleConfig>,
    /// Glob patterns for files to skip during linting.
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    /// Per-path rule overrides.
    #[serde(default)]
    pub overrides: Vec<LintOverride>,

    // Category-level overrides: "off" | "info" | "warning" | "error"
    /// Definite bugs
    pub correctness: Option<String>,
    /// Likely bugs, may be intentional
    pub suspicious: Option<String>,
    /// Naming and code style
    pub style: Option<String>,
    /// Code size and complexity metrics
    pub complexity: Option<String>,
    /// Godot runtime performance
    pub performance: Option<String>,
    /// Godot engine best practices
    pub godot: Option<String>,
    /// Type system strictness
    pub type_safety: Option<String>,
    /// Unused code and debug artifacts
    pub maintenance: Option<String>,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            disabled_rules: Vec::new(),
            max_function_length: 50,
            max_function_params: 5,
            max_cyclomatic_complexity: 10,
            max_nesting_depth: 4,
            max_line_length: 120,
            max_file_lines: 500,
            max_public_methods: 20,
            max_god_object_functions: 20,
            max_god_object_members: 15,
            max_god_object_lines: 500,
            rules: HashMap::new(),
            ignore_patterns: Vec::new(),
            overrides: Vec::new(),
            correctness: None,
            suspicious: None,
            style: None,
            complexity: None,
            performance: None,
            godot: None,
            type_safety: None,
            maintenance: None,
        }
    }
}

impl LintConfig {
    /// Get the configured level for a category, if set.
    pub fn category_level(&self, cat: crate::lint::rules::LintCategory) -> Option<&str> {
        use crate::lint::rules::LintCategory;
        match cat {
            LintCategory::Correctness => self.correctness.as_deref(),
            LintCategory::Suspicious => self.suspicious.as_deref(),
            LintCategory::Style => self.style.as_deref(),
            LintCategory::Complexity => self.complexity.as_deref(),
            LintCategory::Performance => self.performance.as_deref(),
            LintCategory::Godot => self.godot.as_deref(),
            LintCategory::TypeSafety => self.type_safety.as_deref(),
            LintCategory::Maintenance => self.maintenance.as_deref(),
        }
    }
}

/// Per-path rule overrides in `[[lint.overrides]]`.
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct LintOverride {
    /// Glob patterns matching file paths (same syntax as `ignore_patterns`).
    #[serde(default)]
    pub paths: Vec<String>,
    /// Rules to exclude for files matching these paths.
    #[serde(default)]
    pub exclude_rules: Vec<String>,
}

/// Per-rule configuration in `[lint.rules.<name>]`.
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct RuleConfig {
    /// Override severity: "info", "warning", "error", or "off".
    pub severity: Option<String>,
    /// Override max lines for long-function rule.
    pub max_lines: Option<usize>,
    /// Override max complexity for cyclomatic-complexity rule.
    pub max_complexity: Option<usize>,
    /// Override max params for too-many-parameters rule.
    pub max_params: Option<usize>,
    /// Override max depth for deeply-nested-code rule.
    pub max_depth: Option<usize>,
    /// magic-number: numeric literals that are always allowed (e.g. [0, 1, 0.5, 2.0]).
    #[serde(default)]
    pub allowed: Vec<f64>,
    /// magic-number: function/constructor calls where numbers are fine (e.g. `["Vector2", "lerp"]`).
    #[serde(default)]
    pub allowed_contexts: Vec<String>,
    /// magic-number: only flag numbers whose absolute value >= this threshold.
    pub min_value: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildConfig {
    /// Export presets to use.
    pub presets: Vec<String>,
    /// Output directory for exports.
    pub output_dir: PathBuf,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            presets: Vec::new(),
            output_dir: PathBuf::from("build"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct RunConfig {
    /// Path to godot binary. Uses PATH lookup if empty.
    pub godot_path: Option<PathBuf>,
    /// Extra arguments to pass to godot.
    pub extra_args: Vec<String>,
}

impl Config {
    /// Load configuration from a gd.toml file, searching upward from `start`.
    /// Returns default config if no file found.
    pub fn load(start: &Path) -> Result<Self> {
        match find_config(start) {
            Some(path) => {
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| miette!("Failed to read {}: {e}", path.display()))?;
                let config: Self = toml::from_str(&content)
                    .map_err(|e| miette!("Failed to parse {}: {e}", path.display()))?;
                if let Ok(raw) = content.parse::<toml::Value>() {
                    warn_unknown_keys(&raw);
                }
                Ok(config)
            }
            None => Ok(Self::default()),
        }
    }
}

/// Print warnings to stderr for any unrecognized keys in gd.toml.
fn warn_unknown_keys(raw: &toml::Value) {
    use owo_colors::OwoColorize;

    let Some(table) = raw.as_table() else {
        return;
    };

    let known_top = &["fmt", "lint", "build", "run"];
    let known_fmt = &[
        "use_tabs",
        "indent_size",
        "max_line_length",
        "blank_lines_around_functions",
        "blank_lines_around_classes",
        "trailing_newline",
    ];
    let known_lint = &[
        "disabled_rules",
        "max_function_length",
        "max_function_params",
        "max_cyclomatic_complexity",
        "max_nesting_depth",
        "max_line_length",
        "max_file_lines",
        "max_public_methods",
        "max_god_object_functions",
        "max_god_object_members",
        "max_god_object_lines",
        "rules",
        "ignore_patterns",
        "overrides",
        // Category-level overrides
        "correctness",
        "suspicious",
        "style",
        "complexity",
        "performance",
        "godot",
        "type_safety",
        "maintenance",
    ];
    let known_rule = &[
        "severity",
        "max_lines",
        "max_complexity",
        "max_params",
        "max_depth",
        "allowed",
        "allowed_contexts",
        "min_value",
    ];
    let known_build = &["presets", "output_dir"];
    let known_run = &["godot_path", "extra_args"];

    for key in table.keys() {
        if !known_top.contains(&key.as_str()) {
            eprintln!(
                "{}: unknown key '{}' in gd.toml",
                "warning".yellow().bold(),
                key.bold()
            );
        }
    }

    check_section(table, "fmt", known_fmt);
    check_section(table, "build", known_build);
    check_section(table, "run", known_run);

    // Lint section has nested rules.<name> tables
    if let Some(toml::Value::Table(lint)) = table.get("lint") {
        for key in lint.keys() {
            if !known_lint.contains(&key.as_str()) {
                warn_key(key, "lint");
            }
        }
        if let Some(toml::Value::Table(rules)) = lint.get("rules") {
            for (rule_name, rule_val) in rules {
                if let Some(rule_table) = rule_val.as_table() {
                    for key in rule_table.keys() {
                        if !known_rule.contains(&key.as_str()) {
                            warn_key(key, &format!("lint.rules.{rule_name}"));
                        }
                    }
                }
            }
        }
        // Validate [[lint.overrides]] entries
        let known_override = &["paths", "exclude_rules"];
        if let Some(toml::Value::Array(overrides)) = lint.get("overrides") {
            for (i, entry) in overrides.iter().enumerate() {
                if let Some(table) = entry.as_table() {
                    for key in table.keys() {
                        if !known_override.contains(&key.as_str()) {
                            warn_key(key, &format!("lint.overrides[{i}]"));
                        }
                    }
                }
            }
        }
    }
}

fn check_section(table: &toml::map::Map<String, toml::Value>, section: &str, known: &[&str]) {
    if let Some(toml::Value::Table(sub)) = table.get(section) {
        for key in sub.keys() {
            if !known.contains(&key.as_str()) {
                warn_key(key, section);
            }
        }
    }
}

fn warn_key(key: &str, section: &str) {
    use owo_colors::OwoColorize;
    eprintln!(
        "{}: unknown key '{}' in [{}] section of gd.toml",
        "warning".yellow().bold(),
        key.bold(),
        section
    );
}

/// Walk upward from `start` looking for the project root (directory containing gd.toml or project.godot).
pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut dir = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        if dir.join(CONFIG_FILE).is_file() || dir.join("project.godot").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Walk upward from `start` looking for gd.toml.
fn find_config(start: &Path) -> Option<PathBuf> {
    let mut dir = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        let candidate = dir.join(CONFIG_FILE);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Capture stderr output from warn_unknown_keys by using a gd.toml with unknown keys.
    #[test]
    fn test_warn_unknown_keys_detects_bad_keys() {
        let raw: toml::Value = toml::from_str(
            r#"
            typo_key = "oops"

            [fmt]
            use_tabs = true
            indnt_size = 4

            [lint]
            disbled_rules = []

            [lint.rules.my-rule]
            severity = "error"
            extra_field = true

            [build]
            preseets = []

            [run]
            godott_path = "/usr/bin/godot"
            "#,
        )
        .unwrap();

        // The function prints to stderr; we just verify it doesn't panic
        // and parses all sections correctly.
        warn_unknown_keys(&raw);
    }

    #[test]
    fn test_warn_unknown_keys_no_warnings_for_valid() {
        let raw: toml::Value = toml::from_str(
            r#"
            [fmt]
            use_tabs = true
            indent_size = 4
            max_line_length = 120
            blank_lines_around_functions = 2
            blank_lines_around_classes = 2
            trailing_newline = true

            [lint]
            disabled_rules = ["unused-variable"]
            max_function_length = 80
            ignore_patterns = ["addons/**"]
            correctness = "error"
            suspicious = "warning"
            type_safety = "warning"
            maintenance = "off"

            [[lint.overrides]]
            paths = ["**/test/**", "**/tests/**"]
            exclude_rules = ["private-method-access", "unreachable-code"]

            [[lint.overrides]]
            paths = ["addons/**"]
            exclude_rules = ["naming-convention"]

            [lint.rules.naming-convention]
            severity = "error"

            [lint.rules.long-function]
            severity = "warning"
            max_lines = 80

            [lint.rules.cyclomatic-complexity]
            max_complexity = 15

            [lint.rules.too-many-parameters]
            max_params = 8

            [lint.rules.deeply-nested-code]
            max_depth = 6

            [build]
            presets = ["web"]
            output_dir = "dist"

            [run]
            extra_args = ["--verbose"]
            "#,
        )
        .unwrap();

        warn_unknown_keys(&raw);
    }

    #[test]
    fn test_load_default_when_no_config() {
        let config = Config::load(Path::new("/nonexistent/path")).unwrap();
        assert!(config.fmt.use_tabs);
        assert_eq!(config.fmt.indent_size, 4);
        assert_eq!(config.lint.max_function_length, 50);
    }
}
