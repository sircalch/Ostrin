// Portable scientific evidence packages: schema "ostrin.evidence-package/v1", file extension .ostrin.evidence.json.
//
// A package is one JSON document that carries the runs of the Run History together with the original bytes they
// refer to (sources, stdout, stderr and figures), each encoded in Base64 and identified by its SHA-256. It is
// verifiable offline: the importer recomputes every digest over the decoded bytes, checks every cross-reference, and
// recomputes the package integrity digest. Nothing in a package is executed, interpreted as markup or evaluated.
//
// What the integrity digest proves: that the document is unchanged since it was sealed, relative to a digest that
// the reader already trusts. It is NOT authentication: whoever can change the content can recompute every digest.
// It is not a reproducibility claim (R3 is not claimed): equal digests mean equal bytes, nothing more.
//
// Every SHA-256 here is computed over the exact bytes that were stored: sources and figures as UTF-8 of their text,
// stdout and stderr as the raw bytes captured from the run. Nothing is re-serialised before hashing.
//
// This module is pure: the hash function is injected (digestOf(bytes) -> lowercase hex) so it runs in Node and in
// the page alike.

import { runRecord } from "./workbench-history.js";

export const PACKAGE_SCHEMA = "ostrin.evidence-package/v1";
export const PACKAGE_EXTENSION = ".ostrin.evidence.json";
export const PACKAGE_MIME = "application/vnd.ostrin.evidence+json";
export const CANONICALIZATION = "ostrin-canonical-json/v1";
export const INTEGRITY_ALGORITHM = "sha256";

export const LIMITS = Object.freeze({
  maxPackageChars: 32 * 1024 * 1024,
  maxRuns: 100,
  maxContents: 500,
  maxContentBytes: 8 * 1024 * 1024,
  maxTotalContentBytes: 24 * 1024 * 1024,
  maxDepth: 12,
  maxNodes: 200_000,
});

export const CONTENT_KINDS = Object.freeze(["source", "stdout", "stderr", "figure"]);
export const CONTENT_STATUSES = Object.freeze(["included", "digest-only", "withheld"]);
export const COVERAGE = Object.freeze({
  complete: "complete",
  partialContent: "partial-content",
  incompleteRun: "incomplete-run",
  incompleteResults: "incomplete-results",
});

const HEX64 = /^[0-9a-f]{64}$/;
const BASE64 = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;
const ISO_UTC = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{1,3})?Z$/;
const strictUtf8 = new TextDecoder("utf-8", { fatal: true });
const encoder = new TextEncoder();

export class PackageError extends Error {
  constructor(code, message) {
    super(message);
    this.name = "PackageError";
    this.code = code;
  }
}

const isPlainObject = (value) => value !== null && typeof value === "object" && !Array.isArray(value);

// Canonical JSON: object keys in code-unit order, no whitespace, strings escaped by JSON.stringify. Two documents
// with the same content produce the same bytes whatever their key order or formatting. Only finite numbers allowed.
export function canonicalJson(value) {
  if (value === null) return "null";
  switch (typeof value) {
    case "boolean":
      return value ? "true" : "false";
    case "number":
      if (!Number.isFinite(value)) throw new PackageError("invalid-value", "non-finite number cannot be canonicalised");
      return JSON.stringify(value);
    case "string":
      return JSON.stringify(value);
    case "object":
      if (Array.isArray(value)) return "[" + value.map(canonicalJson).join(",") + "]";
      return "{" + Object.keys(value).sort().map((key) => JSON.stringify(key) + ":" + canonicalJson(value[key])).join(",") + "}";
    default:
      throw new PackageError("invalid-value", `${typeof value} cannot be canonicalised`);
  }
}

export function encodeBase64(bytes) {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  return btoa(binary);
}

