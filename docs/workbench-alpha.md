# Ostrin Workbench Alpha

The Playground (`website/playground.html`) works on a project: several `.ostrin` files, one entry
file, unsaved-change tracking, runs through the real compiler, and project and manifest downloads.

## Project format `ostrin.project` version 1

A JSON document:

```json
{ "format": "ostrin.project", "version": 1, "name": "demo", "entry": "main.ostrin",
  "files": { "main.ostrin": "...", "helpers.ostrin": "..." } }
```

- File paths are relative, use `/`, end in `.ostrin`, and may not contain `..`, a leading `/` or a
  backslash. Limits: 32 files, 256 KB per file, 2 MB per project, 80 characters for the name.
- The entry file must be one of the project's files.
- Serialization is stable: the same project always produces the same bytes.
- Validation happens before any state is replaced. An invalid import leaves the current project as it was.

The model lives in `website/workbench-project.js`; the state controller in `website/workbench-controller.js`;
the explorer in `website/workbench-explorer.js`. The Node tests are `scripts/workbench-project.test.mjs`
and `scripts/workbench-controller.test.mjs`. The browser tests are `tests/browser/workbench.spec.mjs`.

## Running

- Run, Check and Test run the project from its entry file with the compiler from `ostrinc.wasm`.
  Multi-file imports resolve across the project's files; imported functions must be `pub`.
- Format applies to the active file.
- Live demos and the single-file examples keep their previous single-file path.
- Each run uses an immutable copy of the project. Edits made after a run starts do not change its record.

## Results and manifest

- The output separates stdout and stderr and records the command, the entry, and the exit code.
- SVG figures from `--run` can be downloaded.
- The manifest (`run-manifest.json`) contains SHA-256 hashes of every source file, stdout, stderr and the
  figure, computed from the run's input snapshot.
- The compiler identity is the line printed by `ostrinc --version` in the same runtime. When that
  line is not available the manifest says `unknown`; no version is invented.
- The manifest makes **integrity** claims only. It does not claim reproducibility: `claims.reproducibility`
  is `not assessed (no R3 claim)`.

## Storage and security

- Project drafts are kept in `localStorage` only as a convenience. Storage can be blocked; the page
  keeps working without it, and the project file is the durable copy.
- Imported projects are never run automatically.
- The Workbench runs the compiler on the page's main thread. Long programs can block the page;
  a Web Worker is not implemented yet and no cancellation is offered.

## Not yet covered

- Windows visual baselines for the scientific-plot cards.
- A Web Worker or other off-thread execution.
- Multi-file projects in the live demos.
