# LSP Server

`gd lsp` starts a Language Server Protocol server over stdio, providing editor integration with:

- **Diagnostics** — real-time lint warnings and errors
- **Formatting** — format documents on save
- **Code actions** — quick fixes for lint issues
- **Document symbols** — outline of classes, functions, signals, and variables
- **Hover** — type and documentation info on hover (with built-in Godot docs, cross-file resolution with origin class)
- **Go to definition** — jump to function and variable declarations (indexed cross-file lookup)
- **Find references** — find all usages across the project
- **Rename** — rename symbols across files with prepare-rename support
- **Completion** — context-aware autocomplete for symbols, builtins, and lifecycle methods
- **Inlay hints** — ghost text showing inferred types for `:=` variables and parameter names at call sites
- **Signature help** — function signature with active parameter highlight as you type
- **Call hierarchy** — incoming and outgoing calls for any function
- **Find implementations** — find all subclasses and method overrides
- **Semantic tokens** — type-aware syntax highlighting (classes, enums, functions, signals)
- **Workspace symbol search** — fuzzy search across all project symbols (Ctrl+T)
- **Godot proxy** — forwards hover, completion, and definition to Godot's built-in LSP (port 6005) when the editor is running, with `--godot-port` and `--no-godot-proxy` flags

## Editor Setup

See [Getting Started — Editor Setup](getting-started.md#editor-setup) for VS Code and Neovim configuration.
