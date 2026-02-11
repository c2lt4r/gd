import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext): void {
  const config = vscode.workspace.getConfiguration("gd");
  const enabled = config.get<boolean>("lsp.enabled", true);
  if (!enabled) {
    return;
  }

  const gdPath = config.get<string>("path", "gd");

  const serverOptions: ServerOptions = {
    command: gdPath,
    args: ["lsp"],
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "gdscript" }],
  };

  client = new LanguageClient(
    "gd-gdscript",
    "GDScript (gd)",
    serverOptions,
    clientOptions
  );

  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
