// Builds a history entry from one finished Workbench run. The digests are SHA-256 values computed with Web Crypto
// over the same bytes the run manifest hashes: the source texts of the run's immutable input snapshot, the raw
// captured stdout and stderr, and the figure. Nothing here changes the open project.

import { parseResults } from "./workbench-results.js";
import { runRecord } from "./workbench-history.js";

const encoder = new TextEncoder();

export async function sha256Hex(data) {
  const bytes = typeof data === "string" ? encoder.encode(data) : data;
  const digest = await globalThis.crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

// options: { id, input, status, command, startedAt, durationMs, exitCode, compilerIdentity, lines, stdoutBytes,
//            stderrBytes, figureSvg, projectId }
// Returns { entry, figureSvg } ready for recordRun. Incomplete runs (no output) pass null bytes and lines.
export async function captureRun(options) {
  const { id, input, status, command, startedAt, durationMs, exitCode = null, compilerIdentity = null, lines = null, stdoutBytes = null, stderrBytes = null, figureSvg = null, projectId } = options;
  const sources = {};
  for (const [name, text] of Object.entries(input.project.files).sort()) {
    sources[name] = await sha256Hex(text);
  }
  const completed = status === "completed";
  const parsed = completed && lines ? parseResults(lines) : { ok: false, reason: null };
  const entry = runRecord({
    id,
    projectId,
    projectName: input.project.name,
    entry: input.entry,
    command,
    status,
    exitCode,
    compilerIdentity,
    startedAt,
    durationMs,
    parameters: parsed.ok ? { c: parsed.document.rows.map((r) => r.c) } : null,
    results: parsed.ok ? parsed.document : null,
    resultsRejected: completed && !parsed.ok && parsed.reason !== "no structured results in this run" ? parsed.reason : null,
    digests: {
      sources,
      stdout: stdoutBytes ? await sha256Hex(stdoutBytes) : null,
      stderr: stderrBytes ? await sha256Hex(stderrBytes) : null,
      figure: figureSvg !== null ? await sha256Hex(figureSvg) : null,
      manifest: null,
    },
  });
  return { entry, figureSvg };
}
