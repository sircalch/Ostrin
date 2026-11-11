# Portable evidence packages (`ostrin.evidence-package/v1`)

An evidence package is one JSON file that carries runs of the Workbench Run History together with the original bytes
they refer to. It can be moved to another browser, another device or another person and verified there, without the
storage in which the experiment ran.

- Schema: `ostrin.evidence-package/v1`
- File extension: `.ostrin.evidence.json`
- MIME type: `application/vnd.ostrin.evidence+json`
- Integrity algorithm: SHA-256 over the canonical JSON of the document, `ostrin-canonical-json/v1`
- Implementation: `website/workbench-evidence.js` (pure; the hash function is injected)

The schema is independent of the IndexedDB version of the local store (`ostrin-workbench-history`, version 1) and of
the run-entry format `ostrin.history/v1`. A package never contains IndexedDB references.

## What can and cannot be verified

**Can be verified from the file alone:**

- That every byte sequence declared as included decodes from its Base64 and has the declared length.
- That the SHA-256 of each decoded byte sequence equals the digest that the runs refer to.
- That the document has not changed since it was sealed, relative to the integrity digest the reader holds.
- That every run is internally consistent: its status, evidence, validated results and digests agree under the same
  rules the Run History uses.
- That every content is referred to by a run, every run's declared content status agrees with the contents present,
  and the coverage stated for each run is the one its evidence gives.

**Cannot be verified from the file alone:**

- **Authenticity.** An integrity digest stored in the same file, or next to it, proves consistency, not authorship. Whoever
  can change the content can recompute every digest and the integrity digest. A package is authenticated only when its
  integrity digest is obtained through a channel the reader trusts (a signature, a published hash, a repository).
  Signing is not implemented in v1.
- **Reproducibility (R3).** Equal digests mean equal bytes. They do not show that the program would produce those bytes
  again on another machine, compiler or input. R3 is not claimed.
- **Scientific correctness.** A package shows what a run printed and the results it validated, not whether the model or
  the tolerances are the right ones.
- **Absence of other runs**, or that the list of runs is complete.

## Structure

```json
{
  "schema": "ostrin.evidence-package/v1",
  "createdAt": "2026-11-10T12:00:00.000Z",
  "producer": { "name": "ostrin playground" },
  "coverage": { "runs": 2, "complete": 1, "partialContent": 0, "incompleteRun": 1, "incompleteResults": 0 },
  "runs": [
    {
      "entry": { "...every field of ostrin.history/v1 entry..." },
      "contentStatus": {
        "sources": { "main.ostrin": "included" },
        "stdout": "included",
        "stderr": null,
        "figure": "included"
      },
      "coverage": "complete"
    }
  ],
  "contents": [
    { "digest": "<sha256 hex>", "kind": "source", "encoding": "base64", "size": 21, "data": "<base64>" }
  ],
  "withheld": [],
  "integrity": {
    "algorithm": "sha256",
    "canonicalization": "ostrin-canonical-json/v1",
    "covers": "every field except integrity",
    "digest": "<sha256 hex of the canonical JSON of everything except integrity>"
  }
}
```

### Required fields

| Field | Meaning |
| --- | --- |
| `schema` | Must be `ostrin.evidence-package/v1`. Any other value is refused. |
| `createdAt` | UTC ISO 8601 time of the export. |
| `producer.name` | Free text, at most 100 characters. |
| `coverage` | Summary counts. Recomputed by the reader and must match. |
| `runs[]` | One object per run: `entry`, `contentStatus`, `coverage`. At least one, at most 100. |
| `contents[]` | Original bytes. Each object has `digest`, `kind`, `encoding`, `size`, `data`. At most 500. |
| `withheld[]` | Digests whose bytes were not exported, each with a short `reason`. |
| `integrity` | Algorithm, canonicalization, the covered fields and the digest. |

Unknown fields are refused in the package, in each run object, in each content, in each withheld entry and in the run
entry (the rebuilt entry must equal the stored one exactly). The `results` document of a run is validated for the fields it
requires; additional fields inside it are preserved and covered by the integrity digest, but are never interpreted.

### Run entries

