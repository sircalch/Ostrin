import test from "node:test";
import assert from "node:assert/strict";
import { orderedLines } from "../website/workbench-protocol.js";
import { createPresentationGuard } from "../website/workbench-presentation.js";

const enc = (text) => new TextEncoder().encode(text);
const c = (stream, text) => ({ stream, bytes: enc(text) });

test("ordered output: complete lines from both streams follow the write order", () => {
  const lines = orderedLines([c("out", "one\n"), c("err", "two\n"), c("out", "three\n"), c("err", "four\n")]);
  assert.deepEqual(lines, [["out", "one"], ["err", "two"], ["out", "three"], ["err", "four"]]);
});

test("ordered output: a chunk can hold several lines and they keep their order", () => {
  const lines = orderedLines([c("out", "a\nb\n"), c("err", "x\n"), c("out", "c\n")]);
  assert.deepEqual(lines, [["out", "a"], ["out", "b"], ["err", "x"], ["out", "c"]]);
});

test("ordered output: a multi-byte UTF-8 character split across stdout writes decodes intact", () => {
  const bytes = enc("é\n");
  const lines = orderedLines([{ stream: "out", bytes: bytes.slice(0, 1) }, { stream: "out", bytes: bytes.slice(1) }]);
  assert.deepEqual(lines, [["out", "é"]]);
});

test("ordered output: a multi-byte UTF-8 character split across stderr writes decodes intact", () => {
  const bytes = enc("ñ\n");
  const lines = orderedLines([c("out", "x\n"), { stream: "err", bytes: bytes.slice(0, 1) }, { stream: "err", bytes: bytes.slice(1) }]);
  assert.deepEqual(lines, [["out", "x"], ["err", "ñ"]]);
});

test("ordered output: a split character on one stream is not disturbed by writes on the other stream", () => {
  const bytes = enc("é\n");
  const lines = orderedLines([{ stream: "out", bytes: bytes.slice(0, 1) }, c("err", "a\n"), { stream: "out", bytes: bytes.slice(1) }]);
  assert.deepEqual(lines, [["err", "a"], ["out", "é"]]);
});

test("ordered output: unterminated tails of both streams keep their start order", () => {
  // stdout starts first, stderr starts later; neither ends with a newline.
  assert.deepEqual(orderedLines([c("out", "first"), c("err", "second")]), [["out", "first"], ["err", "second"]]);
  // A tail that started before a completed line on the other stream is placed before it.
  assert.deepEqual(orderedLines([c("out", "a"), c("err", "b\n")]), [["out", "a"], ["err", "b"]]);
  // The reverse: the stderr tail started first.
  assert.deepEqual(orderedLines([c("err", "e"), c("out", "o\n")]), [["err", "e"], ["out", "o"]]);
});

test("ordered output: alternating writes that end without newlines keep each stream's line whole", () => {
  const lines = orderedLines([c("out", "o1 "), c("err", "e1 "), c("out", "o2"), c("err", "e2")]);
  assert.deepEqual(lines, [["out", "o1 o2"], ["err", "e1 e2"]]);
});

test("ordered output: empty writes and empty sequences produce no lines", () => {
  assert.deepEqual(orderedLines([]), []);
  assert.deepEqual(orderedLines([c("out", ""), c("err", "")]), []);
  assert.deepEqual(orderedLines([c("out", "\n")]), [["out", ""]], "an empty line still counts as a line");
});

test("presentation guard: only the newest ticket of the same project is current", () => {
  const guard = createPresentationGuard();
  const a = guard.begin();
  assert.equal(guard.isCurrent(a), true);
  const b = guard.begin();
  assert.equal(guard.isCurrent(a), false, "an older run cannot present over a newer one");
  assert.equal(guard.isCurrent(b), true);
});

test("presentation guard: changing the project makes every earlier ticket stale", () => {
  const guard = createPresentationGuard();
  const a = guard.begin();
  guard.changeProject();
  assert.equal(guard.isCurrent(a), false, "a run that finishes after a project change cannot publish");
  const b = guard.begin();
  assert.equal(guard.isCurrent(b), true, "a run started for the new project is current");
});

test("presentation guard: ending a ticket only ends that presentation", () => {
  const guard = createPresentationGuard();
  const a = guard.begin();
  guard.end(a);
  assert.equal(guard.isCurrent(a), false);
  const b = guard.begin();
  guard.end(a); // a stale ticket cannot end the newer one
  assert.equal(guard.isCurrent(b), true);
});

// The three sequences the Playground must survive, expressed on the guard that decides every publication.
test("sequence: A finishes while its identity is pending, B finishes, then A is released and publishes nothing", () => {
  const guard = createPresentationGuard();
  const a = guard.begin(); // A started
  // A's run completes and it waits for the compiler identity. Meanwhile the user starts B.
  const b = guard.begin();
  assert.equal(guard.isCurrent(a), false, "A is stale as soon as B begins");
  assert.equal(guard.isCurrent(b), true);
  // B finishes and publishes; A's identity arrives afterwards.
  guard.end(b);
  assert.equal(guard.isCurrent(a), false, "A stays stale after B ends");
  assert.equal(guard.generation, 0);
});

test("sequence: A fails late, after B already has valid results, and cannot change them", () => {
  const guard = createPresentationGuard();
  const a = guard.begin();
  const b = guard.begin();
  // B's result is current and published; A's late exception is checked against A's ticket only.
  assert.equal(guard.isCurrent(b), true);
  assert.equal(guard.isCurrent(a), false, "A's exception path is refused and cannot overwrite B");
});

test("sequence: A produces an SVG, B produces another, and only B may publish a figure", () => {
  const guard = createPresentationGuard();
  const a = guard.begin();
  const b = guard.begin();
  assert.equal(guard.isCurrent(a), false, "A may not publish its figure");
  assert.equal(guard.isCurrent(b), true, "B is the only figure that can be published");
});
