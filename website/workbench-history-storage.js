// Storage rules for the Run History. This module decides what one saved run contains, verifies stored bytes against
// their SHA-256 digests when they are read back, and isolates damaged records. It never touches IndexedDB: the adapter
// in workbench-history-idb.js performs the reads and writes that this module prepares.
//
// Three things are kept apart:
//   - a registered digest: a SHA-256 value written in a run entry;
//   - verified content: stored bytes whose SHA-256, recomputed now, equals the registered digest;
//   - a reproducible run: nothing here claims it (R3 is not claimed).
//
// The record format (RECORD_SCHEMA) is independent of the IndexedDB version (workbench-history-idb.js) and of the
// run-entry format (HISTORY_SCHEMA, ostrin.history/v1 in workbench-history.js).

import { runRecord, snapshot } from "./workbench-history.js";
import { canonicalJson, referencesOf } from "./workbench-evidence.js";

export const RECORD_SCHEMA = "ostrin.history-record/v1";
// Upper bound for the bytes of one run (sources, stdout, stderr and figure, each counted once). Above it nothing is
// written; the run stays visible in the page and is reported as not saved.
export const MAX_RUN_CONTENT_BYTES = 8 * 1024 * 1024;

export const ERROR_CODES = Object.freeze({
  quota: "quota",
  denied: "denied",
  duplicate: "duplicate",
  tooLarge: "too-large",
  invalid: "invalid",
  unavailable: "unavailable",
  transaction: "transaction",
  conflict: "conflict",
});

// Plans an import against what is already stored. `verified` comes from verifyPackage; `existing` holds the stored
// record (or undefined) for each run id and the stored content (or undefined) for each digest. Nothing is written
// here. Identical runs are not duplicated; a run id with different content is a conflict, and any conflict refuses
// the whole import. Content already stored under the same digest is reused, after comparing its bytes.
export function planImport(verified, existing, now) {
  const identical = [];
  const conflicts = [];
  const newRuns = [];
  for (const { entry, contentStatus } of verified.runs) {
    const stored = existing.records.get(entry.id);
    if (stored === undefined) {
      newRuns.push({ id: entry.id, record: importRecord(entry, contentStatus, now) });
    } else if (canonicalJson(stored.entry) === canonicalJson(entry)) {
      identical.push(entry.id);
    } else {
      conflicts.push(entry.id);
    }
  }
  if (conflicts.length > 0) {
    throw new StorageError(ERROR_CODES.conflict, `run ${conflicts.join(", ")} already stored with different content; nothing was imported`);
  }
  const newContents = [];
  let reused = 0;
  for (const item of verified.contents) {
    const stored = existing.contents.get(item.digest);
    if (stored === undefined) {
      newContents.push(item);
    } else if (!sameBytes(stored.bytes, item.bytes)) {
      throw new StorageError(ERROR_CODES.conflict, `content ${item.digest} is stored with different bytes; nothing was imported`);
    } else {
      reused += 1;
    }
  }
  return {
    newRuns,
    newContents,
    summary: { added: newRuns.map((r) => r.id), identical, contentsAdded: newContents.length, contentsReused: reused },
  };
}

function importRecord(entry, contentStatus, now) {
  const missing = referencesOf(entry)
    .filter((ref) => (ref.kind === "source" ? contentStatus.sources[ref.name] : contentStatus[ref.kind]) !== "included")
    .map((ref) => (ref.kind === "source" ? "source " + ref.name : ref.kind));
  return { schema: RECORD_SCHEMA, entry: snapshot(entry), missing, storedAt: now, origin: "evidence package" };
}

function sameBytes(a, b) {
  if (a.byteLength !== b.byteLength) return false;
  for (let i = 0; i < a.byteLength; i++) if (a[i] !== b[i]) return false;
  return true;
}

export class StorageError extends Error {
  constructor(code, message) {
    super(message);
    this.name = "StorageError";
    this.code = code;
  }
}

