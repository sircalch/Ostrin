const childProcess = require('child_process');

class OstrinLanguageClient {
  constructor(command, cwd, rootUri, onDiagnostics, onError) {
    this.command = command;
    this.cwd = cwd;
    this.rootUri = rootUri;
    this.onDiagnostics = onDiagnostics;
    this.onError = onError;
    this.process = undefined;
    this.buffer = Buffer.alloc(0);
    this.nextRequestId = 1;
    this.pending = new Map();
    this.ready = false;
  }

  start() {
    if (this.process) return Promise.reject(new Error('Ostrin language server is already running.'));
    this.process = childProcess.spawn(this.command, ['--lsp'], {
      cwd: this.cwd,
      windowsHide: true,
      shell: false
    });
    this.process.stdout.on('data', (data) => this.consume(data));
    this.process.stderr.on('data', (data) => {
      if (this.onError) this.onError(data.toString());
    });
    const startup = new Promise((resolve, reject) => {
      this.startResolve = resolve;
      this.startReject = reject;
    });
    this.process.on('error', (error) => {
      if (this.startReject) this.startReject(error);
      if (this.onError) this.onError(error.message);
      this.disposeProcess();
    });
    this.process.on('close', () => {
      if (!this.ready && this.startReject) {
        this.startReject(new Error('Ostrin language server exited before initialization.'));
      }
      this.disposeProcess();
    });
    this.sendRequest('initialize', {
      processId: process.pid,
      rootUri: this.rootUri,
      capabilities: {}
    }).then(() => {
      this.ready = true;
      this.sendNotification('initialized', {});
      if (this.startResolve) this.startResolve();
    }).catch((error) => {
      if (this.startReject) this.startReject(error);
    });
    return startup;
  }

  sendNotification(method, params) {
    if (!this.process || !this.process.stdin.writable) return;
    this.write({ jsonrpc: '2.0', method, params });
  }

  sendRequest(method, params) {
    if (!this.process || !this.process.stdin.writable) {
      return Promise.reject(new Error('Ostrin language server is not running.'));
    }
    const id = this.nextRequestId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.write({ jsonrpc: '2.0', id, method, params });
    });
  }

  didOpen(document) {
    this.sendNotification('textDocument/didOpen', {
      textDocument: {
        uri: document.uri.toString(),
        languageId: document.languageId,
        version: document.version,
        text: document.getText()
      }
    });
  }

  didChange(document) {
    this.sendNotification('textDocument/didChange', {
      textDocument: { uri: document.uri.toString(), version: document.version },
      contentChanges: [{ text: document.getText() }]
    });
  }

  didSave(document) {
    this.sendNotification('textDocument/didSave', {
      textDocument: { uri: document.uri.toString() },
      text: document.getText()
    });
  }

  didClose(document) {
    this.sendNotification('textDocument/didClose', {
      textDocument: { uri: document.uri.toString() }
    });
  }

  static positionParams(document, position) {
    return {
      textDocument: { uri: document.uri.toString() },
      position: { line: position.line, character: position.character }
    };
  }

  hover(document, position) {
    return this.sendRequest('textDocument/hover', OstrinLanguageClient.positionParams(document, position));
  }

  definition(document, position) {
    return this.sendRequest('textDocument/definition', OstrinLanguageClient.positionParams(document, position));
  }

  completion(document, position) {
    return this.sendRequest('textDocument/completion', OstrinLanguageClient.positionParams(document, position));
  }

  signatureHelp(document, position) {
    return this.sendRequest('textDocument/signatureHelp', OstrinLanguageClient.positionParams(document, position));
  }

  references(document, position, context) {
    return this.sendRequest('textDocument/references', {
      ...OstrinLanguageClient.positionParams(document, position),
      context: { includeDeclaration: context?.includeDeclaration !== false }
    });
  }

  rename(document, position, newName) {
    return this.sendRequest('textDocument/rename', {
      ...OstrinLanguageClient.positionParams(document, position),
      newName
    });
  }

  semanticTokens(document) {
    return this.sendRequest('textDocument/semanticTokens/full', {
      textDocument: { uri: document.uri.toString() }
    });
  }

  stop() {
    if (!this.process) return Promise.resolve();
    if (!this.ready) {
      this.disposeProcess();
      return Promise.resolve();
    }
    return this.sendRequest('shutdown', null)
      .catch(() => undefined)
      .then(() => {
        this.sendNotification('exit', null);
        this.disposeProcess();
      });
  }

  write(message) {
    const body = Buffer.from(JSON.stringify(message), 'utf8');
    this.process.stdin.write(Buffer.from(`Content-Length: ${body.length}\r\n\r\n`, 'ascii'));
    this.process.stdin.write(body);
  }

  consume(data) {
    this.buffer = Buffer.concat([this.buffer, Buffer.from(data)]);
    while (true) {
      const separator = this.buffer.indexOf('\r\n\r\n');
      if (separator < 0) return;
      const header = this.buffer.slice(0, separator).toString('ascii');
      const match = header.match(/Content-Length:\s*(\d+)/i);
      if (!match) {
        this.buffer = this.buffer.slice(separator + 4);
        continue;
      }
      const length = Number(match[1]);
      const start = separator + 4;
      if (this.buffer.length < start + length) return;
      const body = this.buffer.slice(start, start + length).toString('utf8');
      this.buffer = this.buffer.slice(start + length);
      let message;
      try {
        message = JSON.parse(body);
      } catch (_) {
        continue;
      }
      this.handleMessage(message);
    }
  }

  handleMessage(message) {
    if (message.id !== undefined && this.pending.has(message.id)) {
      const pending = this.pending.get(message.id);
      this.pending.delete(message.id);
      if (message.error) pending.reject(new Error(message.error.message || 'LSP request failed.'));
      else pending.resolve(message.result);
      return;
    }
    if (message.method === 'textDocument/publishDiagnostics' && this.onDiagnostics) {
      this.onDiagnostics(message.params || {});
    }
  }

  disposeProcess() {
    if (!this.process) return;
    for (const pending of this.pending.values()) {
      pending.reject(new Error('Ostrin language server stopped.'));
    }
    this.pending.clear();
    this.process = undefined;
    this.ready = false;
  }
}

module.exports = { OstrinLanguageClient };
