import test from "node:test";
import assert from "node:assert/strict";
import { parseBlocks, parseInlines, safeHref } from "../website/workbench-markdown.js";

test("headings, paragraphs, lists, quotes, rules and fenced code are parsed as blocks", () => {
  const blocks = parseBlocks("# Title\n\nFirst line\ncontinues here.\n\n- one\n- two\n\n1. first\n2. second\n\n> quoted\n\n---\n\n```\nlet x = 1\n```\n");
  assert.deepEqual(blocks.map((b) => b.type), ["heading", "paragraph", "list", "list", "quote", "rule", "code"]);
  assert.equal(blocks[0].level, 1);
  assert.equal(blocks[1].inlines[0].text, "First line continues here.");
  assert.equal(blocks[2].ordered, false);
  assert.equal(blocks[2].items.length, 2);
  assert.equal(blocks[3].ordered, true);
  assert.equal(blocks[6].text, "let x = 1");
});

test("inline code, strong, emphasis and links are recognised", () => {
  const nodes = parseInlines("use `x` and **bold** and *soft* see [docs](https://example.org/a)");
  assert.deepEqual(nodes.map((n) => n.type), ["text", "code", "text", "strong", "text", "em", "text", "link"]);
  assert.equal(nodes[7].href, "https://example.org/a");
});

test("link targets: web, mail, fragment and plain relative paths are allowed; script schemes are refused", () => {
  assert.equal(safeHref("https://example.org/x?y=1"), "https://example.org/x?y=1");
  assert.equal(safeHref("mailto:someone@example.org"), "mailto:someone@example.org");
  assert.equal(safeHref("#section"), "#section");
  assert.equal(safeHref("docs.html"), "docs.html");
  assert.equal(safeHref("./data/x.csv"), "./data/x.csv");
  assert.equal(safeHref("javascript:alert(1)"), null);
  assert.equal(safeHref("JavaScript:alert(1)"), null);
  assert.equal(safeHref("data:text/html,<script>alert(1)</script>"), null);
  assert.equal(safeHref("//evil.example/x"), null);
  assert.equal(safeHref("vbscript:msgbox"), null);
  assert.equal(safeHref("https://a.example/ b"), null);
  assert.equal(safeHref(""), null);
});

test("a link to a refused target keeps its text and loses its href", () => {
  const nodes = parseInlines("[click](javascript:alert(1))");
  assert.equal(nodes[0].type, "link");
  assert.equal(nodes[0].text, "click");
  assert.equal(nodes[0].href, null);
});

test("raw HTML is never interpreted: it is text", () => {
  const blocks = parseBlocks("<script>alert(1)</script> and <img src=x onerror=alert(2)>");
  assert.equal(blocks[0].type, "paragraph");
  const text = blocks[0].inlines.map((n) => n.text).join("");
  assert.match(text, /<script>alert\(1\)<\/script>/);
});

test("images are shown as their alt text and never loaded", () => {
  const nodes = parseInlines("![plot of the decay](https://example.org/p.svg)");
  assert.deepEqual(nodes, [{ type: "text", text: "plot of the decay" }]);
});

test("dollar expressions are kept as text, since typeset mathematics is not supported", () => {
  const nodes = parseInlines("the rate $k = 0.35$ per second");
  assert.equal(nodes.map((n) => n.text).join(""), "the rate $k = 0.35$ per second");
});
