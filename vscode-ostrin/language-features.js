const keywords = [
  'fn', 'mut', 'return', 'if', 'else', 'match', 'for', 'in', 'while', 'loop',
  'break', 'continue', 'trait', 'impl', 'record', 'enum', 'pub', 'import',
  'as', 'self', 'Self', 'dyn', 'and', 'or', 'not', 'within', 'to', 'until',
  'step', 'approximately', 'tolerance', 'try', 'catch', 'spawn',
  'spawn_scope', 'channel', 'true', 'false'
];

const symbols = new Map([
  ['print', ['Builtin function', 'print(value) writes a value to stdout.']],
  ['sum', ['Builtin function', 'sum(values) adds the values in a List.']],
  ['read_file', ['Standard library', 'read_file(path: String) -> Result<String, String>']],
  ['write_file', ['Standard library', 'write_file(path: String, contents: String) -> Result<Void, String>']],
  ['parse_int', ['Standard library', 'parse_int(text: String) -> Result<Int, String>']],
  ['panic', ['Builtin function', 'panic(message: String) stops execution for programmer errors.']],
  ['Option', ['Core enum', 'Option<T> is Some(value) or None.']],
  ['Result', ['Core enum', 'Result<T, E> is Ok(value) or Err(error).']],
  ['List', ['Core collection', 'List<T> is an ordered mutable collection.']],
  ['Map', ['Core collection', 'Map<K, V> stores values by key.']],
  ['Set', ['Core collection', 'Set<T> stores unique values.']],
  ['Quantity', ['Core type', 'Quantity<D> carries a physical dimension checked by the type system.']],
  ['Int', ['Core type', 'Signed 64-bit integer.']],
  ['Float', ['Core type', '64-bit floating-point number.']],
  ['Bool', ['Core type', 'Boolean value: true or false.']],
  ['String', ['Core type', 'Unicode string.']],
  ['Char', ['Core type', 'Unicode scalar value.']],
  ['Void', ['Core type', 'The unit return type.']],
  ['m', ['SI unit', 'meter — Length']],
  ['kg', ['SI unit', 'kilogram — Mass']],
  ['s', ['SI unit', 'second — Time']],
  ['K', ['SI unit', 'kelvin — Temperature']],
  ['A', ['SI unit', 'ampere — ElectricCurrent']],
  ['mol', ['SI unit', 'mole — AmountOfSubstance']],
  ['cd', ['SI unit', 'candela — LuminousIntensity']],
  ['Hz', ['Derived unit', 'hertz — 1 / s']],
  ['N', ['Derived unit', 'newton — kg * m / s^2']],
  ['Pa', ['Derived unit', 'pascal — N / m^2']],
  ['J', ['Derived unit', 'joule — N * m']],
  ['W', ['Derived unit', 'watt — J / s']]
]);

function markdownFor(name) {
  const entry = symbols.get(name);
  if (!entry) return undefined;
  return `**${entry[0]}**\n\n\`${entry[1]}\``;
}

function wordAt(document, position) {
  const line = document.lineAt(position.line).text;
  const left = line.slice(0, position.character);
  const right = line.slice(position.character);
  const leftMatch = left.match(/[A-Za-z_][A-Za-z0-9_]*$/);
  const rightMatch = right.match(/^[A-Za-z0-9_]*/);
  const start = position.character - (leftMatch ? leftMatch[0].length : 0);
  const end = position.character + (rightMatch ? rightMatch[0].length : 0);
  return { word: line.slice(start, end), start, end };
}

function shortSymbolName(name) {
  return name.split(/::|\./).pop();
}

function sameFile(left, right) {
  return left && right && left.replace(/\\/g, '/').toLowerCase() === right.replace(/\\/g, '/').toLowerCase();
}

function braceBalance(text) {
  return (text.match(/{/g) || []).length - (text.match(/}/g) || []).length;
}

function currentBraceDepth(document, position) {
  let depth = 0;
  for (let line = 0; line <= position.line; line += 1) {
    depth += braceBalance(document.lineAt(line).text);
  }
  return Math.max(0, depth);
}

