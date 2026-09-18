const assert = require('assert');
const features = require('./language-features');

class CompletionItem {
  constructor(label, kind) {
    this.label = label;
    this.kind = kind;
  }
}

class MarkdownString {
  constructor(value) {
    this.value = value;
  }
}

class Position {
  constructor(line, character) {
    this.line = line;
    this.character = character;
  }
}

class Range {
  constructor(start, end) {
    this.start = start;
    this.end = end;
  }
}

class Hover {
  constructor(contents, range) {
    this.contents = contents;
    this.range = range;
  }
}

class Location {
  constructor(uri, range) {
    this.uri = uri;
    this.range = range;
  }
}

class WorkspaceEdit {
  constructor() {
    this.entries = [];
  }

  replace(uri, range, newText) {
    this.entries.push({ uri, range, newText });
  }
}

class TextEdit {
  constructor(range, newText) {
    this.range = range;
    this.newText = newText;
  }

  static replace(range, newText) {
    return new TextEdit(range, newText);
  }
}

const vscode = {
  CompletionItem,
  MarkdownString,
  Position,
  Range,
  Hover,
  Location,
  WorkspaceEdit,
  TextEdit,
  CompletionItemKind: {
    Method: 'method',
    Field: 'field',
    EnumMember: 'enumMember',
    Keyword: 'keyword',
    Value: 'value',
    Function: 'function',
    Struct: 'struct',
    Interface: 'interface',
    Class: 'class'
  },
  SymbolKind: {}
};

const index = {
  symbols: [],
  bindings: [{
    name: 'numbers',
    type: 'List<Int>',
    function: 'main',
    scopeDepth: 1,
    file: 'C:/project/main.ostrin',
    line: 2
  }],
  members: [{
    owner: 'List',
    kind: 'method',
    name: 'push',
    detail: 'push(value: T) -> Void',
    resultType: 'Void',
    ownerGenerics: ['T']
  }]
};

const completionLines = ['fn main() {', '  numbers.', '}'];
const completionDocument = {
  uri: { fsPath: 'C:/project/main.ostrin' },
  lineAt: (line) => ({ text: completionLines[line] }),
  lineCount: completionLines.length
};
const completions = features.provideCompletionItems(
  vscode,
  index,
  completionDocument,
  new Position(1, completionLines[1].length)
);
const pushCompletion = completions.find((item) => item.label === 'push');
assert(pushCompletion, 'push should be offered for List<Int>');
assert.strictEqual(pushCompletion.detail, 'push(value: Int) -> Void');

const hoverLines = ['fn main() {', '  numbers.push(1)', '}'];
const hoverDocument = {
  uri: { fsPath: 'C:/project/main.ostrin' },
  lineAt: (line) => ({ text: hoverLines[line] }),
  lineCount: hoverLines.length
};
const hover = features.provideHover(vscode, hoverDocument, new Position(1, 11), index);
assert(hover, 'push should have hover information');
assert(hover.contents.value.includes('push(value: Int) -> Void'));

const expressionHoverDocument = {
  uri: { fsPath: 'C:/project/types.ostrin' },
  lineAt: (line) => ({ text: ['fn main() {', '  1 + 2', '}'][line] }),
  lineCount: 3
};
const expressionHover = features.provideHover(
  vscode,
  expressionHoverDocument,
  new Position(1, 3),
  {
    symbols: [],
    bindings: [],
    members: [],
    expressions: [{ type: 'Int', file: 'C:/project/types.ostrin', line: 2, column: 3 }]
  }
);
assert(expressionHover, 'inferred expression should have hover information');
assert(expressionHover.contents.value.includes('Int'));

const formatted = features.formatOstrinText([
  'fn main() {',
  'if true {',
  'print("ok")',
  '}',
  '}'
].join('\n'));
assert.strictEqual(formatted, [
  'fn main() {',
  '    if true {',
  '        print("ok")',
  '    }',
  '}'
].join('\n'));

const formatDocument = {
  eol: 1,
  lineCount: 5,
  getText: () => ['fn main() {', 'if true {', 'print("ok")', '}', '}'].join('\n')
};
const formatEdits = features.provideDocumentFormattingEdits(vscode, formatDocument);
assert.strictEqual(formatEdits.length, 1, 'formatter should return one full-document edit');
assert(formatEdits[0].newText.includes('        print("ok")'));

const referenceLines = [
  'fn main() {',
  '  numbers = [1, 2]',
  '  print(numbers)',
  '  numbers.push(3)',
  '}'
];
const referenceDocument = {
  uri: { fsPath: 'C:/project/references.ostrin' },
  lineAt: (line) => ({ text: referenceLines[line] }),
  lineCount: referenceLines.length
};
const referenceIndex = {
  symbols: [],
  bindings: [{
    name: 'numbers',
    type: 'List<Int>',
    function: 'main',
    scopeDepth: 1,
    file: 'C:/project/references.ostrin',
    line: 2,
    column: 3
  }],
  members: []
};
const references = features.provideReferences(
  vscode,
  referenceDocument,
  new Position(1, 5),
  { includeDeclaration: true },
  referenceIndex
);
assert.strictEqual(references.length, 3, 'all local binding references should be found');
const rename = features.provideRenameEdits(
  vscode,
  referenceDocument,
  new Position(1, 5),
  'values',
  referenceIndex
);
assert(rename, 'rename should produce a workspace edit');
assert.strictEqual(rename.entries.length, 3, 'rename should edit every local binding reference');

console.log('Ostrin language feature tests passed');
