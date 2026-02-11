# GDScript (gd) - VS Code Extension

GDScript language support for Visual Studio Code, powered by the [gd](https://github.com/c2lt4r/gd) toolchain.

## Prerequisites

Install the `gd` binary and ensure it is available on your `PATH`, or configure the path in settings.

## Features

- **Syntax highlighting** - TextMate grammar for GDScript
- **Diagnostics** - Real-time lint errors and warnings across your entire project
- **Format on save** - Automatically format `.gd` files on save (enabled by default)
- **Formatting** - Format documents via `gd fmt`
- **Code actions** - Quick fixes from `gd lint --fix`
- **Document symbols** - Outline view for classes, functions, variables, and signals
- **Hover** - Type information and documentation on hover
- **Go to definition** - Jump to symbol definitions
- **Find references** - Find all references to a symbol
- **Rename** - Rename symbols across files
- **Completions** - Autocomplete for symbols, builtins, and lifecycle methods
- **Folding** - `# region` / `# endregion` markers for custom fold regions

## Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `gd.path` | `"gd"` | Path to the `gd` binary |
| `gd.lsp.enabled` | `true` | Enable the language server |
| `gd.trace.server` | `"off"` | Trace LSP communication (`"off"`, `"messages"`, `"verbose"`) |

### Editor Defaults

The extension sets these defaults for GDScript files. You can override them in your `settings.json`:

| Setting | Default | Description |
|---------|---------|-------------|
| `editor.formatOnSave` | `true` | Format on save |
| `editor.defaultFormatter` | `c2lt4r.gd-gdscript` | Use gd as the formatter |
| `editor.insertSpaces` | `false` | Use tabs (GDScript convention) |
| `editor.tabSize` | `4` | 4-space tab width |
| `files.trimTrailingWhitespace` | `true` | Trim trailing whitespace |
| `files.insertFinalNewline` | `true` | Ensure files end with a newline |

To disable format on save for GDScript:

```json
"[gdscript]": {
  "editor.formatOnSave": false
}
```

### Format Rules

Formatting rules are configured in `gd.toml` at the project root:

```toml
[fmt]
use_tabs = true                    # Use tabs (default: true)
indent_size = 4                    # Spaces per indent level (default: 4)
max_line_length = 100              # Warn on long lines (default: 100)
blank_lines_around_functions = 2   # Blank lines around functions (default: 2)
blank_lines_around_classes = 2     # Blank lines around classes (default: 2)
```

## Commands

| Command | Description |
|---------|-------------|
| `GDScript: Restart Language Server` | Restart the gd language server |

## Development

```sh
npm install
npm run compile
```

To package the extension:

```sh
npm run package
```
