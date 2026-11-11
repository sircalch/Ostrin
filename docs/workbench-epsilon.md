# Workbench Epsilon: scientific run history and portable evidence

Epsilon connects the Workbench to a scientific record of its runs. A run executed in the browser is kept with the
original bytes it refers to, can be recovered after a reload, compared under explicit rules, exported as a portable
evidence package, and imported into another browser with clean storage. The format is documented in
[workbench-evidence-package.md](workbench-evidence-package.md).

This note is a closure record. It states what is complete, what was corrected during the audit, what is accepted for
v1, what remains a risk, and what was verified and how.

## Completed

- **Run history model** (`website/workbench-history.js`): immutable entries, real SHA-256 digests supplied by the page,
  incomplete evidence for `failed`, `cancelled` and `timed-out` runs, formal metadata validation, and the comparability
  rules (status, validated results, family, bracket, method, tolerances, compiler identity).
- **Capture of original bytes** (`website/workbench-history-capture.js`): sources as UTF-8, stdout and stderr as raw
  captured bytes, figures as UTF-8, each hashed over those exact bytes.
- **Run History panel** (`website/workbench-history-panel.js`): list, details, provenance digests, comparison with
  reasons when incompatible, shared-parameter differences, exclusive values, references, and both figures.
- **Persistence** (`website/workbench-history-storage.js`, `website/workbench-history-idb.js`): IndexedDB database
  `ostrin-workbench-history`, version 1, with an explicit migration. Each run and its contents commit in one
  transaction. Restore recomputes every SHA-256 over the stored bytes, quarantines a damaged run on its own, and never
  deletes data.
- **Evidence packages** (`website/workbench-evidence.js`): `ostrin.evidence-package/v1`, canonical JSON integrity,
  Base64 contents, explicit content status and coverage, and a verifier that refuses on the first problem.
- **Export and import** (`website/playground.js`, Run History controls): export of one or more runs with re-hashing
  before packaging; import verified in full before any write, then committed atomically.
- **Origin**: imported runs are shown as imported, with the time they were stored, in the list, the details and the
  comparison. Local runs are shown as run in this browser.

## Defects found by the audit, and corrected

Severity follows the brief: critical, high, medium, low.

| Severity | Defect | Evidence | Correction |
| --- | --- | --- | --- |
| High | A resealed package with result rows whose `root` is a string and whose `passed` is a string was accepted. Comparisons would subtract non-numbers. | Probe: `ACCEPTED malformed rows`. | The history model applies the RESULTS validator to every validated result, so the same rule holds for live runs, storage and packages. Test: "rows ... not finite numbers or booleans is refused". |
| High | A resealed package whose declared `parameters.c` disagreed with its result rows was accepted. The details panel could show parameters the table did not contain. | Probe: `ACCEPTED parameters mismatch`. | Parameters must equal `{ c: [rows' c] }` exactly; incomplete evidence carries no parameters. Tests: "declared parameters disagree", "incomplete evidence never carries parameters". |
| Medium | Two rows for the same parameter `c` were accepted. A comparison keeps only one of them without saying so. | Node test before the fix. | The RESULTS validator refuses a repeated `c`. Test: "two result rows for the same parameter c". |
| Medium | Imported and local runs were indistinguishable on screen, although the brief requires them to remain distinguishable. | Panel showed no origin. | The origin is stored with each imported record, restored, and shown in the list, the details and the comparison header. Browser flows A and B assert it. |
| Low | A damaged stored record with the same id as an incoming run produced the message "undefined cannot be canonicalised". The import was still refused atomically. | Node test before the fix. | The planner names the case: "stored as a damaged record; nothing was imported and the record was not changed". |
| Low | The Workbench helper that loads a demo waited on the file list, which was still showing the previous demo. Under parallel load a run could start before its demo loaded and be cancelled by that load. | Intermittent failures in four of seven full runs before the fix; none in the runs after it. | The helpers wait for the demo's own loaded message. The 20-second budget for the completed row is unchanged. This is a test defect, not a product defect: loading a demo cancels a run in flight by design. |
| Low (found during development, fixed then) | The exporter could include contents that no selected run refers to. | Test: "a content referred to by no run". | Only contents referred to by the selected runs are exported. |

## Accepted limitations for v1

- **Authenticity is not provided.** An integrity digest proves that a document is unchanged relative to a digest the
  reader already trusts. Whoever can change the content can recompute every digest. Signatures are out of scope for v1.
