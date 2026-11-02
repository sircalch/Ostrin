import assert from "node:assert/strict";
import test from "node:test";
import { parseMemoryDetail } from "./benchmark-abstraction-overhead.mjs";

test("parses the opt-in native memory-detail line", () => {
  const parsed = parseMemoryDetail("ostrin memory: live_allocations=0 peak_allocations=2 total_allocations=2\nostrin memory-detail: total_bytes=4194320 peak_rss_kb=7988\n");
  assert.deepEqual([parsed.totalBytes, parsed.peakRssKb], [4194320, 7988]);
});

test("rejects missing, duplicated and malformed memory-detail lines", () => {
  assert.throws(() => parseMemoryDetail("ostrin memory: live_allocations=0 peak_allocations=0 total_allocations=0\n"), /exactly one/);
  const line = "ostrin memory-detail: total_bytes=1 peak_rss_kb=2\n";
  assert.throws(() => parseMemoryDetail(line + line), /exactly one/);
  assert.throws(() => parseMemoryDetail("ostrin memory-detail: total_bytes=-1 peak_rss_kb=2\n"), /malformed/);
});
