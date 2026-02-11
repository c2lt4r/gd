# gd

**The Godot toolchain.** A fast, all-in-one CLI for formatting, linting, building, and managing Godot projects — like `cargo` for GDScript.

Built with [tree-sitter](https://tree-sitter.github.io/) for accurate parsing and [Rayon](https://github.com/rayon-rs/rayon) for parallel file processing.

## Features

- **Format** GDScript files with an AST-based formatter aligned to the [GDScript style guide](https://docs.godotengine.org/en/stable/tutorials/scripting/gdscript/gdscript_styleguide.html)
- **Lint** with 46 built-in rules (11 auto-fixable), SARIF output for CI
- **Run**, **build**, **test**, and **clean** your Godot project from the terminal
- **Watch** for file changes and auto-lint/format on save
- **Manage addons** from Git or the Godot Asset Library
- **Generate CI/CD** configurations for GitHub Actions and GitLab CI
- **LSP server** with formatting, diagnostics, hover, go-to-definition, references, rename, and completion
- **Analyze** your project with dependency graphs, class trees, and code statistics

## Installation

### From source

```sh
git clone https://github.com/c2lt4r/gd.git
cd gd
cargo install --path .
```

### With cargo

```sh
cargo install gd
```

## Quick Start

```sh
# Create a new Godot project
gd new my-game

# Or create from a GitHub template
gd new my-game --from user/godot-template

cd my-game

# Format all GDScript files
gd fmt

# Lint for issues
gd lint

# Run the project
gd run
```

## Commands

| Command | Description |
|---------|-------------|
| `gd new <name>` | Create a new Godot project (templates: `default`, `2d`, `3d`, or `--from` GitHub) |
| `gd init` | Initialize gd toolchain in an existing Godot project |
| `gd fmt` | Format GDScript files |
| `gd lint` | Lint GDScript files |
| `gd run` | Run the Godot project |
| `gd build` | Build/export the Godot project |
| `gd check` | Check project for errors without building |
| `gd clean` | Clean build artifacts |
| `gd test` | Run GDScript tests |
| `gd completions` | Generate shell completions (bash, zsh, fish, etc.) |
| `gd tree` | Show project class hierarchy |
| `gd doc` | Generate documentation from doc comments |
| `gd watch` | Watch files and run fmt/lint on changes |
| `gd addons` | Manage project addons (install, remove, search) |
| `gd stats` | Show project statistics |
| `gd ci` | Generate CI/CD pipeline configuration |
| `gd lsp` | Start the Language Server Protocol server |
| `gd deps` | Show script dependency graph |
| `gd man` | Generate man page |
| `gd upgrade` | Self-update to latest release |

### Formatter

```sh
# Format all .gd files in the project
gd fmt

# Check formatting without modifying files (useful in CI)
gd fmt --check

# Show a diff of what would change
gd fmt --diff

# Format specific files or directories
gd fmt scripts/ player.gd
```

### Linter

```sh
# Lint the entire project
gd lint

# Auto-fix supported issues
gd lint --fix

# Output as JSON
gd lint --format json

# Output as SARIF (for GitHub Code Scanning)
gd lint --format sarif

# Lint specific files
gd lint scripts/player.gd
```

### Addons

```sh
# Search the Godot Asset Library
gd addons search "dialogue"

# Install from the Asset Library (by ID or name)
gd addons install 12345
gd addons install "some-addon"

# Install from a Git URL
gd addons install https://github.com/user/addon.git

# List installed addons
gd addons list

# Remove an addon
gd addons remove my-addon
```

### Watch Mode

```sh
# Watch and lint on changes (default)
gd watch

# Watch with auto-formatting
gd watch --fmt

# Watch with Godot error checking
gd watch --check

# Disable lint during watch
gd watch --no-lint
```

### Dependency Graph

```sh
# Show dependency tree
gd deps

# Output as Graphviz DOT
gd deps --format dot

# Output as JSON
gd deps --format json
```

### CI/CD Generation

```sh
# Generate GitHub Actions workflow
gd ci github

# Generate GitLab CI configuration
gd ci gitlab

# Include export stage
gd ci github --export --godot-version 4.4
```

### GitHub Templates

Create projects from any GitHub repository containing a Godot project:

```sh
# From a GitHub repo (auto-detects default branch)
gd new my-game --from user/godot-template

# With a specific branch or tag
gd new my-game --from user/repo@v1.0

# Full GitHub URLs also work
gd new my-game --from https://github.com/user/repo
```

The template system automatically finds `project.godot` within the repository to determine the project root, so templates with nested directory structures work correctly.

## Lint Rules

All 46 built-in rules (38 default-enabled, 8 opt-in):

| Rule | Description | Severity | Fixable |
|------|-------------|----------|---------|
| `await-in-ready` | Detect `await` in `_ready()` | warning | |
| `comparison-with-boolean` | Flag explicit `== true`/`false` comparisons | warning | yes |
| `comparison-with-itself` | Detect `x == x` self-comparisons | warning | |
| `cyclomatic-complexity` | Warn on high cyclomatic complexity | warning | |
| `deeply-nested-code` | Warn on deeply nested code blocks | warning | |
| `duplicate-function` | Detect duplicate function definitions | error | |
| `duplicate-key` | Detect duplicate dictionary keys | warning | |
| `duplicate-signal` | Detect duplicate signal declarations | error | |
| `duplicated-load` | Detect duplicate load/preload calls | warning | |
| `empty-function` | Detect functions with only `pass` in body | warning | |
| `enum-naming` | Enforce PascalCase/UPPER_SNAKE_CASE enums | warning | yes |
| `float-comparison` | Warn on float equality comparisons | warning | yes |
| `get-node-in-process` | Detect `get_node()` in `_process()` | warning | |
| `integer-division` | Warn on integer literal division truncation | warning | |
| `long-function` | Warn on functions exceeding line threshold | warning | |
| `loop-variable-name` | Enforce snake_case loop variables | warning | yes |
| `missing-return` | Detect missing return in typed functions | warning | |
| `missing-type-hint` | Warn on missing type annotations | warning | |
| `naming-convention` | Enforce snake_case/PascalCase naming | warning | yes |
| `node-ready-order` | Detect node access before tree is ready | warning | |
| `parameter-naming` | Enforce snake_case parameters | warning | yes |
| `physics-in-process` | Detect physics calls in `_process()` | warning | |
| `preload-type-hint` | Warn on untyped preload/load assignments | warning | |
| `private-method-access` | Warn on calling private methods externally | warning | |
| `redundant-else` | Detect unnecessary else after return | warning | |
| `return-type-mismatch` | Detect void/non-void return mismatches | warning | |
| `self-assignment` | Detect `x = x` assignments | warning | yes |
| `shadowed-variable` | Detect variable shadowing in inner scopes | warning | |
| `signal-name-convention` | Warn on signals with `on_` prefix | warning | yes |
| `standalone-expression` | Detect side-effect-free expressions | warning | |
| `static-type-inference` | Suggest explicit type annotations | warning | |
| `too-many-parameters` | Warn on functions with too many parameters | warning | |
| `unnecessary-pass` | Detect `pass` in non-empty function bodies | warning | yes |
| `unreachable-code` | Detect code after return/break/continue | warning | yes |
| `untyped-array` | Suggest typed array annotations | warning | |
| `unused-preload` | Detect unused preload variables | warning | |
| `unused-signal` | Detect signals that are never emitted | warning | |
| `unused-variable` | Detect unused local variables | warning | yes |

**Opt-in rules** (enable via `[lint.rules.<name>]` in `gd.toml`):

| Rule | Description | Severity | Fixable |
|------|-------------|----------|---------|
| `class-definitions-order` | Enforce canonical member ordering | warning | |
| `magic-number` | Flag unexplained numeric literals | warning | |
| `max-file-lines` | Enforce maximum file length | warning | |
| `max-line-length` | Enforce maximum line length | warning | |
| `max-public-methods` | Enforce maximum public methods per class | warning | |
| `print-statement` | Detect debug print calls | warning | |
| `todo-comment` | Detect TODO/FIXME/HACK comments | warning | |
| `unused-parameter` | Detect unused function parameters | warning | |

### Inline Suppression

Suppress lint warnings with comments:

```gdscript
# Suppress all rules on this line
var x = 42  # gd:ignore

# Suppress all rules on the next line
# gd:ignore-next-line
var y = 42

# Suppress specific rules
var z = 42  # gd:ignore[magic-number]

# Suppress specific rules on next line
# gd:ignore-next-line[naming-convention, magic-number]
var MyVar = 100
```

## Configuration

Configure `gd` via a `gd.toml` file in your project root. The toolchain searches upward from the current directory.

```toml
[fmt]
use_tabs = true
indent_size = 4
max_line_length = 100
blank_lines_around_functions = 2
blank_lines_around_classes = 2
trailing_newline = true

[lint]
disabled_rules = []
max_function_length = 50
ignore_patterns = ["addons/**"]

# Per-rule severity overrides
[lint.rules.magic-number]
severity = "warning"  # enable opt-in rule

[lint.rules.naming-convention]
severity = "error"  # upgrade to error

[build]
output_dir = "build"

[run]
# godot_path = "/usr/bin/godot"
extra_args = []
```

### Configuration Options

**`[fmt]`**

| Option | Default | Description |
|--------|---------|-------------|
| `use_tabs` | `true` | Use tabs instead of spaces for indentation |
| `indent_size` | `4` | Number of spaces per indent level (when not using tabs) |
| `max_line_length` | `100` | Maximum line length before warnings |
| `blank_lines_around_functions` | `2` | Blank lines around top-level functions |
| `blank_lines_around_classes` | `2` | Blank lines around inner class definitions |
| `trailing_newline` | `true` | Ensure file ends with exactly one newline |

**`[lint]`**

| Option | Default | Description |
|--------|---------|-------------|
| `disabled_rules` | `[]` | List of rule names to disable |
| `max_function_length` | `50` | Max lines before `long-function` warns |
| `max_function_params` | `5` | Max parameters before `too-many-parameters` warns |
| `max_cyclomatic_complexity` | `10` | Max complexity before `cyclomatic-complexity` warns |
| `max_nesting_depth` | `4` | Max depth before `deeply-nested-code` warns |
| `max_line_length` | `120` | Max line length before `max-line-length` warns |
| `max_file_lines` | `500` | Max file lines before `max-file-lines` warns |
| `max_public_methods` | `20` | Max public methods before `max-public-methods` warns |
| `ignore_patterns` | `[]` | Glob patterns for files to skip |

**`[lint.rules.<name>]`** — per-rule overrides:

| Option | Values | Description |
|--------|--------|-------------|
| `severity` | `"warning"`, `"error"`, `"off"` | Override severity or disable a rule |

Set severity on an opt-in rule to enable it. Set `"off"` to disable any rule.

**`[build]`**

| Option | Default | Description |
|--------|---------|-------------|
| `output_dir` | `"build"` | Directory for export output |

**`[run]`**

| Option | Default | Description |
|--------|---------|-------------|
| `godot_path` | (auto) | Path to the Godot binary; uses `PATH` if unset |
| `extra_args` | `[]` | Additional arguments passed to Godot |

## SARIF Output

`gd lint` supports [SARIF 2.1.0](https://sarifweb.azurewebsites.net/) output for integration with GitHub Code Scanning:

```sh
gd lint --format sarif > results.sarif
```

Example GitHub Actions step:

```yaml
- name: Lint GDScript
  run: gd lint --format sarif > results.sarif

- name: Upload SARIF
  uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: results.sarif
```

## LSP Server

`gd lsp` starts a Language Server Protocol server over stdio, providing editor integration with:

- **Diagnostics** &mdash; real-time lint warnings and errors
- **Formatting** &mdash; format documents on save
- **Code actions** &mdash; quick fixes for lint issues
- **Document symbols** &mdash; outline of classes, functions, signals, and variables
- **Hover** &mdash; type and documentation info on hover
- **Go to definition** &mdash; jump to function and variable declarations
- **Find references** &mdash; find all usages across the project
- **Rename** &mdash; rename symbols across files with prepare-rename support
- **Completion** &mdash; context-aware autocomplete for symbols and builtins

### Editor Setup

**VS Code:** Download the `.vsix` from the [latest release](https://github.com/c2lt4r/gd/releases/latest), then install it with:

```sh
code --install-extension gd-gdscript-0.1.1.vsix
```

**Neovim (nvim-lspconfig):**

```lua
require('lspconfig').gdscript_gd.setup {
  cmd = { 'gd', 'lsp' },
  filetypes = { 'gdscript' },
}
```

## License

MIT
