// Execution controller for the Workbench. It owns at most one active run and one idle Worker, and it
// is the only place that terminates a Worker. The Worker is created by the injected `createWorker`
// factory, so the controller can be tested in Node with a fake Worker.
//
// Policy:
//   - One active run. A new run replaces the active one: the active run is cancelled (its Worker is
//     terminated) and its late replies can never be published.
//   - Cancel and timeout terminate the Worker, because a synchronous WebAssembly run cannot be
//     interrupted cooperatively.
//   - A Worker that finished a run normally is kept for the next run. The compiled module lives in the
//     Worker, so the next run does not pay the compile cost again.
//   - Every reply is checked against the active run's id and state before it can change anything.

import {
  acceptsReply,
  createRun,
  LIMITS,
  parseWorkerReply,
  transition,
  validateRunRequest,
} from "./workbench-protocol.js";

export function createExecutor({ createWorker, onChange = () => {}, setTimer = setTimeout, clearTimer = clearTimeout }) {
  let idleWorker = null;
  let active = null; // { run, worker, timer, resolve, reject, request }
  let counter = 0;

  function nextId() {
    counter += 1;
    return `run-${counter}-${Date.now().toString(36)}`;
  }

  function publish(run, detail = {}) {
    onChange({ ...run, ...detail });
  }

  function stopWorker(worker) {
    if (worker) worker.terminate();
  }

  function finish(to, detail = {}) {
    const current = active;
    if (!current) return null;
    clearTimer(current.timer);
    active = null;
    current.run = transition(current.run, to, detail);
    publish(current.run);
    return current;
  }

  function settle(current, value) {
    current.resolve(value);
  }

  function onWorkerMessage(run, event) {
    if (!active || active.run.id !== run.id) return; // a reply from a Worker that is no longer active
    let reply;
    try {
      reply = parseWorkerReply(event.data);
    } catch (error) {
      failActive("runtime-error", `invalid worker reply: ${error.message}`);
      return;
    }
    if (reply.type === "ready") return;
    if (!acceptsReply(active.run, reply)) return;
    if (reply.type === "started") {
      if (active.run.state === "loading") {
        active.run = transition(active.run, "running");
        publish(active.run);
      }
      return;
    }
    if (reply.type === "runtime-error") {
      failActive("runtime-error", reply.message);
      return;
    }
    if (reply.type === "result") {
      // A result proves the program started, even when its started message was not seen.
      if (active.run.state === "loading") {
        active.run = transition(active.run, "running");
      }
      // Page-side measures: the time the reply took to arrive, the Worker's reuse and creation cost, and the total.
      const receivedAt = performance.timeOrigin + performance.now();
      const timing = {
        ...(reply.timing ?? {}),
        workerReused: active.start?.reused ?? false,
        workerCreateMs: active.start?.createMs ?? 0,
        transferMs: reply.timing ? receivedAt - reply.timing.repliedAt : null,
        totalMs: performance.now() - active.startedAt,
      };
      const current = finish("completed", { result: { code: reply.code, stdout: reply.stdout, stderr: reply.stderr, ordered: reply.ordered, timing } });
      keepWorker(current);
      settle(current, { state: "completed", result: current.run.result, id: current.run.id });
    }
  }

  function onWorkerError(run, error) {
    if (!active || active.run.id !== run.id) return;
    failActive("worker-terminated", `the Worker stopped unexpectedly: ${error?.message ?? "unknown error"}`);
  }

  function keepWorker(current) {
    // Only a Worker whose run ended normally is reused. The active worker is already idle now.
    if (current?.worker) idleWorker = current.worker;
  }

  function failActive(kind, message) {
    const current = finish("failed", { error: { kind, message } });
    if (!current) return;
    stopWorker(current.worker);
    if (idleWorker === current.worker) idleWorker = null;
    settle(current, { state: "failed", error: current.run.error, id: current.run.id });
  }

  // Returns the Worker for a run and records whether it was reused and what its creation cost.
  // `createMs` is zero for a reused Worker; for a new one it is the synchronous cost of the constructor call.
  function startWorker(run) {
    if (idleWorker) {
      const worker = idleWorker;
      idleWorker = null;
      lastStart = { reused: true, createMs: 0 };
      return worker;
    }
    const createStart = performance.now();
    const worker = createWorker();
    lastStart = { reused: false, createMs: performance.now() - createStart };
    return worker;
  }
  let lastStart = { reused: false, createMs: 0 };


  return {
    // Starts a run. Returns a promise that resolves with { state, ... } when the run ends in any state
    // (completed, failed, cancelled or timed-out). It never rejects for a run; a request that fails
    // validation throws synchronously, before anything starts.
    run(request) {
      const { files, entry, command, timeoutMs = LIMITS.defaultTimeoutMs } = request;
      const problems = validateRunRequest({ type: "run", id: "validate-only-run", command, files, entry });
      if (problems.length > 0) throw new Error(`invalid run: ${problems.join("; ")}`);

      if (active) {
        // Replace the active run: it is cancelled and its Worker is terminated.
        const replaced = finish("cancelled", { reason: "replaced by a newer run" });
        stopWorker(replaced.worker);
        settle(replaced, { state: "cancelled", id: replaced.run.id, reason: "replaced by a newer run" });
      }

      const id = nextId();
      const run = createRun(id, { timeoutMs });
      return new Promise((resolve) => {
        const current = { run, worker: null, timer: null, resolve, request, startedAt: performance.now(), start: null };
        active = current;
        current.run = transition(current.run, "loading");
        publish(current.run);
        current.timer = setTimer(() => {
          const timedOut = finish("timed-out", { error: { kind: "timeout", message: `no result within ${timeoutMs} ms` } });
          if (timedOut) {
            stopWorker(timedOut.worker);
            if (idleWorker === timedOut.worker) idleWorker = null;
            settle(timedOut, { state: "timed-out", id: timedOut.run.id, error: timedOut.run.error });
          }
        }, timeoutMs);
        // A Worker that cannot be created or cannot accept the request ends the run as failed, with its timer
        // cleared and no active reference left behind.
        let worker;
        try {
          worker = startWorker(run);
          current.start = lastStart;
          worker.onmessage = (event) => onWorkerMessage(run, event);
          worker.onerror = (event) => onWorkerError(run, event);
          current.worker = worker;
          worker.postMessage({ type: "run", id, command, entry, files });
        } catch (error) {
          if (worker) stopWorker(worker);
          if (idleWorker === worker) idleWorker = null;
          const failed = finish("failed", { error: { kind: "worker-start", message: String(error?.message ?? error) } });
          if (failed) settle(failed, { state: "failed", id: failed.run.id, error: failed.run.error });
        }
      });
    },

    // Cancels the active run by terminating its Worker. Returns the cancelled run, or null when idle.
    cancel() {
      if (!active) return null;
      const current = finish("cancelled", { reason: "cancelled by the user" });
      stopWorker(current.worker);
      settle(current, { state: "cancelled", id: current.run.id, reason: "cancelled by the user" });
      return current.run;
    },

    // Terminates every Worker and drops every reference. Used when the page is replaced or closed.
    dispose() {
      if (active) this.cancel();
      stopWorker(idleWorker);
      idleWorker = null;
    },

    get activeRun() {
      return active ? active.run : null;
    },

    get hasIdleWorker() {
      return idleWorker !== null;
    },
  };
}