export function decodeBase64(text) {
  if (typeof text !== "string" || !BASE64.test(text)) throw new PackageError("bad-encoding", "content is not well-formed Base64");
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

// Coverage of one run: what the evidence can support. Never upgraded by the content that is present.
export function coverageOf(entry, contentStatus) {
  if (entry.status !== "completed") return COVERAGE.incompleteRun;
  if (entry.evidence !== "validated") return COVERAGE.incompleteResults;
  const statuses = [...Object.values(contentStatus.sources), contentStatus.stdout, contentStatus.stderr, contentStatus.figure].filter(
    (s) => s !== null,
  );
  return statuses.every((s) => s === "included") ? COVERAGE.complete : COVERAGE.partialContent;
}

// Every digest a run refers to, with its kind. Same order as the Run History: sources, stdout, stderr, figure.
export function referencesOf(entry) {
  const refs = [];
  for (const [name, digest] of Object.entries(entry.digests.sources)) refs.push({ kind: "source", name, digest });
  if (entry.digests.stdout) refs.push({ kind: "stdout", name: "stdout", digest: entry.digests.stdout });
  if (entry.digests.stderr) refs.push({ kind: "stderr", name: "stderr", digest: entry.digests.stderr });
  if (entry.digests.figure) refs.push({ kind: "figure", name: "figure", digest: entry.digests.figure });
  return refs;
}

// The integrity block covers every field of the package except itself, serialised canonically.
export async function sealPackage(pkg, digestOf) {
  const { integrity: _ignored, ...body } = pkg;
  const digest = await digestOf(encoder.encode(canonicalJson(body)));
  return {
    ...body,
    integrity: { algorithm: INTEGRITY_ALGORITHM, canonicalization: CANONICALIZATION, covers: "every field except integrity", digest },
  };
}

// Builds a package from runs and the bytes available for them.
//   runs:     [{ entry }] where entry is a Run History entry (ostrin.history/v1).
//   contents: Map digest -> { kind, bytes } for every byte sequence that may be exported.
//   withheld: Map digest -> reason, for digests whose bytes must not be exported as verified evidence.
// Each content is hashed again here. A mismatch withholds it; it is never exported under its declared digest.
export async function buildPackage({ runs, contents, withheld = new Map(), createdAt, producer, digestOf }) {
  if (runs.length === 0) throw new PackageError("empty", "no runs selected");
  if (runs.length > LIMITS.maxRuns) throw new PackageError("too-many-runs", `a package holds at most ${LIMITS.maxRuns} runs`);
  const included = new Map();
  const refused = new Map(withheld);
  const referenced = new Set(runs.flatMap(({ entry }) => referencesOf(entry).map((ref) => ref.digest)));
  let total = 0;
  for (const [digest, { kind, bytes }] of contents) {
    // Only bytes that a selected run refers to are exported; anything else in the caller's store is left out.
    if (!referenced.has(digest)) continue;
    if ((await digestOf(bytes)) !== digest) {
      refused.set(digest, "stored bytes do not match their SHA-256 digest");
      continue;
    }
    if (bytes.byteLength > LIMITS.maxContentBytes) throw new PackageError("too-large", `content ${digest} is ${bytes.byteLength} bytes; the limit is ${LIMITS.maxContentBytes}`);
    total += bytes.byteLength;
    included.set(digest, { kind, bytes });
  }
  if (total > LIMITS.maxTotalContentBytes) throw new PackageError("too-large", `the contents total ${total} bytes; the limit is ${LIMITS.maxTotalContentBytes}`);
  if (included.size > LIMITS.maxContents) throw new PackageError("too-many-contents", `a package holds at most ${LIMITS.maxContents} contents`);

  const statusOf = (digest) => (included.has(digest) ? "included" : refused.has(digest) ? "withheld" : "digest-only");
  const packagedRuns = runs.map(({ entry }) => {
    const contentStatus = { sources: {}, stdout: null, stderr: null, figure: null };
    for (const ref of referencesOf(entry)) {
      const status = statusOf(ref.digest);
      if (ref.kind === "source") contentStatus.sources[ref.name] = status;
      else contentStatus[ref.kind] = status;
    }
    const reasons = {};
    for (const ref of referencesOf(entry)) if (refused.has(ref.digest)) reasons[ref.digest] = refused.get(ref.digest);
    return { entry: JSON.parse(JSON.stringify(entry)), contentStatus, coverage: coverageOf(entry, contentStatus), reasons };
  });
  const packagedContents = [...included.entries()]
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([digest, { kind, bytes }]) => ({ digest, kind, encoding: "base64", size: bytes.byteLength, data: encodeBase64(bytes) }));
  const reasonsOut = {};
  for (const run of packagedRuns) Object.assign(reasonsOut, run.reasons);
  const pkg = {
    schema: PACKAGE_SCHEMA,
    createdAt,
    producer,
    coverage: summarise(packagedRuns),
    runs: packagedRuns.map(({ entry, contentStatus, coverage }) => ({ entry, contentStatus, coverage })),
    contents: packagedContents,
    withheld: Object.entries(reasonsOut).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)).map(([digest, reason]) => ({ digest, reason })),
  };
  const sealed = await sealPackage(pkg, digestOf);
  return { pkg: sealed, text: JSON.stringify(sealed, null, 2) + "\n" };
}

