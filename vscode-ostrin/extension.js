const vscode = require('vscode');
const childProcess = require('child_process');
const path = require('path');

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

async function runCompiler(document, run) {
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
  const args = run ? ['--run', document.uri.fsPath] : [document.uri.fsPath];
  const output = vscode.window.createOutputChannel('Ostrin');
  const display = [executable, ...args].map((value) => JSON.stringify(value)).join(' ');
  output.appendLine(`> ${display}`);
  output.show(true);

  const child = childProcess.spawn(executable, args, {
    cwd,
    windowsHide: true,
    shell: false
  });

  child.stdout.on('data', (data) => output.append(data.toString()));
  child.stderr.on('data', (data) => output.append(data.toString()));
  child.on('error', (error) => {
    output.appendLine(`\\nCould not start ostrinc: ${error.message}`);
    vscode.window.showErrorMessage(`Ostrin compiler could not be started: ${error.message}`);
  });
  child.on('close', (code) => {
    if (code === 0) {
      vscode.window.showInformationMessage(run ? 'Ostrin program finished successfully.' : 'Ostrin check passed.');
    } else if (code !== null) {
      vscode.window.showErrorMessage(`Ostrin ${run ? 'run' : 'check'} failed with exit code ${code}.`);
    }
  });
}

function activate(context) {
  const check = vscode.commands.registerCommand('ostrin.check', async () => {
    const document = currentDocument();
    if (document) await runCompiler(document, false);
  });

  const run = vscode.commands.registerCommand('ostrin.run', async () => {
    const document = currentDocument();
    if (document) await runCompiler(document, true);
  });

  const saveSubscription = vscode.workspace.onDidSaveTextDocument(async (document) => {
    const enabled = vscode.workspace.getConfiguration('ostrin').get('checkOnSave', false);
    if (enabled && document.languageId === 'ostrin') {
      await runCompiler(document, false);
    }
  });

  context.subscriptions.push(check, run, saveSubscription);
}

function deactivate() {}

module.exports = { activate, deactivate };