function currentFunctionName(document, position, semanticSymbols) {
  for (let line = position.line; line >= 0; line -= 1) {
    const text = document.lineAt(line).text;
    const match = text.match(/^\s*(?:pub\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\b/);
    if (!match) continue;
    let balance = 0;
    for (let candidate = line; candidate <= position.line; candidate += 1) {
      balance += braceBalance(document.lineAt(candidate).text);
    }
    if (balance > 0) return match[1];
  }
  return semanticSymbols
    .filter((entry) => entry.kind === 'function' && (!entry.file || sameFile(entry.file, document.uri.fsPath)))
    .filter((entry) => entry.line && entry.line <= position.line + 1)
    .sort((left, right) => (right.line || 0) - (left.line || 0))[0]?.name;
}

function normalizeSemanticIndex(index) {
  if (Array.isArray(index)) return { symbols: index, members: [], bindings: [] };
  const source = index || {};
  return {
    symbols: Array.isArray(source.symbols) ? source.symbols : [],
    members: Array.isArray(source.members) ? source.members : [],
    bindings: Array.isArray(source.bindings) ? source.bindings : []
  };
}

function baseType(typeName) {
  const match = String(typeName || '').match(/^([A-Za-z_][A-Za-z0-9_]*)/);
  return match ? match[1] : undefined;
}

function memberAccessAt(document, position) {
  const token = wordAt(document, position);
  const line = document.lineAt(position.line).text;
  const prefix = line.slice(0, token.start);
  const dot = prefix.lastIndexOf('.');
  if (dot < 0) return undefined;
  const receiverExpression = prefix.slice(0, dot).trim();
  if (!receiverExpression || !/[A-Za-z0-9_)\]]$/.test(receiverExpression)) return undefined;
  return { receiverExpression, token };
}

function splitMemberChain(expression) {
  const parts = [];
  let current = '';
  let parentheses = 0;
  let brackets = 0;
  let braces = 0;
  for (const character of expression) {
    if (character === '.' && parentheses === 0 && brackets === 0 && braces === 0) {
      if (current.trim()) parts.push(current.trim());
      current = '';
      continue;
    }
    current += character;
    if (character === '(') parentheses += 1;
    else if (character === ')') parentheses = Math.max(0, parentheses - 1);
    else if (character === '[') brackets += 1;
    else if (character === ']') brackets = Math.max(0, brackets - 1);
    else if (character === '{') braces += 1;
    else if (character === '}') braces = Math.max(0, braces - 1);
  }
  if (current.trim()) parts.push(current.trim());
  return parts
    .map((part) => part.match(/^([A-Za-z_][A-Za-z0-9_]*)\s*(?:\(([\s\S]*)\))?$/))
    .filter(Boolean)
    .map((match) => ({ name: match[1], call: match[2] !== undefined }));
}

function parseGenericArgs(typeName) {
  const open = String(typeName || '').indexOf('<');
  if (open < 0 || !String(typeName).endsWith('>')) return [];
  const inner = String(typeName).slice(open + 1, -1);
  const args = [];
  let current = '';
  let depth = 0;
  for (const character of inner) {
    if (character === '<') depth += 1;
    else if (character === '>') depth = Math.max(0, depth - 1);
    if (character === ',' && depth === 0) {
      args.push(current.trim());
      current = '';
    } else {
      current += character;
    }
  }
  if (current.trim()) args.push(current.trim());
  return args;
}

function substituteType(typeName, replacements) {
  let result = String(typeName || '');
  for (const [name, value] of Object.entries(replacements)) {
    result = result.replace(new RegExp(`\\b${name}\\b`, 'g'), value);
  }
  return result;
}

function memberResultType(member, receiverType) {
  if (!member || !member.resultType) return undefined;
  const args = parseGenericArgs(receiverType);
  const genericNames = Array.isArray(member.ownerGenerics) ? member.ownerGenerics : [];
  const replacements = { Self: baseType(receiverType) || '' };
  genericNames.forEach((name, index) => {
    if (args[index]) replacements[name] = args[index];
  });
  return substituteType(member.resultType, replacements);
}

function symbolReturnType(symbol) {
  if (!symbol) return undefined;
  if (symbol.returnType) return symbol.returnType;
  const match = String(symbol.detail || '').match(/->\s*(.+)$/);
  return match ? match[1].trim() : undefined;
}