function summarise(runs) {
  const count = (c) => runs.filter((r) => r.coverage === c).length;
  return {
    runs: runs.length,
    complete: count(COVERAGE.complete),
    partialContent: count(COVERAGE.partialContent),
    incompleteRun: count(COVERAGE.incompleteRun),
    incompleteResults: count(COVERAGE.incompleteResults),
  };
}

// Iterative structure check: depth and node count are bounded before anything is interpreted.
function checkShape(root) {
  const stack = [[root, 0]];
  let nodes = 0;
  while (stack.length > 0) {
    const [value, depth] = stack.pop();
    nodes += 1;
    if (nodes > LIMITS.maxNodes) throw new PackageError("too-complex", `the package holds more than ${LIMITS.maxNodes} values`);
    if (value !== null && typeof value === "object") {
      if (depth > LIMITS.maxDepth) throw new PackageError("too-deep", `nesting is deeper than ${LIMITS.maxDepth}`);
      for (const child of Object.values(value)) stack.push([child, depth + 1]);
    }
  }
}

function exactKeys(object, keys, where) {
  if (!isPlainObject(object)) throw new PackageError("bad-structure", `${where} must be an object`);
  const actual = Object.keys(object).sort();
  const expected = [...keys].sort();
  if (actual.length !== expected.length || actual.some((k, i) => k !== expected[i])) {
    throw new PackageError("bad-structure", `${where} must have exactly the fields ${expected.join(", ")}`);
  }
}

