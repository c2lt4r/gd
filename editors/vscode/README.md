# GDScript (gd) - VS Code Extension

GDScript language support for Visual Studio Code, powered by the [gd](../../README.md) toolchain.

## Prerequisites

Install the `gd` binary and ensure it is available on your `PATH`, or configure the path in settings.

## Features

- **Syntax highlighting** - TextMate grammar for GDScript
- **Diagnostics** - Real-time lint errors and warnings
- **Formatting** - Format documents via `gd fmt`
- **Code actions** - Quick fixes from `gd lint --fix`
- **Document symbols** - Outline view for classes, functions, variables, and signals
- **Hover** - Type information and documentation on hover
- **Go to definition** - Jump to symbol definitions
- **Find references** - Find all references to a symbol
- **Rename** - Rename symbols across files

## Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `gd.path` | `"gd"` | Path to the `gd` binary |
| `gd.lsp.enabled` | `true` | Enable the language server |

## Development

```sh
npm install
npm run compile
```

To package the extension:

```sh
npm run package
```