function visibleBinding(name, document, position, bindings, semanticSymbols) {
  const activeFunction = currentFunctionName(document, position, semanticSymbols);
  let candidates = bindings
    .filter((entry) => entry.name === name)
    .filter((entry) => !entry.file || sameFile(entry.file, document.uri.fsPath))
    .filter((entry) => !entry.line || entry.line <= position.line + 1)
    .filter((entry) => !entry.scopeDepth || entry.scopeDepth <= currentBraceDepth(document, position))
    .sort((left, right) =>
      (right.scopeDepth || 0) - (left.scopeDepth || 0) || (right.line || 0) - (left.line || 0)
    );
  if (activeFunction) {
    candidates = candidates.filter((entry) => entry.function === activeFunction);
  }
  return candidates[0];
}

function resolveReceiverType(document, position, index) {
  const access = memberAccessAt(document, position);
  if (!access) return undefined;
  const chain = splitMemberChain(access.receiverExpression);
  if (!chain.length) return undefined;
  const first = chain.shift();
  const binding = visibleBinding(first.name, document, position, index.bindings, index.symbols);
  const symbol = index.symbols.find((entry) =>
    (entry.kind === 'function' || entry.kind === 'method') && shortSymbolName(entry.name) === first.name
  );
  let currentType = binding?.type || symbolReturnType(symbol) || first.name;
  for (const segment of chain) {
    const owner = baseType(currentType);
    const member = index.members.find((entry) => entry.owner === owner && entry.name === segment.name);
    if (!member) return undefined;
    currentType = memberResultType(member, currentType);
    if (!currentType) return undefined;
  }
  return currentType;
}

function receiverOwner(document, position, index) {
  const access = memberAccessAt(document, position);
  if (!access) return undefined;
  return baseType(resolveReceiverType(document, position, index))
    || baseType(access.receiverExpression);
}

function semanticCompletionItems(vscode, semanticSymbols) {
  const items = [];
  const known = new Set(symbols.keys());
  const kinds = {
    function: vscode.CompletionItemKind.Function,
    record: vscode.CompletionItemKind.Struct,
    enum: vscode.CompletionItemKind.Enum,
    trait: vscode.CompletionItemKind.Interface,
    method: vscode.CompletionItemKind.Method,
    field: vscode.CompletionItemKind.Field,
    enumMember: vscode.CompletionItemKind.EnumMember,
    implementation: vscode.CompletionItemKind.Class
  };
  for (const entry of semanticSymbols) {
    const name = shortSymbolName(entry.name);
    if (!name || known.has(name)) continue;
    known.add(name);
    const item = new vscode.CompletionItem(name, kinds[entry.kind] || vscode.CompletionItemKind.Value);
    item.detail = entry.detail || `Ostrin ${entry.kind || 'symbol'}`;
    if (entry.detail) item.documentation = new vscode.MarkdownString(`\`${entry.detail}\``);
    items.push(item);
  }
  return items;
}

function memberCompletionItems(vscode, members, owner) {
  const items = [];
  const known = new Set();
  const kinds = {
    method: vscode.CompletionItemKind.Method,
    field: vscode.CompletionItemKind.Field,
    enumMember: vscode.CompletionItemKind.EnumMember
  };
  for (const entry of members) {
    if (entry.owner !== owner || !entry.name || known.has(entry.name)) continue;
    known.add(entry.name);
    const item = new vscode.CompletionItem(entry.name, kinds[entry.kind] || vscode.CompletionItemKind.Value);
    item.detail = entry.detail || `Ostrin ${entry.kind || 'member'}`;
    if (entry.detail) item.documentation = new vscode.MarkdownString(`\`${entry.detail}\``);
    items.push(item);
  }
  return items;
}

function provideCompletionItems(vscode, semanticIndex = [], document, position) {
  const index = normalizeSemanticIndex(semanticIndex);
  if (document && position) {
    const owner = receiverOwner(document, position, index);
    if (owner) return memberCompletionItems(vscode, index.members, owner);
  }
  const items = [];
  for (const keyword of keywords) {
    const item = new vscode.CompletionItem(keyword, vscode.CompletionItemKind.Keyword);
    item.detail = 'Ostrin keyword';
    items.push(item);
  }
  for (const [name, [kind, signature]] of symbols) {
    const item = new vscode.CompletionItem(name, vscode.CompletionItemKind.Value);
    item.detail = kind;
    item.documentation = new vscode.MarkdownString(`\`${signature}\``);
    items.push(item);
  }
  return items.concat(semanticCompletionItems(vscode, index.symbols));
}