- **Reproducibility (R3) is not claimed.** Equal digests mean equal bytes; they do not show that a run can be repeated.
- **No interactive conflict resolution.** A run id with different content refuses the whole package.
- **Manifests are carried as digests only.** Manifest bytes are not exported, and the entry's manifest digest is `null`
  for runs produced today.
- **Duplicate JSON keys are not refused.** The reader takes the last value and the integrity digest covers that value.
  A different parser could take the first value; the package would then be refused, not accepted.
- **Comparability depends on declared metadata**: family, bracket, method, tolerances and compiler identity. The rules
  do not analyse the source. Two programs that declare the same family but differ in their source are comparable under
  the existing rules.
- **Figures are validated only as UTF-8 text**, not as SVG. They are shown as images, so scripts in them do not run, and
  external references are not loaded in that context.
- **Storage is the browser's.** There is no total quota of the store; each run is bounded at 8 MiB, each package at
  24 MiB of contents and 32 MiB of text. Quota failures are reported and nothing partial is kept.
- **Another tab does not refresh the list** until it reloads. Writes are atomic per transaction, so no data is lost,
  but the list can be stale.

## Residual risks

| Risk | Level | Mitigation today | Follow-up |
| --- | --- | --- | --- |
| A reader that trusts an integrity digest from the same file gets no protection against a deliberate rewrite. | High for the claim, low for the software | Documented in the format and in the Run History note. | Signature or external trusted digest (future). |
| Scientific comparisons trust declared family and method strings. | Medium | Documented as a scientific limitation; the rules are unchanged. | Source identity or a declared model hash, as a separate design. |
| Importing a 128 MiB file reads it into memory before verification. | Medium | The file size is checked before reading; the text limit is 32 MiB of characters. | Streaming verification if larger packages are needed. |
| Duplicate JSON keys could be read differently by another implementation. | Low | Refusal of inconsistent readers; documented. | Strict JSON parser in a later phase. |
| Duplicate SHA-256 helpers exist in `workbench-project.js` and `workbench-history-capture.js`. | Low | Both compute the same digest over the same bytes. | Consolidate when the project module is next changed. |
| Run History orchestration lives in `playground.js`, which is large. | Low | The model, storage, packages and panel are separate modules. | Move the orchestration to its own module in a later phase. |

## Verification

The results are those observed on the branch `horizon/workbench-epsilon` at the closure commit. Nothing below is
expected: each item was run.

- **Node** (`node --test scripts/workbench-*.test.mjs`): 134 tests, 134 passing, 0 failing. The set covers the history
  model, storage, evidence packages, the results contract and the demos.
- **Chromium** (`npx playwright test` in `tests/browser` with `--project=chromium`, for the Alpha, Beta, Gamma, Delta,
  Epsilon, persistence, evidence and website specs): 84 passing in each full run, repeated consecutively.
- **Flows requested by the brief**, all in Chromium with the real compiler: a run recorded and its results and figure
  shown; a reload that recovers the same identifier and results; two runs compared; export of one run and of two;
  import into a clean browser context; digests recomputed in Node over every content; no automatic run after import
  (the timing hook is absent); a tampered package refused whole with no partial record.
- **Repository checks**, run locally: `linguist-check`, `project-plan-check`, `fallback-ledger-check`,
  `measurement-contract-check`, `quantity-ownership-check`, `website-metadata`, `provenance-page --check`,
  `website-check --wasm`, `distribution-check`, `svg-references-check`, and `workbench-demo` (document drift). Each
  passes, and the unit tests for the checks that have them pass.

### Not verified here

- **Firefox and WebKit**: only Chromium is installed locally. CI runs all three in `website-verify` (`npm test`).
  These engines are pending until CI reports them.
- **Windows visual suite**: `visual.spec.mjs` has one failing test locally, "scientific plot visual fixtures match
  their published gallery cards". Its baselines for win32 were never refreshed; the failure predates Epsilon, and
  Epsilon does not modify the viz gallery files. It is recorded as failing, not as passing, and it needs a separate
  decision about the baselines.
- **Native compiler, sanitizers, benchmarks and release workflows** are in CI on other platforms and were not run here.

## Future work (separate phases)

- Signatures or an externally trusted digest for packages.
- Interactive conflict resolution for imports.
- Export of manifest bytes.
- A combined comparative figure for two runs.
- Strict JSON parsing that refuses duplicate keys.
- Source identity or a declared model hash in the comparability rules, designed separately from the current rules.
- Cross-tab notification of new runs.
- Streaming verification for packages larger than the current limits.
