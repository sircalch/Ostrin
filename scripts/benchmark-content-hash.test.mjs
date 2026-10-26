import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { normalizedTextSha256 } from "./benchmark-content-hash.mjs";

function digest(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

test("canonical text hashes ignore LF, CRLF and bare CR checkout differences", () => {
  const lf = Buffer.from("line one\nline two\n", "utf8");
  const crlf = Buffer.from("line one\r\nline two\r\n", "utf8");
  const cr = Buffer.from("line one\rline two\r", "utf8");

  assert.equal(normalizedTextSha256(lf), digest(lf));
  assert.equal(normalizedTextSha256(crlf), normalizedTextSha256(lf));
  assert.equal(normalizedTextSha256(cr), normalizedTextSha256(lf));
  assert.notEqual(digest(crlf), digest(lf));
});

test("canonical text hashes preserve BOMs and reject binary or invalid UTF-8", () => {
  const withBom = Buffer.from("\uFEFFsource\r\n", "utf8");
  const withoutBom = Buffer.from("source\n", "utf8");

  assert.notEqual(normalizedTextSha256(withBom), normalizedTextSha256(withoutBom));
  assert.equal(normalizedTextSha256(Buffer.from([0, 1, 2])), null);
  assert.equal(normalizedTextSha256(Buffer.from([0xff, 0xfe])), null);
});