function provideHover(vscode, document, position, semanticIndex = []) {
  const index = normalizeSemanticIndex(semanticIndex);
  const token = wordAt(document, position);
  const access = memberAccessAt(document, position);
  const owner = access && receiverOwner(document, position, index);
  const member = owner && index.members.find((entry) => entry.owner === owner && entry.name === token.word);
  const binding = visibleBinding(token.word, document, position, index.bindings, index.symbols);
  const semantic = index.symbols.find((entry) => shortSymbolName(entry.name) === token.word);
  const markdown = member
    ? `**Ostrin ${member.kind || 'member'}**\n\n\`${member.owner}.${member.name}: ${member.detail || ''}\``
    : binding
      ? `**Ostrin local binding**\n\n\`${binding.name}: ${binding.type}\``
      : semantic
    ? `**Ostrin ${semantic.kind || 'symbol'}**\n\n\`${semantic.detail || semantic.name}\``
    : markdownFor(token.word);
  if (!markdown) return undefined;
  const range = new vscode.Range(
    new vscode.Position(position.line, token.start),
    new vscode.Position(position.line, token.end)
  );
  return new vscode.Hover(new vscode.MarkdownString(markdown), range);
}

function provideDocumentSymbols(vscode, document, semanticIndex = []) {
  const index = normalizeSemanticIndex(semanticIndex);
  const topLevelKinds = new Set(['function', 'record', 'enum', 'trait', 'implementation']);
  const local = index.symbols.filter((entry) =>
    topLevelKinds.has(entry.kind) && (!entry.file || sameFile(entry.file, document.uri.fsPath))
  );
  if (local.length) {
    const kinds = {
      function: vscode.SymbolKind.Function,
      record: vscode.SymbolKind.Struct,
      enum: vscode.SymbolKind.Enum,
      trait: vscode.SymbolKind.Interface,
      implementation: vscode.SymbolKind.Class
    };
    return local.map((entry) => {
      const line = Math.max(0, (entry.line || 1) - 1);
      const text = line < document.lineCount ? document.lineAt(line).text : '';
      const name = shortSymbolName(entry.name);
      const start = Math.max(0, text.indexOf(name));
      const range = new vscode.Range(
        new vscode.Position(line, 0),
        new vscode.Position(line, text.length)
      );
      const selectionRange = new vscode.Range(
        new vscode.Position(line, start),
        new vscode.Position(line, start + name.length)
      );
      return new vscode.DocumentSymbol(
        name,
        entry.detail || entry.kind,
        kinds[entry.kind] || vscode.SymbolKind.Namespace,
        range,
        selectionRange
      );
    });
  }

  const result = [];
  const declaration = /^\s*(?:pub\s+)?(fn|record|enum|trait|impl)\s+([A-Za-z_][A-Za-z0-9_]*)/;
  for (let line = 0; line < document.lineCount; line += 1) {
    const text = document.lineAt(line).text;
    const match = text.match(declaration);
    if (!match) continue;
    const kind = {
      fn: vscode.SymbolKind.Function,
      record: vscode.SymbolKind.Struct,
      enum: vscode.SymbolKind.Enum,
      trait: vscode.SymbolKind.Interface,
      impl: vscode.SymbolKind.Class
    }[match[1]];
    const start = text.indexOf(match[2]);
    const range = new vscode.Range(
      new vscode.Position(line, 0),
      new vscode.Position(line, text.length)
    );
    const selectionRange = new vscode.Range(
      new vscode.Position(line, start),
      new vscode.Position(line, start + match[2].length)
    );
    result.push(new vscode.DocumentSymbol(match[2], match[1], kind, range, selectionRange));
  }
  return result;
}

module.exports = {
  provideCompletionItems,
  provideHover,
  provideDocumentSymbols
};
