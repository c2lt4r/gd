use miette::{miette, Result};
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
}

impl Default for FmtConfig {
    fn default() -> Self {
        Self {
            use_tabs: true,
            indent_size: 4,
            max_line_length: 100,
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
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            disabled_rules: Vec::new(),
            max_function_length: 50,
        }
    }
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
                toml::from_str(&content)
                    .map_err(|e| miette!("Failed to parse {}: {e}", path.display()))
            }
            None => Ok(Self::default()),
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
