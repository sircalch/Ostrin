# Workbench Gamma: execution in a Worker

Runs of a Workbench project execute in a Web Worker (`website/workbench-worker.js`), so the editor stays
responsive. The compiler is the same `ostrinc.wasm` the page used before.

## Layers

- `workbench-protocol.js`: message validation, the run state machine and `orderedLines`.
- `workbench-execution.js`: the controller. One active run; a new run replaces the active one; cancel and
  timeout terminate the Worker; a finished Worker is reused.
- `workbench-worker.js`: compiles the module once per Worker and captures the exact output bytes.
- `workbench-presentation.js`: the interface-level guard. A result, manifest, figure or control state is
  published only while its ticket is current.
- `playground.js`: connects them. Run, Check and Test go through the executor; Cancel terminates the Worker.

## Why cancellation terminates the Worker

A synchronous WebAssembly run cannot be interrupted cooperatively. The only real stop is terminating the Worker,
so cancel and timeout do that. A cancelled run is marked cancelled, and any later reply from its Worker is ignored.

## Stale results

Three checks protect the screen, in order:

1. The controller accepts a reply only for the active run id and a live state.
2. The presentation guard issues a ticket per Run. A run whose ticket is no longer current does not write its
   result, record evidence, attach a figure or re-enable the controls. A project change invalidates every ticket.
3. The controls are re-enabled by the current run, or by a stale one only when no run is active.

Tests: `scripts/workbench-presentation.test.mjs` (guard rules), `scripts/workbench-execution.test.mjs`
(replacement, stale and foreign replies, initialization failures), and `tests/browser/workbench-gamma.spec.mjs`
(cancel then run, project change during a run, two Run clicks, time limit).

## Initialization failures

- A Worker that cannot be created, or that rejects the request, ends the run as `failed` with `worker-start`;
  its timer is cleared and no reference is kept.
- A WebAssembly load failure is reported by the Worker as `runtime-error`.
- A Worker that stops before `started` ends the run as `failed` with `worker-terminated`.

## Output order

Each Worker write is a tagged chunk. `orderedLines` defines the contract:

- A line sits where its newline arrived (its completion).
- An unterminated final line sits where its first byte was written (its start).
- stdout and stderr each have their own UTF-8 decoder, so a multi-byte character split across writes of the
  same stream decodes intact, and bytes of one stream never enter the other.

The display order is therefore the emission order of lines, across both streams. It is not the order of
individual bytes, which the page does not display.

## Figures and errors

A figure is taken from stdout. Errors (stderr) are shown above the figure, as a separate group, so the figure
is never presented as a line of the log. The panel does not claim to rebuild a single sequence that mixes an
SVG document with the log text.

## Evidence

The manifest hashes the captured bytes when they were recorded (`source: "raw runtime bytes"`), and the
normalized text otherwise (`source: "normalized text"`). Integrity claims only: R3 reproducibility is not claimed.

## Limits

- The main-thread runtime is still used by live demos and Format on single-file examples. There is no automatic
  fallback when a Worker is unavailable.
- Memory is not limited by the page; no memory guarantee is made.
- Performance (start-up, latency, memory) is not measured in this document; see the Delta package.
