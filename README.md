# gd

**The Godot toolchain.** A fast, all-in-one CLI for formatting, linting, building, and managing Godot projects — like `cargo` for GDScript.

Built with [tree-sitter-gdscript](https://github.com/PrestonKnopp/tree-sitter-gdscript) and [tree-sitter-godot-resource](https://github.com/PrestonKnopp/tree-sitter-godot-resource) for accurate parsing, and [Rayon](https://github.com/rayon-rs/rayon) for parallel file processing.

## Features

- **Format** GDScript files with an AST-based formatter aligned to the [GDScript style guide](https://docs.godotengine.org/en/stable/tutorials/scripting/gdscript/gdscript_styleguide.html)
- **Lint** with 96 built-in rules (26 auto-fixable) plus compiler-level checks with singleton/cross-file/autoload awareness, SARIF output for CI
- **Run**, **build**, **test**, and **clean** your Godot project from the terminal
- **Watch** for file changes and auto-lint/format on save
- **Manage addons** from Git or the Godot Asset Library (with lockfile and update support)
- **Generate CI/CD** configurations for GitHub Actions and GitLab CI
- **LSP server** with formatting, diagnostics, hover, go-to-definition, references, rename, completion, inlay hints, signature help, call hierarchy, find implementations, semantic tokens, workspace symbol search, scene-aware cross-referencing, and Godot LSP proxy
- **Refactoring** (`gd refactor`) — 25 structural refactoring commands with undo support, collision warnings, type inference, and cross-file resolution
- **Code editing** (`gd edit`) — AST-safe editing primitives (replace, insert, remove, extract, create-file)
- **Code queries** (`gd query`) — one-shot code intelligence (references, hover, definition, symbols, completions, scene info, and more)
- **Scene management** — create scenes, add/remove/duplicate nodes, instance scenes, add sub-resources, batch-add nodes, set properties, wire connections, attach/detach scripts — plus validate `.tscn`/`.tres` files and visualize scene hierarchies
- **Debug** a running Godot game via Godot's binary debug protocol — breakpoints, stepping, variable inspection, expression evaluation, live scene tree, node inspection, game speed control, and hot-reload
- **Godot LSP proxy** — forward hover, completion, and go-to-definition to Godot's built-in LSP when the editor is running
- **Analyze** your project with dependency graphs, class trees, and code statistics

## Installation

```sh
git clone https://github.com/c2lt4r/gd.git
cd gd
cargo install --path .
```

## Quick Start

```sh
gd new my-game && cd my-game
gd fmt                # format all GDScript files
gd lint               # lint for issues
gd run                # run the project
gd watch              # watch for changes, auto-lint
```

See [Getting Started](docs/getting-started.md) for templates, editor setup, and more.

## Commands

| Command | Description |
|---------|-------------|
| `gd new <name>` | Create a new Godot project (templates: `default`, `2d`, `3d`, or `--from` GitHub) |
| `gd init` | Initialize gd toolchain in an existing project (detects export paths) |
| `gd fmt` | Format GDScript files |
| `gd lint` | Lint GDScript files |
| `gd run` | Run the Godot project |
| `gd stop` | Stop the running Godot game |
| `gd log` | View game output log (`--tail`, `--follow`, `--errors`, `--grep`) |
| `gd build` | Build/export the Godot project |
| `gd check` | Check project for errors (parse, structural, semantic, `.tscn`/`.tres`) |
| `gd parse` | Parse GDScript files (syntax errors only, no semantic analysis) |
| `gd clean` | Clean build artifacts |
| `gd eval` | Evaluate GDScript expressions or scripts |
| `gd test` | Run GDScript tests (GUT, gdUnit4, native, or raw scripts) |
| `gd completions` | Generate shell completions |
| `gd tree` | Show project class hierarchy (`--scene` for scene trees) |
| `gd doc` | Generate documentation from doc comments |
| `gd watch` | Watch files and run fmt/lint on changes |
| `gd addons` | Manage project addons (install, remove, search, update, lock) |
| `gd overview` | Show project architecture overview |
| `gd stats` | Show project statistics (`--diff`, `--by-dir`, `--top N`) |
| `gd ci` | Generate CI/CD pipeline configuration |
| `gd daemon` | Manage the background daemon |
| `gd debug` | Debug a running Godot game (breakpoints, stepping, eval, scene tree) |
| `gd resource` | Manage `.tres` resource files |
| `gd scene` | Manage `.tscn` scene files |
| `gd lsp` | Start the LSP server |
| `gd refactor` | Refactoring operations (rename, extract, inline, etc.) |
| `gd edit` | Code editing primitives (replace, insert, remove, extract, create-file) |
| `gd query` | Code intelligence queries (references, hover, definition, etc.) |
| `gd ssr` | Structural search and replace (`$placeholder` patterns, type constraints, dry-run) |
| `gd deps` | Show script dependency graph |
| `gd env` | Show environment info |
| `gd man` | Generate man page |
| `gd upgrade` | Self-update to latest release |
| `gd llm` | Print AI-readable command reference |

## Documentation

- [Getting Started](docs/getting-started.md) — installation, quick start, templates, editor setup
- [Command Reference](docs/commands.md) — all commands with usage examples
- [Configuration](docs/configuration.md) — `gd.toml` reference and lint rules
- [LSP Server](docs/lsp.md) — language server capabilities
- [CI/CD Integration](docs/ci.md) — GitHub Actions, GitLab CI, SARIF output

## License

MIT