`runs[].entry` is the Run History entry exactly as `ostrin.history/v1` defines it: `id` (the original run identifier),
`projectId`, `projectName`, `entry`, `command`, `status`, `evidence`, `exitCode`, `compilerIdentity`, `startedAt`,
`durationMs`, `parameters`, `results` (the validated RESULTS document or `null`), `resultsRejected`, and `digests`
(`sources`, `stdout`, `stderr`, `figure`, `manifest`). The reader rebuilds the entry through the same validation as the
history model and requires the result to be identical. In addition:

- every result row has finite numbers for `c`, `root`, `residual`, `residual_bound`, `analytic_derivative` and `numeric_derivative`, a boolean `passed`, a string `message`, a known `status`, and a `c` that appears once;
- `parameters`, when present, is exactly `{ "c": [...] }` with the parameter values of the rows, in the same order;
- an incomplete run (not `completed`, or without validated results) carries no `parameters` and no `results`.

`runs[].contentStatus` says, for every digest the entry refers to, what the package carries:

- `included`: the bytes are in `contents` and were verified.
- `digest-only`: the run refers to this digest, but its bytes were never stored in the browser where the run happened.
  The package says so explicitly; nothing is invented.
- `withheld`: the bytes existed locally but did not match their digest at export time. They are listed in `withheld`
  with the reason and are not exported as evidence.

`runs[].coverage` is one of:

- `complete`: validated results and every declared content included.
- `partial-content`: validated results, but some declared content is `digest-only` or `withheld`.
- `incomplete-run`: the run did not complete (`failed`, `cancelled` or `timed-out`). It has no validated results.
- `incomplete-results`: the run completed, but no validated structured results were recorded.

Coverage is never upgraded by the presence of content. A cancelled run with a source is still `incomplete-run`.

### Contents

Each content is encoded in standard Base64 (RFC 4648, padded). Its digest is computed over the **decoded bytes as
originally captured**:

- sources and figures: the UTF-8 bytes of the text;
- stdout and stderr: the raw bytes the run captured.

Nothing is re-serialised, normalised or re-encoded before hashing.

`kind` is one of `source`, `stdout`, `stderr`, `figure`, and must be a kind under which some run refers to the digest.
Sources and figures must be valid UTF-8. Each digest appears at most once.

## Integrity

1. The document is serialised with object keys sorted by code unit and no insignificant whitespace (canonical JSON).
   Non-finite numbers are refused.
2. The canonical bytes of every field except `integrity` are hashed with SHA-256. The result is `integrity.digest`.
3. The reader repeats step 2 before anything else. A changed field, even inside metadata, fails here.
4. Each content is decoded, its length checked against `size`, and its SHA-256 recomputed over the decoded bytes.
5. Every run is rebuilt through the history model and compared with its stored fields.
6. Every cross-reference is checked: references and kinds, declared statuses against the contents present, coverage
   against the evidence, and the package summary against the runs.

If step 2 passes but step 4 fails, the file was resealed around altered bytes. If step 3 fails, the file was changed
without resealing.

## Limits

| Limit | Value |
| --- | --- |
| Package text | 32 MiB of characters (the file is refused above 128 MiB in bytes) |
| Runs per package | 100 |
| Contents per package | 500 |
| One content | 8 MiB |
| All contents together | 24 MiB |
| Nesting depth | 12 |
| Values per document | 200 000 |

Export refuses a selection that exceeds these limits and says which one. Import checks the limits before parsing.

## Import

Import is validated completely before any write:

1. The file size is checked before it is read.
2. The text is parsed as JSON. Malformed JSON is refused.
3. Depth and size of the structure are bounded.
4. Schema, integrity, runs, contents and cross-references are verified in that order.

Then the store plans the import against what it already holds:

- A run id not present is added.
- A run id present with **identical** content is reported as identical and is not duplicated.
- A run id present with **different** content is a conflict. The whole import is refused. Nothing is overwritten and no
  explicit resolution is offered in v1.
- Content already stored under the same digest is reused, after comparing its bytes. Different bytes under the same
  digest are a conflict.

The writes are one readwrite transaction. A quota error, a transaction error or a concurrent duplicate aborts it, so no
run is committed without its contents, and no content without its run.

Importing never executes a program, never evaluates text, and never interprets an SVG as markup. Figures are displayed
as `<img>` with a data URL, which does not run scripts. Names and text are inserted as text.

