use crate::cprintln;
use clap::{Args, Subcommand};
use miette::Result;

#[derive(Args)]
pub struct QueryArgs {
    #[command(subcommand)]
    pub command: QueryCommand,
    /// Disable proxy to Godot's built-in LSP server
    #[arg(long)]
    pub no_godot_proxy: bool,
}

#[derive(Subcommand)]
pub enum QueryCommand {
    /// Find all references to a symbol
    References {
        /// Search by symbol name across the project (alternative to --file/--line/--column)
        #[arg(long)]
        name: Option<String>,
        /// Path to the GDScript file
        #[arg(long)]
        file: Option<String>,
        /// Line number (1-based)
        #[arg(long)]
        line: Option<usize>,
        /// Column number (1-based)
        #[arg(long)]
        column: Option<usize>,
        /// Filter results to a specific class (requires --name)
        #[arg(long, requires = "name")]
        class: Option<String>,
        /// Output format: json or human (default: human)
        #[arg(long, default_value = "human")]
        format: String,
    },
    /// Go to the definition of a symbol
    Definition {
        #[command(flatten)]
        pos: QueryPositionArgs,
        /// Output format: json or human (default: human)
        #[arg(long, default_value = "human")]
        format: String,
    },
    /// Show hover information for a symbol
    Hover {
        #[command(flatten)]
        pos: QueryPositionArgs,
        /// Output format: json or human (default: human)
        #[arg(long, default_value = "human")]
        format: String,
    },
    /// List completions at a position
    Completions {
        #[command(flatten)]
        pos: QueryPositionArgs,
        /// Maximum number of results to return
        #[arg(long)]
        limit: Option<usize>,
        /// Filter by kind (e.g. function, method, variable, property, constant, class, enum, enum_member, event, keyword)
        #[arg(long)]
        kind: Option<String>,
        /// Output format: json or human (default: human)
        #[arg(long, default_value = "human")]
        format: String,
    },
    /// List available code actions at a position
    CodeActions {
        #[command(flatten)]
        pos: QueryPositionArgs,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// List symbols in a file
    Symbols {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Filter by symbol kind (repeatable, comma-separated: function, method, variable, class, constant, enum, event; aliases: field/property = variable+field)
        #[arg(long)]
        kind: Vec<String>,
        /// Output format: json or human (default: human)
        #[arg(long, default_value = "human")]
        format: String,
    },
    /// View lines from a GDScript file (with optional line range)
    View {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Line range as START-END (e.g. 5-20; 1-based, inclusive)
        #[arg(long, conflicts_with_all = ["start_line", "end_line"])]
        range: Option<String>,
        /// First line to show (1-based, inclusive; default: 1)
        #[arg(long)]
        start_line: Option<usize>,
        /// Last line to show (1-based, inclusive; default: end of file)
        #[arg(long)]
        end_line: Option<usize>,
        /// Number of context lines around start_line/end_line
        #[arg(long)]
        context: Option<usize>,
        /// Output format: json for structured output (default: human-readable)
        #[arg(long)]
        format: Option<String>,
    },
    /// Show scene structure from a .tscn file (nodes, resources, connections)
    SceneInfo {
        /// Path to the .tscn file
        #[arg(long)]
        file: String,
        /// Show only nodes (compact output)
        #[arg(long)]
        nodes_only: bool,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// List all scenes that reference a GDScript file
    SceneRefs {
        /// Path to the .gd file
        #[arg(long)]
        file: String,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// List all signal connections targeting handler functions in a script
    SignalConnections {
        /// Path to the .gd file
        #[arg(long)]
        file: String,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Find all classes that implement (define) a given method
    FindImplementations {
        /// Method name to search for
        #[arg(long)]
        name: String,
        /// Only include classes extending this type
        #[arg(long)]
        base: Option<String>,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
}

#[derive(Args)]
pub struct QueryPositionArgs {
    /// Path to the GDScript file
    #[arg(long)]
    pub file: String,
    /// Line number (1-based)
    #[arg(long)]
    pub line: usize,
    /// Column number (1-based)
    #[arg(long)]
    pub column: usize,
}

fn print_references_human(result: &crate::lsp::query::ReferencesOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "{} ({} reference{})",
        result.symbol.bold(),
        result.references.len(),
        if result.references.len() == 1 {
            ""
        } else {
            "s"
        }
    );
    for r in &result.references {
        cprintln!(
            "  {}:{}:{}  {}",
            r.file.cyan(),
            r.line,
            r.column,
            r.context.dimmed()
        );
    }
}

fn print_definition_human(result: &crate::lsp::query::DefinitionOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "{} {} {}:{}:{}",
        result.symbol.bold(),
        "→".dimmed(),
        result.file.cyan(),
        result.line,
        result.column
    );
}

fn print_definition_from_json(val: &serde_json::Value) {
    use owo_colors::OwoColorize;
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("?");
    let file = val.get("file").and_then(|v| v.as_str()).unwrap_or("?");
    let line = val
        .get("line")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let col = val
        .get("column")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    cprintln!(
        "{} {} {}:{line}:{col}",
        symbol.bold(),
        "→".dimmed(),
        file.cyan()
    );
}

fn print_completions_human(items: &[crate::lsp::query::CompletionOutput]) {
    use owo_colors::OwoColorize;
    if items.is_empty() {
        cprintln!("  (no completions)");
        return;
    }
    let max_kind = items.iter().map(|s| s.kind.len()).max().unwrap_or(0);
    let max_label = items.iter().map(|s| s.label.len()).max().unwrap_or(0);

    for item in items {
        let kind_colored = color_kind(&item.kind, max_kind);
        let detail = item
            .detail
            .as_deref()
            .map_or(String::new(), |d| format!("  {}", d.dimmed()));
        cprintln!(
            "  {kind_colored}  {:width$}{detail}",
            item.label.bold(),
            width = max_label,
        );
    }
}

fn color_kind(kind: &str, width: usize) -> String {
    use owo_colors::OwoColorize;
    let padded = format!("{kind:width$}");
    match kind {
        "function" | "method" => padded.cyan().to_string(),
        "constant" => padded.yellow().to_string(),
        "variable" | "property" => padded.blue().to_string(),
        "event" => padded.magenta().to_string(),
        "enum" | "enum_member" => padded.green().to_string(),
        "class" => padded.red().to_string(),
        "keyword" => padded.dimmed().to_string(),
        _ => padded,
    }
}

fn print_symbols_human(symbols: &[crate::lsp::query::SymbolOutput]) {
    use owo_colors::OwoColorize;
    if symbols.is_empty() {
        cprintln!("  (no symbols)");
        return;
    }
    let max_kind = symbols.iter().map(|s| s.kind.len()).max().unwrap_or(0);
    let max_name = symbols.iter().map(|s| s.name.len()).max().unwrap_or(0);

    for s in symbols {
        let kind_colored = color_kind(&s.kind, max_kind);
        let detail = s
            .detail
            .as_deref()
            .map_or(String::new(), |d| format!("  {}", d.dimmed()));
        cprintln!(
            "  {kind_colored}  {:width$}  L{}{detail}",
            s.name.bold(),
            s.line,
            width = max_name,
        );
    }
}

fn print_code_actions_human(actions: &[crate::lsp::query::CodeActionOutput]) {
    use owo_colors::OwoColorize;
    if actions.is_empty() {
        cprintln!("  (no code actions)");
        return;
    }
    for (i, action) in actions.iter().enumerate() {
        cprintln!(
            "  {}. {}",
            (i + 1).to_string().dimmed(),
            action.title.bold()
        );
    }
}

fn print_scene_info_human(r: &crate::lsp::query::SceneInfoOutput) {
    use owo_colors::OwoColorize;
    cprintln!("{}", r.file.bold());

    if let Some(ref nodes) = r.nodes {
        for (i, n) in nodes.iter().enumerate() {
            let depth = match n.parent.as_deref() {
                None => 0,
                Some(".") => 1,
                Some(p) => p.chars().filter(|&c| c == '/').count() + 2,
            };

            let indent = if depth == 0 {
                String::new()
            } else {
                let is_last = nodes.get(i + 1).is_none_or(|next| {
                    let nd = match next.parent.as_deref() {
                        None => 0,
                        Some(".") => 1,
                        Some(p) => p.chars().filter(|&c| c == '/').count() + 2,
                    };
                    nd <= depth
                });
                let connector = if is_last { "└── " } else { "├── " };
                format!("{}{}", "│   ".repeat(depth.saturating_sub(1)), connector)
            };

            let type_part = n
                .r#type
                .as_deref()
                .map_or_else(String::new, |t: &str| format!(" ({})", t.dimmed()));
            let script_part = n
                .script
                .as_deref()
                .map_or_else(String::new, |s: &str| format!(" [{}]", s.cyan()));
            let groups_part = if n.groups.is_empty() {
                String::new()
            } else {
                format!(" {{{}}}", n.groups.join(", ").dimmed())
            };

            cprintln!(
                "{indent}{}{type_part}{script_part}{groups_part}",
                n.name.bold()
            );
        }
    }

    if let Some(ref connections) = r.connections
        && !connections.is_empty()
    {
        cprintln!("\n{}:", "Connections".bold());
        for c in connections {
            cprintln!(
                "  {} {} {} {} {}",
                c.from.cyan(),
                format!(".{}", c.signal).dimmed(),
                "→".dimmed(),
                c.to.cyan(),
                format!(".{}", c.method).dimmed()
            );
        }
    }
}

fn print_scene_refs_human(refs: &[crate::lsp::query::SceneRefOutput]) {
    use owo_colors::OwoColorize;
    if refs.is_empty() {
        cprintln!("  (no scenes reference this script)");
        return;
    }
    cprintln!(
        "{} scene{}:",
        refs.len(),
        if refs.len() == 1 { "" } else { "s" }
    );
    for r in refs {
        let type_part = r
            .node_type
            .as_deref()
            .map_or_else(String::new, |t| format!(" ({})", t.dimmed()));
        cprintln!("  {}  node: {}{}", r.scene.cyan(), r.node.bold(), type_part,);
    }
}

fn print_signal_connections_human(conns: &[crate::lsp::query::SignalConnectionOutput]) {
    use owo_colors::OwoColorize;
    if conns.is_empty() {
        cprintln!("  (no signal connections)");
        return;
    }
    cprintln!(
        "{} connection{}:",
        conns.len(),
        if conns.len() == 1 { "" } else { "s" }
    );
    for c in conns {
        cprintln!(
            "  {} {} {} {} {}  [{}]",
            c.from_node.cyan(),
            format!(".{}", c.signal).dimmed(),
            "→".dimmed(),
            c.to_node.cyan(),
            format!(".{}", c.method).dimmed(),
            c.scene.dimmed(),
        );
    }
}

fn print_find_implementations_human(r: &crate::lsp::query::ImplementationsOutput) {
    use owo_colors::OwoColorize;
    let count = r.implementations.len();
    cprintln!(
        "{} ({} implementation{})",
        r.method.bold(),
        count,
        if count == 1 { "" } else { "s" }
    );
    for entry in &r.implementations {
        let class_info = match (&entry.class_name, &entry.extends) {
            (Some(cn), Some(ext)) => format!("  {} extends {}", cn.green(), ext.dimmed()),
            (Some(cn), None) => format!("  {}", cn.green()),
            (None, Some(ext)) => format!("  extends {}", ext.dimmed()),
            (None, None) => String::new(),
        };
        cprintln!("  {}:{}{}", entry.file.cyan(), entry.line, class_info);
    }
}

fn is_json(format: Option<&String>) -> bool {
    format.map(String::as_str) == Some("json")
}

/// Parse a range string like "5-20" into (start, end) line numbers.
fn parse_range(range: &str) -> Result<(usize, usize)> {
    let parts: Vec<&str> = range.splitn(2, '-').collect();
    if parts.len() != 2 {
        return Err(miette::miette!(
            "invalid range '{range}' — expected START-END (e.g. 5-20)"
        ));
    }
    let start: usize = parts[0]
        .parse()
        .map_err(|_| miette::miette!("invalid start line in range: '{}'", parts[0]))?;
    let end: usize = parts[1]
        .parse()
        .map_err(|_| miette::miette!("invalid end line in range: '{}'", parts[1]))?;
    if start == 0 || end == 0 {
        return Err(miette::miette!("line numbers are 1-based"));
    }
    if start > end {
        return Err(miette::miette!(
            "start ({start}) must be <= end ({end}) in range"
        ));
    }
    Ok((start, end))
}

#[allow(clippy::too_many_lines)]
pub fn exec(args: QueryArgs) -> Result<()> {
    match args.command {
        QueryCommand::References {
            name,
            file,
            line,
            column,
            class,
            format,
        } => {
            let results: Vec<crate::lsp::query::ReferencesOutput> = if let Some(ref name) = name {
                // Support comma-separated names: --name "foo,bar,baz"
                let names: Vec<&str> = name
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect();
                let mut all = Vec::with_capacity(names.len());
                for n in &names {
                    all.push(crate::lsp::query::query_references_by_name(
                        n,
                        file.as_deref(),
                        class.as_deref(),
                    )?);
                }
                all
            } else {
                let file = file
                    .ok_or_else(|| miette::miette!("--file is required when not using --name"))?;
                let line = line
                    .ok_or_else(|| miette::miette!("--line is required when not using --name"))?;
                let column = column
                    .ok_or_else(|| miette::miette!("--column is required when not using --name"))?;
                vec![crate::lsp::query::query_references(&file, line, column)?]
            };
            if format == "json" {
                // Single symbol: unwrap for backward compat; multi: array
                let j = if results.len() == 1 {
                    serde_json::to_string_pretty(&results[0])
                } else {
                    serde_json::to_string_pretty(&results)
                }
                .map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{j}");
            } else {
                for result in &results {
                    print_references_human(result);
                }
            }
            Ok(())
        }
        QueryCommand::Definition { pos, format } => {
            // Try daemon for rich Godot results
            if !args.no_godot_proxy
                && let Some(result) = crate::lsp::daemon_client::query_daemon(
                    "definition",
                    serde_json::json!({"file": pos.file, "line": pos.line, "column": pos.column}),
                    None,
                )
            {
                if format == "json" {
                    cprintln!(
                        "{}",
                        serde_json::to_string_pretty(&result)
                            .map_err(|e| miette::miette!("{e}"))?
                    );
                } else {
                    print_definition_from_json(&result);
                }
                return Ok(());
            }
            // Fallback: static analysis only
            let result =
                crate::lsp::query::query_definition(&pos.file, pos.line, pos.column, None)?;
            if format == "json" {
                let j =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{j}");
            } else {
                print_definition_human(&result);
            }
            Ok(())
        }
        QueryCommand::Hover { pos, format } => {
            // Try daemon for rich Godot results
            if !args.no_godot_proxy
                && let Some(result) = crate::lsp::daemon_client::query_daemon(
                    "hover",
                    serde_json::json!({"file": pos.file, "line": pos.line, "column": pos.column}),
                    None,
                )
            {
                if format == "json" {
                    cprintln!(
                        "{}",
                        serde_json::to_string_pretty(&result)
                            .map_err(|e| miette::miette!("{e}"))?
                    );
                } else if let Some(content) = result.get("content").and_then(|v| v.as_str()) {
                    cprintln!("{content}");
                }
                return Ok(());
            }
            // Fallback: static analysis only
            let result = crate::lsp::query::query_hover(&pos.file, pos.line, pos.column, None)?;
            if format == "json" {
                let j =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{j}");
            } else {
                cprintln!("{}", result.content);
            }
            Ok(())
        }
        QueryCommand::Completions {
            pos,
            limit,
            kind,
            format,
        } => {
            // Try daemon for rich Godot results
            if !args.no_godot_proxy
                && let Some(result) = crate::lsp::daemon_client::query_daemon(
                    "completion",
                    serde_json::json!({"file": pos.file, "line": pos.line, "column": pos.column}),
                    None,
                )
            {
                let mut items: Vec<serde_json::Value> = if let Some(arr) = result.as_array() {
                    arr.clone()
                } else {
                    vec![result]
                };
                if let Some(ref filter) = kind {
                    items.retain(|v| {
                        v.get("kind")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|k| k == filter)
                    });
                }
                if let Some(n) = limit {
                    items.truncate(n);
                }
                cprintln!(
                    "{}",
                    serde_json::to_string_pretty(&items).map_err(|e| miette::miette!("{e}"))?
                );
                return Ok(());
            }
            // Fallback: static analysis only
            let mut result =
                crate::lsp::query::query_completions(&pos.file, pos.line, pos.column, None)?;
            if let Some(ref filter) = kind {
                result.retain(|c| c.kind == *filter);
            }
            if let Some(n) = limit {
                result.truncate(n);
            }
            if format == "json" {
                let j =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{j}");
            } else {
                print_completions_human(&result);
            }
            Ok(())
        }
        QueryCommand::CodeActions { pos, format } => {
            let result = crate::lsp::query::query_code_actions(&pos.file, pos.line, pos.column)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_code_actions_human(&result);
            }
            Ok(())
        }
        QueryCommand::Symbols { file, kind, format } => {
            let mut result = crate::lsp::query::query_symbols(&file)?;
            let kind_filter: Vec<String> = kind
                .iter()
                .flat_map(|s| s.split(',').map(|k| k.trim().to_lowercase()))
                // "field" and "property" are aliases for "variable" + "field"
                .flat_map(|k| match k.as_str() {
                    "field" | "property" => vec!["variable".to_string(), "field".to_string()],
                    other => vec![other.to_string()],
                })
                .collect();
            if !kind_filter.is_empty() {
                result.retain(|s| kind_filter.iter().any(|k| k == &s.kind));
            }
            if format == "json" {
                let j =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{j}");
            } else {
                print_symbols_human(&result);
            }
            Ok(())
        }
        QueryCommand::View {
            file,
            range,
            start_line,
            end_line,
            context,
            format,
        } => {
            let (start_line, end_line) = if let Some(ref r) = range {
                let (s, e) = parse_range(r)?;
                (Some(s), Some(e))
            } else {
                (start_line, end_line)
            };
            let result = crate::lsp::query::query_view(&file, start_line, end_line, context)?;
            if format.as_deref() == Some("json") {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                // Human-readable output (cat -n style)
                let width = if result.end_line > 0 {
                    result.end_line.to_string().len()
                } else {
                    1
                };
                for (i, line) in result.content.lines().enumerate() {
                    let line_num = result.start_line as usize + i;
                    cprintln!("{line_num:>width$}\t{line}");
                }
            }
            Ok(())
        }
        QueryCommand::SceneInfo {
            file,
            nodes_only,
            format,
        } => {
            let result = crate::lsp::query::query_scene_info(&file, nodes_only)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_scene_info_human(&result);
            }
            Ok(())
        }
        QueryCommand::SceneRefs { file, format } => {
            let result = crate::lsp::query::query_scene_refs(&file)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_scene_refs_human(&result);
            }
            Ok(())
        }
        QueryCommand::SignalConnections { file, format } => {
            let result = crate::lsp::query::query_signal_connections(&file)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_signal_connections_human(&result);
            }
            Ok(())
        }
        QueryCommand::FindImplementations { name, base, format } => {
            let result = crate::lsp::query::query_find_implementations(&name, base.as_deref())?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_find_implementations_human(&result);
            }
            Ok(())
        }
    }
}
