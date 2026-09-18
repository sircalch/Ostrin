const vscode = require('vscode');
const childProcess = require('child_process');
const path = require('path');

let diagnostics;

function compilerPath() {
  return vscode.workspace.getConfiguration('ostrin').get('compilerPath', 'ostrinc');
}

function currentDocument() {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== 'ostrin') {
    vscode.window.showWarningMessage('Open an Ostrin (.ostrin) file first.');
    return undefined;
  }
  return editor.document;
}

function diagnosticUri(document, file) {
  if (!file) return document.uri;
  return vscode.Uri.file(path.resolve(file));
}

function addDiagnostic(document, item) {
  const line = Number.isInteger(item.line) ? Math.max(0, item.line - 1) : 0;
  const column = Number.isInteger(item.column) ? Math.max(0, item.column - 1) : 0;
  const target = diagnosticUri(document, item.file);
  const range = new vscode.Range(
    new vscode.Position(line, column),
    new vscode.Position(line, Math.max(column + 1, column))
  );
  const code = item.code ? `OSTRIN-${item.code}` : 'OSTRIN';
  const diagnostic = new vscode.Diagnostic(
    range,
    item.message || 'Ostrin compiler error.',
    vscode.DiagnosticSeverity.Error
  );
  diagnostic.code = code;
  const previous = diagnostics.get(target) || [];
  diagnostics.set(target, [...previous, diagnostic]);
}

function consumeDiagnosticLine(document, line, output) {
  if (!line.trim()) return;
  try {
    const item = JSON.parse(line);
    if (item && item.severity === 'error') {
      addDiagnostic(document, item);
      output.appendLine(`${item.code ? `OSTRIN-${item.code}: ` : ''}${item.message}`);
      return;
    }
  } catch (_) {
    // Keep non-JSON compiler output visible for forward compatibility.
  }
  output.appendLine(line);
}

async function runCompiler(document, run, notify = true) {
  if (document.isUntitled) {
    vscode.window.showWarningMessage('Save the Ostrin file before running the compiler.');
    return;
  }

  if (document.isDirty) {
    await document.save();
  }

  const cwd = vscode.workspace.getWorkspaceFolder(document.uri)?.uri.fsPath
    ?? path.dirname(document.uri.fsPath);
  const executable = compilerPath();
  if (!run) diagnostics.delete(document.uri);
  const args = run ? ['--run', document.uri.fsPath] : ['--check', '--json', document.uri.fsPath];
  const output = vscode.window.createOutputChannel('Ostrin');
  const display = [executable, ...args].map((value) => JSON.stringify(value)).join(' ');
  output.appendLine(`> ${display}`);
  output.show(true);

  const child = childProcess.spawn(executable, args, {
    cwd,
    windowsHide: true,
    shell: false
  });

  let stdoutBuffer = '';
  child.stdout.on('data', (data) => {
    if (run) {
      output.append(data.toString());
      return;
    }
    stdoutBuffer += data.toString();
    const lines = stdoutBuffer.split(/\r?\n/);
    stdoutBuffer = lines.pop() || '';
    for (const line of lines) consumeDiagnosticLine(document, line, output);
  });
  child.stderr.on('data', (data) => output.append(data.toString()));
  child.on('error', (error) => {
    output.appendLine(`\\nCould not start ostrinc: ${error.message}`);
    vscode.window.showErrorMessage(`Ostrin compiler could not be started: ${error.message}`);
  });
  child.on('close', (code) => {
    if (!run && stdoutBuffer.trim()) consumeDiagnosticLine(document, stdoutBuffer, output);
    if (code === 0) {
      if (notify) vscode.window.showInformationMessage(run ? 'Ostrin program finished successfully.' : 'Ostrin check passed.');
    } else if (code !== null) {
      if (notify) vscode.window.showErrorMessage(`Ostrin ${run ? 'run' : 'check'} failed with exit code ${code}.`);
    }
  });
}

function activate(context) {
  diagnostics = vscode.languages.createDiagnosticCollection('ostrin');
  const check = vscode.commands.registerCommand('ostrin.check', async () => {
    const document = currentDocument();
    if (document) await runCompiler(document, false, true);
  });

  const run = vscode.commands.registerCommand('ostrin.run', async () => {
    const document = currentDocument();
    if (document) await runCompiler(document, true);
  });

  const saveSubscription = vscode.workspace.onDidSaveTextDocument(async (document) => {
    const enabled = vscode.workspace.getConfiguration('ostrin').get('checkOnSave', false);
    if (enabled && document.languageId === 'ostrin') {
      await runCompiler(document, false, false);
    }
  });

  context.subscriptions.push(diagnostics, check, run, saveSubscription);
}

function deactivate() {}

module.exports = { activate, deactivate };