## Export

Run History exports the runs selected with the checkboxes, or a single run from its details. For each digest the exporter
looks for the bytes in the store, and falls back to the figure text in memory if the figure was never saved. Every byte
sequence is hashed again before it is packaged; a mismatch withholds it. The result is downloaded with a safe file name
built from the run identifier and the time, for example `ostrin-evidence-run-…-2026-11-10T12-00-00.ostrin.evidence.json`.

A package can contain source code and program output, which may be sensitive. The Run History says so next to the
export controls.

## Compatibility

- Readers accept only the schema versions they know. This reader accepts `ostrin.evidence-package/v1` only; any other
  schema value, such as a v2 package, is refused with `unknown-schema`. Nothing is silently reinterpreted.
- A future v2 changes the schema string. Fields are never reused with a different meaning inside v1.
- The canonicalisation and integrity algorithm are named in the file, so a reader can refuse an algorithm it does not
  implement.

## Usage

Export one run: select it in Run History, then press **Export this run as evidence**.

Export two runs for comparison: tick both checkboxes and press **Export selected runs (2)**.

Import: press **Import evidence package…** and choose the file. The result, and any refusal with its reason, is shown
above the list.

Verify a package outside the browser, with Node:

```js
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
const pkg = JSON.parse(readFileSync("run.ostrin.evidence.json", "utf8"));
for (const item of pkg.contents) {
  const bytes = Buffer.from(item.data, "base64");
  console.log(item.kind, item.digest, createHash("sha256").update(bytes).digest("hex") === item.digest);
}
```

This checks the contents only. Use `verifyPackage` from `website/workbench-evidence.js` for the full verification.

## Scientific conditions for a comparison

Two runs are compared only when the existing comparability rules hold: both completed with validated results, the same
model family, bracket, method and tolerances, and the same compiler identity. The experiment name, a shared run id, shared
source text or equal numbers do not make runs comparable or reproducible. Parameters present in only one run are listed
as exclusive to it, and no difference is computed for them. An imported run is compared under exactly the same rules as a
local one.

## Known limitations (v1)

- No signature or external trusted digest. Authenticity is not provided.
- Import has no per-run resolution for conflicts; a conflict refuses the whole package. A run id that is already stored as a damaged record is also refused, and the record is not changed.
- Duplicate JSON object keys are not refused. The reader takes the last value, and the integrity digest covers that value. Another JSON reader could take the first value; then its digest would not match, and the package would be refused rather than accepted.
- Comparability is decided by the metadata the program declares (family, bracket, method, tolerances) and by the compiler identity. It does not analyse the source. Two programs that declare the same family but differ in their source are comparable under the rules; this is the existing rule set, and it is a scientific limitation, not a defect of the format.
- Imported runs are marked with their origin (`imported from an evidence package`, with the time they were stored). Local runs are marked `run in this browser`. Records saved before imports existed count as local.
- A second browser tab does not see runs saved by the first one until it reloads; the store is per page and writes are checked by IndexedDB, so no data is lost, but the list can be stale.
- Manifest documents (`digests.manifest`) are carried as digests only; manifest bytes are not exported.
- Two figures are shown side by side; no combined comparative figure exists yet.
- Storage quotas are the browser's. A package larger than the limits must be split by the user.
- Verification of the package in the browser runs in the page; it depends on the browser's SHA-256 implementation.

## Reproducing the checks

These are the commands that verify this format and the Run History. They run on Windows, macOS and Linux with Node 24
and a built `website/ostrinc.wasm` (the browser tests need it).

```bash
# Node: history, storage, evidence package and the other Workbench model tests
node --test scripts/workbench-*.test.mjs

# Repository checks that cover the website and the demos
node scripts/workbench-demo.mjs
node scripts/website-check.mjs --wasm
node scripts/linguist-check.mjs

# Chromium: the Epsilon flows, persistence, evidence packages and the earlier phases
cd tests/browser
npx playwright test workbench-epsilon.spec.mjs workbench-persistence.spec.mjs workbench-evidence.spec.mjs --project=chromium
```

Firefox and WebKit are run by CI only (`npm test` in `tests/browser` with all projects). The Windows visual suite is
recorded separately in the Epsilon closure note.