// Maps a platform error to a StorageError with a stable code. Anything unrecognised is a transaction failure.
export function classifyStorageError(error) {
  if (error instanceof StorageError) return error;
  const name = error?.name ?? "";
  const detail = error?.message ?? String(error);
  if (name === "QuotaExceededError" || error?.code === 22) {
    return new StorageError(ERROR_CODES.quota, "the browser storage quota is full");
  }
  if (name === "ConstraintError") return new StorageError(ERROR_CODES.duplicate, "a run with this id is already stored");
  if (name === "SecurityError" || name === "NotAllowedError") {
    return new StorageError(ERROR_CODES.denied, "the browser does not allow local storage for this page");
  }
  return new StorageError(ERROR_CODES.transaction, "the storage transaction failed: " + detail);
}

// Every digest an entry refers to, with the kind of bytes it stands for. Order: sources, stdout, stderr, figure.
export function digestsOf(entry) {
  const refs = [];
  for (const [name, digest] of Object.entries(entry.digests.sources)) refs.push({ kind: "source " + name, digest });
  if (entry.digests.stdout) refs.push({ kind: "stdout", digest: entry.digests.stdout });
  if (entry.digests.stderr) refs.push({ kind: "stderr", digest: entry.digests.stderr });
  if (entry.digests.figure) refs.push({ kind: "figure", digest: entry.digests.figure });
  return refs;
}

// Builds what is written for one run. `contents` is an array of { bytes: Uint8Array }: the original bytes. Each one is
// hashed here, and only bytes that the entry refers to are accepted. A digest without bytes is recorded as missing;
// nothing is invented. Throws a StorageError before anything is written when the input is inconsistent or too large.
export async function prepareRecord(entry, contents, digestOf) {
  const refs = digestsOf(entry);
  const referenced = new Set(refs.map((ref) => ref.digest));
  const byDigest = new Map();
  for (const item of contents) {
    if (!(item.bytes instanceof Uint8Array)) throw new StorageError(ERROR_CODES.invalid, "stored content must be bytes");
    const digest = await digestOf(item.bytes);
    if (!referenced.has(digest)) {
      throw new StorageError(ERROR_CODES.invalid, `content ${digest} is not referenced by run ${entry.id}`);
    }
    if (!byDigest.has(digest)) byDigest.set(digest, item.bytes.slice());
  }
  let total = 0;
  for (const bytes of byDigest.values()) total += bytes.byteLength;
  if (total > MAX_RUN_CONTENT_BYTES) {
    throw new StorageError(ERROR_CODES.tooLarge, `run ${entry.id} holds ${total} bytes; the limit is ${MAX_RUN_CONTENT_BYTES}`);
  }
  const stateOf = (digest) => (byDigest.has(digest) ? "verified" : "missing");
  const sources = {};
  for (const [name, digest] of Object.entries(entry.digests.sources)) sources[name] = stateOf(digest);
  const verification = {
    sources,
    stdout: entry.digests.stdout ? stateOf(entry.digests.stdout) : null,
    stderr: entry.digests.stderr ? stateOf(entry.digests.stderr) : null,
    figure: entry.digests.figure ? stateOf(entry.digests.figure) : null,
  };
  const missing = refs.filter((ref) => !byDigest.has(ref.digest)).map((ref) => ref.kind);
  return {
    key: entry.id,
    record: {
      schema: RECORD_SCHEMA,
      entry: snapshot(entry),
      missing,
      storedAt: new Date().toISOString(),
    },
    contents: [...byDigest.entries()].map(([digest, bytes]) => ({ key: digest, value: { bytes, size: bytes.byteLength } })),
    verification,
    bytes: total,
  };
}

