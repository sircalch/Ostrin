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

const vscode = {
  CompletionItem,
  MarkdownString,
  Position,
  Range,
  Hover,
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

console.log('Ostrin language feature tests passed');
