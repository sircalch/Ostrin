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

function provideCompletionItems(vscode) {
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
  return items;
}

function provideHover(vscode, document, position) {
  const token = wordAt(document, position);
  const markdown = markdownFor(token.word);
  if (!markdown) return undefined;
  const range = new vscode.Range(
    new vscode.Position(position.line, token.start),
    new vscode.Position(position.line, token.end)
  );
  return new vscode.Hover(new vscode.MarkdownString(markdown), range);
}

function provideDocumentSymbols(vscode, document) {
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