// Rebuilds a run from its stored record. The record is accepted only when running it through runRecord reproduces the
// stored fields exactly: status, evidence, metadata and digests must all be consistent. Throws with the reason otherwise.
function rebuildEntry(key, value) {
  if (!value || value.schema !== RECORD_SCHEMA) throw new Error("unknown record format");
  const stored = value.entry;
  if (!stored || typeof stored !== "object") throw new Error("the record holds no run entry");
  if (stored.id !== key) throw new Error("the record key does not match the run id");
  const rebuilt = runRecord({ ...stored });
  if (JSON.stringify(rebuilt) !== JSON.stringify(stored)) {
    throw new Error("run fields are inconsistent (status, evidence, metadata or digests were altered)");
  }
  return rebuilt;
}

const strictUtf8 = new TextDecoder("utf-8", { fatal: true });

// Verifies every stored content by recomputing its SHA-256 over the bytes actually stored, then rebuilds each run.
// A damaged run is quarantined on its own: it is reported and not loaded, and the others are unaffected. Nothing is
// deleted. Returns:
//   entries      [{ entry, verification }] for every run that loaded
//   figures      { digest: svg } for every figure whose bytes verified
//   quarantined  [{ key, reason }] runs that could not be rebuilt
//   altered      [digest] stored contents whose bytes no longer match their key
//   damaged      [key] stored contents that are not readable bytes at all
export async function restoreRecords(rawRecords, rawContents, digestOf) {
  const verified = new Map();
  const altered = new Set();
  const damaged = [];
  for (const { key, value } of rawContents) {
    const bytes = value?.bytes;
    if (typeof key !== "string" || !(bytes instanceof Uint8Array)) {
      damaged.push(String(key));
      continue;
    }
    if ((await digestOf(bytes)) === key) verified.set(key, bytes);
    else altered.add(key);
  }

  const entries = [];
  const quarantined = [];
  const figures = {};
  for (const { key, value } of rawRecords) {
    let entry;
    try {
      entry = rebuildEntry(key, value);
    } catch (error) {
      quarantined.push({ key: String(key), reason: error.message });
      continue;
    }
    const stateOf = (digest) => (verified.has(digest) ? "verified" : altered.has(digest) ? "altered" : "missing");
    const sources = {};
    for (const [name, digest] of Object.entries(entry.digests.sources)) sources[name] = stateOf(digest);
    const verification = {
      sources,
      stdout: entry.digests.stdout ? stateOf(entry.digests.stdout) : null,
      stderr: entry.digests.stderr ? stateOf(entry.digests.stderr) : null,
      figure: entry.digests.figure ? stateOf(entry.digests.figure) : null,
    };
    if (verification.figure === "verified") {
      try {
        figures[entry.digests.figure] = strictUtf8.decode(verified.get(entry.digests.figure));
      } catch (_) {
        verification.figure = "altered";
      }
    }
    entries.push({ entry, verification });
  }
  return { entries, figures, quarantined, altered: [...altered], damaged };
}

// Adds restored runs to the in-memory history without replacing any run already there. Runs are listed by start time.
export function mergeHistory(history, restored) {
  const present = new Set(history.entries.map((e) => e.id));
  const added = restored.entries.filter(({ entry }) => !present.has(entry.id));
  const entries = [...history.entries, ...added.map(({ entry }) => entry)].sort(
    (x, y) => (x.startedAt < y.startedAt ? -1 : x.startedAt > y.startedAt ? 1 : x.id < y.id ? -1 : x.id > y.id ? 1 : 0),
  );
  const figures = { ...restored.figures, ...history.figures };
  const verification = Object.fromEntries(added.map(({ entry, verification: v }) => [entry.id, v]));
  return { history: { ...history, entries, figures }, verification };
}

// Serialises asynchronous tasks: each starts after the previous one has settled, whether it succeeded or not, so a
// failed write never blocks the ones queued after it and no two writes overlap.
export function createSerialQueue() {
  let tail = Promise.resolve();
  return (task) => {
    const run = tail.then(() => task());
    tail = run.catch(() => {});
    return run;
  };
}
