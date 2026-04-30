import * as vscode from 'vscode';
import * as path from 'path';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: vscode.ExtensionContext) {
  // Get the workspace root
  const workspaceFolders = vscode.workspace.workspaceFolders;
  if (!workspaceFolders) {
    vscode.window.showErrorMessage('No workspace folder found');
    return;
  }

  const workspaceRoot = workspaceFolders[0].uri.fsPath;

  // Server options: spawn cargo run --bin aether-lsp
  const serverOptions: ServerOptions = {
    command: 'cargo',
    args: ['run', '--bin', 'aether-lsp'],
    options: {
      cwd: workspaceRoot,
      stdio: 'pipe'
    }
  };

  // Client options
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'aether' }],
    stdioTransport: true
  };

  // Create and start the language client
  client = new LanguageClient(
    'aetherLanguageServer',
    'Aether Language Server',
    serverOptions,
    clientOptions
  );

  // Start the client
  client.start().then(() => {
    console.log('Aether Language Server started');
  }).catch((error) => {
    console.error('Failed to start Aether Language Server:', error);
    vscode.window.showErrorMessage(`Failed to start Aether Language Server: ${error}`);
  });
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
