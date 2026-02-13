# gd

**The Godot toolchain.** A fast, all-in-one CLI for formatting, linting, building, and managing Godot projects — like `cargo` for GDScript.

Built with [tree-sitter](https://tree-sitter.github.io/) for accurate parsing and [Rayon](https://github.com/rayon-rs/rayon) for parallel file processing.

## Features

- **Format** GDScript files with an AST-based formatter aligned to the [GDScript style guide](https://docs.godotengine.org/en/stable/tutorials/scripting/gdscript/gdscript_styleguide.html)
- **Lint** with 58 built-in rules (13 auto-fixable), SARIF output for CI
- **Run**, **build**, **test**, and **clean** your Godot project from the terminal
- **Watch** for file changes and auto-lint/format on save
- **Manage addons** from Git or the Godot Asset Library (with lockfile and update support)
- **Generate CI/CD** configurations for GitHub Actions and GitLab CI
- **LSP server** with formatting, diagnostics, hover, go-to-definition, references, rename, completion, and 16 refactoring commands
- **Scene analysis** &mdash; validate `.tscn`/`.tres` files, visualize scene node hierarchies, and track resource dependencies
- **Debug** a running Godot game via DAP &mdash; breakpoints, stepping, variable inspection, and expression evaluation
- **Godot LSP proxy** &mdash; forward hover, completion, and go-to-definition to Godot's built-in LSP when the editor is running
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
| `gd init` | Initialize gd toolchain in an existing project (detects export paths) |
| `gd fmt` | Format GDScript files |
| `gd lint` | Lint GDScript files |
| `gd run` | Run the Godot project (launches via DAP when editor is open) |
| `gd build` | Build/export the Godot project |
| `gd check` | Check project for errors (parse, structural, semantic, `.tscn`/`.tres` validation) (`--format json`) |
| `gd clean` | Clean build artifacts |
| `gd test` | Run GDScript tests with GUT, gdUnit4, or raw scripts (`--format json`) |
| `gd completions` | Generate shell completions (bash, zsh, fish, etc.) |
| `gd tree` | Show project class hierarchy (`--scene` for scene node trees) |
| `gd doc` | Generate documentation from doc comments (`--format json`, `--check`) |
| `gd watch` | Watch files and run fmt/lint on changes |
| `gd addons` | Manage project addons (install, remove, search, update, lock) |
| `gd stats` | Show project statistics (`--diff <branch>`, `--by-dir`, `--top N`) |
| `gd ci` | Generate CI/CD pipeline configuration |
| `gd debug` | Debug a running Godot game via DAP (breakpoints, stepping, eval, set-var, conditional breaks) |
| `gd lsp` | Start the LSP server, or run one-shot queries (see below) |
| `gd deps` | Show script dependency graph (`--include-resources` for `.tscn`/`.tres`) |
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

# Show surrounding code context (like grep -C)
gd lint --context 3
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

# Check for updates
gd addons update

# Apply available updates
gd addons update --apply

# Generate a lock file for reproducible installs
gd addons lock

# Install all addons from the lock file
gd addons install --locked
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

# Include .tscn/.tres resource dependencies
gd deps --include-resources

# Output as Graphviz DOT
gd deps --format dot

# Output as JSON
gd deps --format json
```

### Scene Tree

```sh
# Show scene node hierarchy for a .tscn file
gd tree --scene main.tscn

# Show all scenes in a directory
gd tree --scene .

# JSON output
gd tree --scene main.tscn --format json
```

### Statistics

```sh
# Show project statistics
gd stats

# JSON output
gd stats --format json

# Per-directory breakdown
gd stats --by-dir

# Top 5 longest functions (complexity hotspots)
gd stats --top 5

# Compare current branch vs main
gd stats --diff main
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

### Debugging

Debug a running Godot game via the Debug Adapter Protocol. A background daemon maintains persistent connections to Godot's LSP and DAP servers, so CLI queries are instant.

```sh
# Launch game with debugging (returns immediately, game runs in Godot)
gd run

# Check DAP connection and threads
gd debug status

# Set breakpoint, wait for hit, dump stack + variables, then resume
gd debug break --file scripts/player.gd --line 42

# Break on function entry by name (no line number needed)
gd debug break --name apply_input

# Conditional breakpoint (only triggers when expression is true)
gd debug break --file scripts/player.gd --line 42 --condition "speed > 20.0"

# Evaluate expression while paused at breakpoint
gd debug eval --expr "self.speed"

# Modify a variable while paused
gd debug set-var --name speed --value 100.0
gd debug set-var --name is_boosting --value true --scope locals

# Execution control (non-interactive)
gd debug continue
gd debug next      # step over
gd debug step      # step into
gd debug pause

# Terminate the running game
gd debug stop

# Start an interactive debug session (REPL)
gd debug attach

# JSON output for scripting
gd debug break --file scripts/player.gd --line 42 --format json
gd debug status --format json
gd debug eval --expr "self.position" --format json
```

Interactive session commands (`gd debug attach`):

| Command | Description |
|---------|-------------|
| `break <file> <line>` | Set breakpoint(s) |
| `clear <file>` | Clear breakpoints in a file |
| `wait [timeout]` | Wait for breakpoint hit |
| `continue` / `c` | Continue execution |
| `pause` / `p` | Pause execution |
| `next` / `n` | Step over |
| `step` / `s` | Step into |
| `stack` / `bt` | Show call stack |
| `vars [scope]` | Show variables (locals/members/globals) |
| `expand <ref>` | Expand nested variable |
| `eval <expr>` | Evaluate expression |
| `quit` / `q` | Disconnect and exit |

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

All 58 built-in rules (42 default-enabled, 16 opt-in):

| Rule | Description | Severity | Fixable |
|------|-------------|----------|---------|
| `await-in-ready` | Detect `await` in `_ready()` | warning | |
| `callable-null-check` | Warn on `.call()` without `.is_valid()` guard | warning | |
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
| `monitoring-in-signal` | Detect direct `monitoring`/`monitorable` assignment in Area signal callbacks | warning | |
| `naming-convention` | Enforce snake_case/PascalCase naming | warning | yes |
| `node-ready-order` | Detect node access before tree is ready | warning | |
| `parameter-naming` | Enforce snake_case parameters | warning | yes |
| `parameter-shadows-field` | Warn when parameter name shadows a class field | warning | |
| `physics-in-process` | Detect physics calls in `_process()` | warning | |
| `preload-type-hint` | Warn on untyped preload/load assignments | warning | |
| `private-method-access` | Warn on calling private methods externally | warning | |
| `redundant-else` | Detect unnecessary else after return | warning | yes |
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
| `untyped-array-literal` | Warn on `var x := [...]` without typed Array annotation | warning | yes |
| `unused-preload` | Detect unused preload variables | warning | |
| `unused-signal` | Detect signals that are never emitted | warning | |
| `unused-variable` | Detect unused local variables | warning | yes |

**Opt-in rules** (enable via `[lint.rules.<name>]` in `gd.toml`):

| Rule | Description | Severity | Fixable |
|------|-------------|----------|---------|
| `breakpoint-statement` | Detect leftover `breakpoint` statements | info | |
| `class-definitions-order` | Enforce canonical member ordering | warning | |
| `look-at-before-tree` | Detect tree-dependent calls before `add_child()` | warning | |
| `duplicate-delegate` | Detect pure pass-through delegate functions | info | |
| `god-object` | Warn on classes with too many functions/members/lines | warning | |
| `magic-number` | Flag unexplained numeric literals | warning | |
| `max-file-lines` | Enforce maximum file length | warning | |
| `max-line-length` | Enforce maximum line length | warning | |
| `max-public-methods` | Enforce maximum public methods per class | warning | |
| `null-after-await` | Warn on member access after `await` without null guard | warning | |
| `print-statement` | Detect debug print calls | info | |
| `signal-not-connected` | Detect signals emitted but never connected | info | |
| `todo-comment` | Detect TODO/FIXME/HACK comments | info | |
| `unused-parameter` | Detect unused function parameters | warning | |
| `use-before-assign` | Detect method calls accessing uninitialized members | warning | |
| `variant-inference` | Warn on `:=` inferring Variant from dict/array access | warning | |

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
| `max_god_object_functions` | `20` | Max functions before `god-object` warns |
| `max_god_object_members` | `15` | Max member variables before `god-object` warns |
| `max_god_object_lines` | `500` | Max lines before `god-object` warns |
| `ignore_patterns` | `[]` | Glob patterns for files to skip |

**`[lint.rules.<name>]`** — per-rule overrides:

| Option | Values | Description |
|--------|--------|-------------|
| `severity` | `"info"`, `"warning"`, `"error"`, `"off"` | Override severity or disable a rule |

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
- **Hover** &mdash; type and documentation info on hover (with built-in Godot docs)
- **Go to definition** &mdash; jump to function and variable declarations
- **Find references** &mdash; find all usages across the project
- **Rename** &mdash; rename symbols across files with prepare-rename support
- **Completion** &mdash; context-aware autocomplete for symbols, builtins, and lifecycle methods
- **Godot proxy** &mdash; forwards hover, completion, and definition to Godot's built-in LSP (port 6005) when the editor is running, with `--godot-port` and `--no-godot-proxy` flags

### One-Shot CLI Queries

`gd lsp` also exposes one-shot subcommands that output JSON to stdout — designed for AI tools and scripting:

```sh
# Rename a symbol across the project (applies to disk by default)
gd lsp rename --file player.gd --line 5 --column 10 --new-name move_character

# Rename by name (project-wide search)
gd lsp rename --name old_func --new-name new_func

# Preview without writing
gd lsp rename --file player.gd --line 5 --column 10 --new-name move_character --dry-run

# Find all references to a symbol (by position)
gd lsp references --file player.gd --line 5 --column 10

# Find all references by name (project-wide search)
gd lsp references --name speed

# Go to definition
gd lsp definition --file player.gd --line 5 --column 10

# Hover information
gd lsp hover --file player.gd --line 5 --column 10

# List completions
gd lsp completions --file player.gd --line 5 --column 10

# Available code actions / quick fixes
gd lsp code-actions --file player.gd --line 5 --column 1

# Run diagnostics (same as gd lint --format json)
gd lsp diagnostics

# List symbols in a file
gd lsp symbols --file player.gd

# Filter symbols by kind
gd lsp symbols --file player.gd --kind function,signal
```

# View lines from a file (human-readable by default)
gd lsp view --file player.gd --range 10-20
gd lsp view --file player.gd --start-line 15 --context 3
gd lsp view --file player.gd --format json  # structured output for AI tools

# Scene info (nodes, resources, connections from a .tscn file)
gd lsp scene-info --file main.tscn
gd lsp scene-info --file main.tscn --nodes-only

# Create a new GDScript file with scaffolding
gd lsp create-file --file enemies/boss.gd --extends CharacterBody2D --class-name Boss

# Create a file with custom content (from stdin or --input-file)
echo 'extends Node2D' | gd lsp create-file --file utils/helper.gd
```

All positions are **1-based** (line 1, column 1 is the first character). Paths in output are relative to the project root with forward slashes.

### Refactoring Commands

`gd lsp` includes structural refactoring commands that output JSON and support `--dry-run`:

```sh
# Delete a symbol (fails if references exist, use --force to override)
gd lsp delete-symbol --file player.gd --name unused_func
gd lsp delete-symbol --file player.gd --name unused_func --force

# Delete multiple symbols at once
gd lsp bulk-delete-symbol --file player.gd --names "a,b,c"

# Move a symbol between files
gd lsp move-symbol --name helper --from utils.gd --to helpers.gd

# Move and update preload paths in callers
gd lsp move-symbol --name helper --from utils.gd --to helpers.gd --update-callers

# Extract code into a new function
gd lsp extract-method --file player.gd --start-line 10 --end-line 15 --name do_attack

# Extract symbols to a new file
gd lsp extract-class --file player.gd --symbols "speed,health,take_damage" --to stats.gd

# Inline a function at its call sites
gd lsp inline-method --file player.gd --line 5 --column 2

# Inline a pass-through delegate function
gd lsp inline-delegate --file player.gd --name attack

# Rename multiple symbols atomically
gd lsp bulk-rename --file player.gd --renames "speed:velocity,health:hp"

# Change function signature (add/remove/reorder/rename params)
gd lsp change-signature --file player.gd --name move \
  --add-param "speed: float = 1.0" --remove-param old_param --reorder "a,b,c"

# Extract an expression into a local variable
gd lsp introduce-variable --file player.gd --line 5 --column 10 --end-column 30 --name velocity

# Turn a hardcoded value into a parameter with default
gd lsp introduce-parameter --file player.gd --line 5 --column 10 --end-column 20 --name speed

# AST-aware editing (reads new content from stdin or --input-file)
echo -e '\tprint("hello")' | gd lsp replace-body --file player.gd --name _ready
echo 'func _process(delta):\n\tpass' | gd lsp insert --file player.gd --after _ready
echo 'var speed: float = 42.0' | gd lsp replace-symbol --file player.gd --name speed
echo '\t# replaced' | gd lsp edit-range --file player.gd --range 5-7

# Or use --input-file to avoid stdin pipe encoding issues (recommended on Windows)
gd lsp insert --file player.gd --after _ready --input-file /tmp/new_func.gd
```

All refactoring commands support `--dry-run` to preview changes without writing to disk.
Edit commands also support `--no-format` to skip auto-formatting, `--class` for inner class targets, and `--input-file` to read from a file instead of stdin.

### Editor Setup

**VS Code:** Download the `.vsix` from the [latest release](https://github.com/c2lt4r/gd/releases/latest), then install it with:

```sh
code --install-extension gd-gdscript-0.1.2.vsix
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
