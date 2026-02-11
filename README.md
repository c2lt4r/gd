# gd

**The Godot toolchain.** A fast, all-in-one CLI for formatting, linting, building, and managing Godot projects — like `cargo` for GDScript.

Built with [tree-sitter](https://tree-sitter.github.io/) for accurate parsing and [Rayon](https://github.com/rayon-rs/rayon) for parallel file processing.

## Features

- **Format** GDScript files with an AST-based formatter aligned to the [GDScript style guide](https://docs.godotengine.org/en/stable/tutorials/scripting/gdscript/gdscript_styleguide.html)
- **Lint** with 25 built-in rules, auto-fix support, and SARIF output for CI
- **Run**, **build**, **test**, and **clean** your Godot project from the terminal
- **Watch** for file changes and auto-lint/format on save
- **Manage addons** from Git or the Godot Asset Library
- **Generate CI/CD** configurations for GitHub Actions and GitLab CI
- **LSP server** with formatting, diagnostics, hover, go-to-definition, references, and rename
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
| `gd new <name>` | Create a new Godot project (templates: `default`, `2d`, `3d`) |
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

## Lint Rules

All 25 built-in rules:

| Rule | Description | Severity | Fixable |
|------|-------------|----------|---------|
| `naming-convention` | Check snake_case/PascalCase naming conventions | warning | yes |
| `unused-variable` | Detect assigned but unused variables | warning | |
| `missing-type-hint` | Warn on missing parameter and return type hints | warning | |
| `empty-function` | Detect functions with only `pass` in body | warning | |
| `long-function` | Warn on functions exceeding line threshold | warning | |
| `duplicate-signal` | Detect duplicate signal declarations | error | |
| `self-assignment` | Detect variables assigned to themselves | warning | |
| `unreachable-code` | Detect code after return/break/continue | warning | |
| `shadowed-variable` | Detect variable shadowing in inner scopes | warning | |
| `comparison-with-boolean` | Flag explicit `== true`/`false` comparisons | warning | |
| `unnecessary-pass` | Detect `pass` in non-empty function bodies | warning | |
| `preload-type-hint` | Warn on untyped preload/load assignments | warning | |
| `integer-division` | Warn on integer literal division truncation | warning | |
| `signal-name-convention` | Warn on signals with `on_` prefix | warning | yes |
| `magic-number` | Flag unexplained numeric literals in functions | warning | |
| `float-comparison` | Warn on float equality comparisons | warning | |
| `missing-super-call` | Warn on lifecycle overrides without `super()` | warning | |
| `return-type-mismatch` | Detect void/non-void return mismatches | warning | |
| `private-method-access` | Warn on calling private methods externally | warning | |
| `untyped-array` | Suggest typed array annotations | warning | |
| `duplicate-function` | Detect duplicate function definitions | error | |
| `unused-signal` | Detect signals that are never emitted | warning | |
| `duplicate-key` | Detect duplicate dictionary keys | warning | |
| `await-in-ready` | Warn about `await` in `_ready()` | warning | |
| `missing-return` | Detect missing return in typed functions | warning | |

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

[lint]
disabled_rules = []
max_function_length = 50

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

**`[lint]`**

| Option | Default | Description |
|--------|---------|-------------|
| `disabled_rules` | `[]` | List of rule names to disable |
| `max_function_length` | `50` | Max lines in a function before `long-function` warns |

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

### Editor Setup

**VS Code:** Download the `.vsix` from the [latest release](https://github.com/c2lt4r/gd/releases/latest), then install it with:

```sh
code --install-extension gd-gdscript-0.1.0.vsix
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
