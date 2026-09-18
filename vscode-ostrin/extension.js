const vscode = require('vscode');
const childProcess = require('child_process');
const fs = require('fs');
const path = require('path');
const languageFeatures = require('./language-features');

let diagnostics;
const semanticIndex = new Map();
const semanticGenerations = new Map();

function samePath(left, right) {
  return left && right
    && left.replace(/\\/g, '/').toLowerCase() === right.replace(/\\/g, '/').toLowerCase();
}

function workspaceRootFor(document) {
  return vscode.workspace.getWorkspaceFolder(document.uri)?.uri.fsPath
    ?? path.dirname(document.uri.fsPath);
}

function invalidateSemanticIndex(document) {
  const key = document.uri.toString();
  const generation = (semanticGenerations.get(key) || 0) + 1;
  semanticGenerations.set(key, generation);
  semanticIndex.delete(key);
  return { key, generation };
}

function semanticIndexFor(document) {
  const merged = { symbols: [], members: [], bindings: [], expressions: [] };
  const seen = new Set();
  const add = (kind, item) => {
    if (!item) return;
    const key = [
      kind,
      item.name,
      item.owner,
      item.file,
      item.line,
      item.column,
      item.function,
      item.scopeDepth,
      item.type
    ].join('|');
    if (seen.has(key)) return;
    seen.add(key);
    merged[kind].push(item);
  };

  // The compiler index is refreshed per document. Merge the indexes here so
  // editor providers can resolve symbols in every currently opened Ostrin
  // document without turning every completion request into a compiler run.
  const targetRoot = workspaceRootFor(document);
  for (const index of semanticIndex.values()) {
    if (targetRoot && index.workspaceRoot && !samePath(targetRoot, index.workspaceRoot)) continue;
    for (const item of index.symbols || []) add('symbols', item);
    for (const item of index.members || []) add('members', item);
    for (const item of index.bindings || []) add('bindings', item);
    for (const item of index.expressions || []) add('expressions', item);
  }
  if (!merged.symbols.length && !merged.members.length && !merged.bindings.length && !merged.expressions.length) {
    return { symbols: [], members: [], bindings: [], expressions: [] };
  }
  return merged;
}

function compilerPath(document) {
  const configured = vscode.workspace.getConfiguration('ostrin').get('compilerPath', 'ostrinc');
  if (configured && configured !== 'ostrinc') return configured;

  const workspace = document
    ? vscode.workspace.getWorkspaceFolder(document.uri)?.uri.fsPath
    : undefined;
  if (workspace) {
    const executable = process.platform === 'win32' ? 'ostrinc.exe' : 'ostrinc';
    const candidates = [
      path.join(workspace, 'compiler', 'target', 'debug', executable),
      path.join(workspace, 'compiler', 'target', 'release', executable),
      path.join(workspace, 'target', 'debug', executable),
      path.join(workspace, 'target', 'release', executable)
    ];
    const local = candidates.find((candidate) => fs.existsSync(candidate));
    if (local) return local;
  }
  return configured || 'ostrinc';
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

function refreshSemanticIndex(document) {
  if (document.isUntitled) return;
  const { key, generation } = invalidateSemanticIndex(document);
  const cwd = workspaceRootFor(document);
  const found = { symbols: [], members: [], bindings: [], expressions: [], workspaceRoot: cwd };
  const addSemanticItem = (item) => {
    if (!item || (!item.name && item.kind !== 'expression')) return;
    if (item.kind === 'member') found.members.push({ ...item, kind: item.memberKind || 'method' });
    else if (item.kind === 'binding') found.bindings.push(item);
    else if (item.kind === 'expression') found.expressions.push(item);
    else if (item.kind) found.symbols.push(item);
  };
  let pending = 3;
  let successful = true;
  const finish = () => {
    pending -= 1;
    if (pending === 0 && successful && semanticGenerations.get(key) === generation) {
      semanticIndex.set(key, found);
    }
  };
  for (const mode of ['members', 'symbols', 'types']) {
    const child = childProcess.spawn(compilerPath(document), [`--${mode}`, '--json', document.uri.fsPath], {
      cwd,
      windowsHide: true,
      shell: false
    });
    let buffer = '';
    let settled = false;
    const settle = (ok) => {
      if (settled) return;
      settled = true;
      if (!ok) successful = false;
      finish();
    };
    child.on('error', () => {
      // The normal compiler check reports startup failures to the user. The
      // background symbol index must stay silent when the compiler is absent.
      settle(false);
    });
    child.stdout.on('data', (data) => {
      buffer += data.toString();
      const lines = buffer.split(/\r?\n/);
      buffer = lines.pop() || '';
      for (const line of lines) {
        try {
          const item = JSON.parse(line);
          addSemanticItem(item);
        } catch (_) {
          // Ignore incomplete or diagnostic output from an older compiler.
        }
      }
    });
    child.on('close', (code) => {
      if (buffer.trim()) {
        try {
          const item = JSON.parse(buffer);
          addSemanticItem(item);
        } catch (_) {}
      }
      settle(code === 0);
    });
  }
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
  const executable = compilerPath(document);
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
  const selector = { language: 'ostrin', scheme: 'file' };
  const completion = vscode.languages.registerCompletionItemProvider(
    selector,
    { provideCompletionItems: (document, position) => languageFeatures.provideCompletionItems(vscode, semanticIndexFor(document), document, position) },
    '.', ':'
  );
  const hover = vscode.languages.registerHoverProvider(selector, {
    provideHover: (document, position) => languageFeatures.provideHover(vscode, document, position, semanticIndexFor(document))
  });
  const definitions = vscode.languages.registerDefinitionProvider(selector, {
    provideDefinition: (document, position) => languageFeatures.provideDefinition(vscode, document, position, semanticIndexFor(document))
  });
  const references = vscode.languages.registerReferenceProvider(selector, {
    provideReferences: (document, position, context) => languageFeatures.provideReferences(vscode, document, position, context, semanticIndexFor(document))
  });
  const rename = vscode.languages.registerRenameProvider(selector, {
    provideRenameEdits: (document, position, newName) => languageFeatures.provideRenameEdits(vscode, document, position, newName, semanticIndexFor(document))
  });
  const symbols = vscode.languages.registerDocumentSymbolProvider(selector, {
    provideDocumentSymbols: (document) => languageFeatures.provideDocumentSymbols(vscode, document, semanticIndexFor(document))
  });
  const formatting = vscode.languages.registerDocumentFormattingEditProvider(selector, {
    provideDocumentFormattingEdits: (document) => languageFeatures.provideDocumentFormattingEdits(vscode, document)
  });
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
    if (document.languageId === 'ostrin') refreshSemanticIndex(document);
  });
  const changeSubscription = vscode.workspace.onDidChangeTextDocument((event) => {
    if (event.document.languageId === 'ostrin') invalidateSemanticIndex(event.document);
  });
  const closeSubscription = vscode.workspace.onDidCloseTextDocument((document) => {
    if (document.languageId === 'ostrin') invalidateSemanticIndex(document);
  });

  const openSubscription = vscode.workspace.onDidOpenTextDocument((document) => {
    if (document.languageId === 'ostrin') refreshSemanticIndex(document);
  });
  for (const editor of vscode.window.visibleTextEditors) {
    if (editor.document.languageId === 'ostrin') refreshSemanticIndex(editor.document);
  }

  context.subscriptions.push(diagnostics, completion, hover, definitions, references, rename, symbols, formatting, check, run, saveSubscription, changeSubscription, closeSubscription, openSubscription);
}

function deactivate() {}

module.exports = { activate, deactivate };
