import * as vscode from "vscode";
import * as cp from "child_process";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;
let statusBar: vscode.StatusBarItem | undefined;
let outputChannel: vscode.OutputChannel | undefined;

function getConfig() {
  return vscode.workspace.getConfiguration("gd");
}

async function startClient(context: vscode.ExtensionContext): Promise<void> {
  const config = getConfig();
  const gdPath = config.get<string>("path", "gd");

  if (!outputChannel) {
    outputChannel = vscode.window.createOutputChannel("GDScript (gd)");
  }

  const serverOptions: ServerOptions = {
    command: gdPath,
    args: ["lsp"],
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "gdscript" }],
    outputChannel,
    traceOutputChannel: outputChannel,
  };

  client = new LanguageClient(
    "gd-gdscript",
    "GDScript (gd)",
    serverOptions,
    clientOptions
  );

  updateStatusBar("$(sync~spin) gd", "Starting language server...");

  try {
    await client.start();
    updateStatusBar("$(check) gd", "GDScript language server running");
  } catch (e) {
    updateStatusBar(
      "$(error) gd",
      "GDScript language server failed to start"
    );
    const msg = e instanceof Error ? e.message : String(e);
    outputChannel.appendLine(`Failed to start language server: ${msg}`);
    vscode.window.showErrorMessage(
      `GDScript language server failed to start. Is 'gd' installed and on your PATH?`
    );
  }
}

async function stopClient(): Promise<void> {
  if (client) {
    await client.stop();
    client = undefined;
  }
}

function updateStatusBar(text: string, tooltip: string): void {
  if (statusBar) {
    statusBar.text = text;
    statusBar.tooltip = tooltip;
  }
}

export function activate(context: vscode.ExtensionContext): void {
  const config = getConfig();
  const enabled = config.get<boolean>("lsp.enabled", true);

  // Status bar item — always visible when a .gd file is open
  statusBar = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    0
  );
  statusBar.command = "gd.restartLsp";
  context.subscriptions.push(statusBar);

  // Show status bar only when gdscript files are active
  const updateVisibility = () => {
    const editor = vscode.window.activeTextEditor;
    if (editor && editor.document.languageId === "gdscript") {
      statusBar?.show();
    } else {
      statusBar?.hide();
    }
  };
  context.subscriptions.push(
    vscode.window.onDidChangeActiveTextEditor(updateVisibility)
  );
  updateVisibility();

  // Register restart command
  context.subscriptions.push(
    vscode.commands.registerCommand("gd.restartLsp", async () => {
      outputChannel?.appendLine("Restarting language server...");
      updateStatusBar("$(sync~spin) gd", "Restarting language server...");
      await stopClient();
      await startClient(context);
    })
  );

  // Register format all command
  context.subscriptions.push(
    vscode.commands.registerCommand("gd.formatAll", async (uri?: vscode.Uri) => {
      const gdPath = getConfig().get<string>("path", "gd");
      const workspaceFolder = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
      const targetPath = uri?.fsPath ?? workspaceFolder;

      if (!targetPath) {
        vscode.window.showErrorMessage("No workspace folder open.");
        return;
      }

      if (!outputChannel) {
        outputChannel = vscode.window.createOutputChannel("GDScript (gd)");
      }

      await vscode.window.withProgress(
        {
          location: vscode.ProgressLocation.Notification,
          title: "Formatting GDScript files...",
          cancellable: false,
        },
        () =>
          new Promise<void>((resolve) => {
            cp.execFile(gdPath, ["fmt", targetPath], (err, stdout, stderr) => {
              if (stdout) {
                outputChannel!.appendLine(stdout);
              }
              if (stderr) {
                outputChannel!.appendLine(stderr);
              }

              if (err) {
                outputChannel!.show(true);
                vscode.window.showErrorMessage(
                  `gd fmt failed (exit code ${err.code}). See output channel for details.`
                );
              } else {
                vscode.window.showInformationMessage("GDScript files formatted.");
              }
              resolve();
            });
          })
      );
    })
  );

  if (!enabled) {
    updateStatusBar("$(circle-slash) gd", "GDScript language server disabled");
    return;
  }

  startClient(context);
}

export function deactivate(): Thenable<void> | undefined {
  statusBar?.dispose();
  outputChannel?.dispose();
  if (!client) {
    return undefined;
  }
  return client.stop();
}