// Verifies a package text completely before anything is imported. Returns the verified runs and contents, with the
// bytes decoded. Throws PackageError with a code and a message on the first problem found.
export async function verifyPackage(text, digestOf) {
  if (typeof text !== "string") throw new PackageError("invalid-input", "the package is not text");
  if (text.length > LIMITS.maxPackageChars) {
    throw new PackageError("too-large", `the package is ${text.length} characters; the limit is ${LIMITS.maxPackageChars}`);
  }
  let pkg;
  try {
    pkg = JSON.parse(text);
  } catch (error) {
    throw new PackageError("invalid-json", "the file is not valid JSON: " + error.message);
  }
  checkShape(pkg);
  if (!isPlainObject(pkg)) throw new PackageError("bad-structure", "the package must be a JSON object");
  if (pkg.schema !== PACKAGE_SCHEMA) {
    throw new PackageError("unknown-schema", `unsupported package schema ${JSON.stringify(pkg.schema)}; this reader understands ${PACKAGE_SCHEMA}`);
  }
  exactKeys(pkg, ["schema", "createdAt", "producer", "coverage", "runs", "contents", "withheld", "integrity"], "the package");

  // Integrity first: any change to the document, including its metadata, is detected here.
  const integrity = pkg.integrity;
  exactKeys(integrity, ["algorithm", "canonicalization", "covers", "digest"], "integrity");
  if (integrity.algorithm !== INTEGRITY_ALGORITHM) throw new PackageError("unknown-algorithm", `unsupported integrity algorithm ${JSON.stringify(integrity.algorithm)}`);
  if (integrity.canonicalization !== CANONICALIZATION) throw new PackageError("unknown-algorithm", `unsupported canonicalization ${JSON.stringify(integrity.canonicalization)}`);
  if (typeof integrity.digest !== "string" || !HEX64.test(integrity.digest)) throw new PackageError("bad-structure", "integrity digest must be lowercase SHA-256 hex");
  const { integrity: _ignored, ...body } = pkg;
  const recomputed = await digestOf(encoder.encode(canonicalJson(body)));
  if (recomputed !== integrity.digest) {
    throw new PackageError("package-digest-mismatch", "the package does not match its integrity digest: its content was changed after it was sealed");
  }

  if (typeof pkg.createdAt !== "string" || !ISO_UTC.test(pkg.createdAt) || Number.isNaN(Date.parse(pkg.createdAt))) {
    throw new PackageError("bad-structure", "createdAt must be an ISO 8601 UTC time");
  }
  exactKeys(pkg.producer, ["name"], "producer");
  if (typeof pkg.producer.name !== "string" || pkg.producer.name.length > 100) throw new PackageError("bad-structure", "producer name must be a string of at most 100 characters");

  if (!Array.isArray(pkg.runs) || pkg.runs.length === 0) throw new PackageError("bad-structure", "runs must be a non-empty array");
  if (pkg.runs.length > LIMITS.maxRuns) throw new PackageError("too-many-runs", `a package holds at most ${LIMITS.maxRuns} runs`);
  if (!Array.isArray(pkg.contents)) throw new PackageError("bad-structure", "contents must be an array");
  if (pkg.contents.length > LIMITS.maxContents) throw new PackageError("too-many-contents", `a package holds at most ${LIMITS.maxContents} contents`);
  if (!Array.isArray(pkg.withheld)) throw new PackageError("bad-structure", "withheld must be an array");

  // Runs: each one must rebuild exactly through the history model, so its status, evidence and metadata agree.
  const ids = new Set();
  const references = new Map(); // digest -> Set of kinds in which runs refer to it
  const runs = [];
  for (const [index, run] of pkg.runs.entries()) {
    exactKeys(run, ["entry", "contentStatus", "coverage"], `run ${index}`);
    let rebuilt;
    try {
      rebuilt = runRecord({ ...run.entry });
    } catch (error) {
      throw new PackageError("inconsistent-run", `run ${index}: ${error.message}`);
    }
    if (canonicalJson(rebuilt) !== canonicalJson(run.entry)) {
      throw new PackageError("inconsistent-run", `run ${index} (${run.entry.id}): its fields do not agree with each other (status, evidence, metadata or digests)`);
    }
    if (ids.has(rebuilt.id)) throw new PackageError("duplicate-run", `run id ${rebuilt.id} appears twice in the package`);
    ids.add(rebuilt.id);
    for (const ref of referencesOf(rebuilt)) {
      if (!references.has(ref.digest)) references.set(ref.digest, new Set());
      references.get(ref.digest).add(ref.kind);
    }
    exactKeys(run.contentStatus, ["sources", "stdout", "stderr", "figure"], `run ${rebuilt.id} contentStatus`);
    exactKeys(run.contentStatus.sources, Object.keys(rebuilt.digests.sources), `run ${rebuilt.id} source statuses`);
    runs.push({ entry: rebuilt, contentStatus: run.contentStatus, coverage: run.coverage });
  }

  // Contents: each is decoded, its length checked, and its SHA-256 recomputed over the decoded bytes.
  const contents = new Map();
  let total = 0;
  for (const [index, item] of pkg.contents.entries()) {
    exactKeys(item, ["digest", "kind", "encoding", "size", "data"], `content ${index}`);
    if (typeof item.digest !== "string" || !HEX64.test(item.digest)) throw new PackageError("bad-structure", `content ${index} digest must be lowercase SHA-256 hex`);
    if (contents.has(item.digest)) throw new PackageError("duplicate-content", `content ${item.digest} appears twice`);
    if (!CONTENT_KINDS.includes(item.kind)) throw new PackageError("bad-structure", `content ${item.digest} has unknown kind ${JSON.stringify(item.kind)}`);
    if (item.encoding !== "base64") throw new PackageError("bad-encoding", `content ${item.digest} must be base64, not ${JSON.stringify(item.encoding)}`);
    if (!Number.isInteger(item.size) || item.size < 0 || item.size > LIMITS.maxContentBytes) {
      throw new PackageError("too-large", `content ${item.digest} declares size ${item.size}; the limit is ${LIMITS.maxContentBytes}`);
    }
    const bytes = decodeBase64(item.data);
    if (bytes.byteLength !== item.size) throw new PackageError("size-mismatch", `content ${item.digest} declares ${item.size} bytes but holds ${bytes.byteLength}`);
    total += bytes.byteLength;
    if (total > LIMITS.maxTotalContentBytes) throw new PackageError("too-large", `the contents total more than ${LIMITS.maxTotalContentBytes} bytes`);
    const actual = await digestOf(bytes);
    if (actual !== item.digest) {
      throw new PackageError("digest-mismatch", `content ${item.digest} does not match its SHA-256: its bytes are ${actual}`);
    }
    if (!references.has(item.digest)) throw new PackageError("unreferenced-content", `content ${item.digest} is not referred to by any run`);
    if (!references.get(item.digest).has(item.kind)) {
      throw new PackageError("kind-mismatch", `content ${item.digest} is declared as ${item.kind}, but the runs use it as ${[...references.get(item.digest)].join(" or ")}`);
    }
    if (item.kind === "source" || item.kind === "figure") {
      try {
        strictUtf8.decode(bytes);
      } catch (_) {
        throw new PackageError("bad-encoding", `${item.kind} ${item.digest} is not valid UTF-8 text`);
      }
    }
    contents.set(item.digest, { digest: item.digest, kind: item.kind, bytes });
  }

  // Every run's declared status must agree with the contents actually present.
  for (const run of runs) {
    for (const ref of referencesOf(run.entry)) {
      const statusOfRun = ref.kind === "source" ? run.contentStatus.sources[ref.name] : run.contentStatus[ref.kind];
      const present = contents.has(ref.digest);
      if (present && statusOfRun !== "included") throw new PackageError("inconsistent-content", `run ${run.entry.id}: ${ref.kind} ${ref.digest} is present but declared ${statusOfRun}`);
      if (!present && statusOfRun === "included") throw new PackageError("missing-content", `run ${run.entry.id}: ${ref.kind} ${ref.digest} is declared included but its bytes are missing`);
      if (!present && !CONTENT_STATUSES.includes(statusOfRun)) throw new PackageError("bad-structure", `run ${run.entry.id}: unknown content status ${JSON.stringify(statusOfRun)}`);
    }
    if (run.contentStatus.stdout === undefined) throw new PackageError("bad-structure", "contentStatus is incomplete");
    const expected = coverageOf(run.entry, run.contentStatus);
    if (run.coverage !== expected) throw new PackageError("coverage-mismatch", `run ${run.entry.id} declares coverage ${run.coverage}, but its evidence gives ${expected}`);
  }
  for (const item of pkg.withheld) {
    exactKeys(item, ["digest", "reason"], "withheld entry");
    if (typeof item.digest !== "string" || !HEX64.test(item.digest) || typeof item.reason !== "string" || item.reason.length > 200) {
      throw new PackageError("bad-structure", "withheld entries need a SHA-256 digest and a short reason");
    }
    if (!references.has(item.digest)) throw new PackageError("unreferenced-content", `withheld ${item.digest} is not referred to by any run`);
    if (contents.has(item.digest)) throw new PackageError("inconsistent-content", `${item.digest} is both included and withheld`);
  }
  const summary = summarise(runs);
  if (canonicalJson(summary) !== canonicalJson(pkg.coverage)) throw new PackageError("coverage-mismatch", "the package summary does not match its runs");
  return { createdAt: pkg.createdAt, producer: pkg.producer, runs, contents: [...contents.values()], coverage: summary };
}

// Safe file name for a package: only letters, digits, dots, dashes and underscores, bounded in length.
export function packageFileName(ids, date = new Date()) {
  const stamp = date.toISOString().replace(/[:.]/g, "-").slice(0, 19);
  const safe = ids.length === 1 ? ids[0].replace(/[^A-Za-z0-9_-]+/g, "-").slice(0, 60) : `${ids.length}-runs`;
  return `ostrin-evidence-${safe}-${stamp}${PACKAGE_EXTENSION}`;
}
