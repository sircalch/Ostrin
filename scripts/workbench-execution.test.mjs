import test from "node:test";
import assert from "node:assert/strict";
import { createExecutor } from "../website/workbench-execution.js";
import { createRun, orderedLines, parseWorkerReply, transition, validateRunRequest, acceptsReply, LIMITS } from "../website/workbench-protocol.js";

// A Worker stand-in: records what the page sends, lets the test deliver replies, and counts terminations.
class FakeWorker {
  constructor() {
    this.sent = [];
    this.terminated = false;
    this.onmessage = null;
    this.onerror = null;
  }
  postMessage(message) {
    assert.equal(this.terminated, false, "the page must not send to a terminated Worker");
    this.sent.push(message);
  }
  terminate() {
    this.terminated = true;
  }
  deliver(data) {
    this.onmessage?.({ data });
  }
  crash(message) {
    this.onerror?.({ message });
  }
}

function harness() {
  const workers = [];
  const timers = [];
  const changes = [];
  const executor = createExecutor({
    createWorker: () => {
      const worker = new FakeWorker();
      workers.push(worker);
      return worker;
    },
    onChange: (run) => changes.push(run.state),
    setTimer: (callback, ms) => {
      const timer = { callback, ms, cleared: false };
      timers.push(timer);
      return timer;
    },
    clearTimer: (timer) => {
      if (timer) timer.cleared = true;
    },
  });
  return { executor, workers, timers, changes };
}

const request = (id = "x") => ({
  command: ["--run", "main.ostrin"],
  entry: "main.ostrin",
  files: { "main.ostrin": "fn main() -> Void {\n    print(\"49\")\n}\n", "helpers.ostrin": "pub fn one() -> Int {\n    1\n}\n" },
  timeoutMs: 5000,
  ...(id === "x" ? {} : {}),
});

const bytes = (text) => new TextEncoder().encode(text);

test("a run completes with raw stdout and stderr bytes and the Worker is kept for the next run", async () => {
  const { executor, workers, changes } = harness();
  const promise = executor.run(request());
  const worker = workers[0];
  assert.equal(worker.sent.length, 1);
  const id = worker.sent[0].id;
  worker.deliver({ type: "started", id });
  worker.deliver({ type: "result", id, code: 0, stdout: bytes("49\n"), stderr: bytes(""), ordered: [] });
  const outcome = await promise;
  assert.equal(outcome.state, "completed");
  assert.equal(new TextDecoder().decode(outcome.result.stdout), "49\n", "stdout keeps its exact bytes, including the final newline");
  assert.equal(outcome.result.code, 0);
  assert.equal(worker.terminated, false, "a normally finished Worker stays alive for reuse");
  assert.deepEqual(changes, ["loading", "running", "completed"]);

  const second = executor.run(request());
  assert.equal(workers.length, 1, "the next run reuses the idle Worker instead of creating another");
  worker.deliver({ type: "result", id: worker.sent[1].id, code: 0, stdout: bytes(""), stderr: bytes(""), ordered: [] });
  assert.equal((await second).state, "completed");
});

test("a compiler error is a completed run with a non-zero code, not a runtime failure", async () => {
  const { executor, workers } = harness();
  const promise = executor.run(request());
  const id = workers[0].sent[0].id;
  workers[0].deliver({ type: "result", id, code: 1, stdout: bytes(""), stderr: bytes("error: boom\n"), ordered: [] });
  const outcome = await promise;
  assert.equal(outcome.state, "completed");
  assert.equal(outcome.result.code, 1);
  assert.equal(new TextDecoder().decode(outcome.result.stderr), "error: boom\n");
});

test("the ordered output keeps stdout and stderr interleaved as the program wrote them", async () => {
  const { executor, workers } = harness();
  const promise = executor.run(request());
  const worker = workers[0];
  const id = worker.sent[0].id;
  const chunk = (stream, text) => ({ stream, bytes: bytes(text) });
  worker.deliver({
    type: "result",
    id,
    code: 1,
    stdout: bytes("first\nthird\n"),
    stderr: bytes("second\n"),
    ordered: [chunk("out", "first\n"), chunk("err", "second\n"), chunk("out", "third\n")],
  });
  const outcome = await promise;
  assert.equal(outcome.state, "completed");
  assert.deepEqual(orderedLines(outcome.result.ordered), [["out", "first"], ["err", "second"], ["out", "third"]]);
});

test("cancel terminates the Worker and a late reply from it is never published", async () => {
  const { executor, workers, changes } = harness();
  const promise = executor.run(request());
  const worker = workers[0];
  const id = worker.sent[0].id;
  worker.deliver({ type: "started", id });
  const cancelled = executor.cancel();
  assert.equal(cancelled.state, "cancelled");
  assert.equal(worker.terminated, true, "the Worker is terminated, which is the only way to stop synchronous WASM");
  const outcome = await promise;
  assert.equal(outcome.state, "cancelled");
  worker.deliver({ type: "result", id, code: 0, stdout: bytes("late"), stderr: bytes(""), ordered: [] });
  assert.deepEqual(changes, ["loading", "running", "cancelled"], "no state change after cancellation");
  assert.equal(executor.activeRun, null);
});

test("cancelling an idle executor does nothing", () => {
  const { executor } = harness();
  assert.equal(executor.cancel(), null);
});

test("a second run replaces the active one: the first is cancelled and cannot publish over the second", async () => {
  const { executor, workers } = harness();
  const first = executor.run(request());
  const firstWorker = workers[0];
  const firstId = firstWorker.sent[0].id;
  const second = executor.run(request());
  assert.equal((await first).state, "cancelled");
  assert.equal(firstWorker.terminated, true);
  const secondWorker = workers[1];
  assert.notEqual(secondWorker, firstWorker);
  const secondId = secondWorker.sent[0].id;
  assert.notEqual(secondId, firstId);

  firstWorker.deliver({ type: "result", id: firstId, code: 0, stdout: bytes("old"), stderr: bytes(""), ordered: [] });
  secondWorker.deliver({ type: "result", id: secondId, code: 0, stdout: bytes("new\n"), stderr: bytes(""), ordered: [] });
  const outcome = await second;
  assert.equal(outcome.state, "completed");
  assert.equal(new TextDecoder().decode(outcome.result.stdout), "new\n", "only the newer run's result is published");
});

test("a reply that names another run id is ignored", async () => {
  const { executor, workers } = harness();
  const promise = executor.run(request());
  const worker = workers[0];
  const id = worker.sent[0].id;
  worker.deliver({ type: "result", id: "some-other-run", code: 0, stdout: bytes("x"), stderr: bytes(""), ordered: [] });
  assert.equal(executor.activeRun.state, "loading", "the foreign reply changed nothing");
  worker.deliver({ type: "result", id, code: 0, stdout: bytes("49\n"), stderr: bytes(""), ordered: [] });
  assert.equal((await promise).state, "completed");
});

test("timeout terminates the Worker and reports timed-out", async () => {
  const { executor, workers, timers } = harness();
  const promise = executor.run(request());
  const worker = workers[0];
  assert.equal(timers.length, 1);
  assert.equal(timers[0].ms, 5000);
  timers[0].callback();
  const outcome = await promise;
  assert.equal(outcome.state, "timed-out");
  assert.equal(outcome.error.kind, "timeout");
  assert.equal(worker.terminated, true);
});

test("a finished run clears its timer, so a late timeout cannot fire", async () => {
  const { executor, workers, timers } = harness();
  const promise = executor.run(request());
  const id = workers[0].sent[0].id;
  workers[0].deliver({ type: "result", id, code: 0, stdout: bytes(""), stderr: bytes(""), ordered: [] });
  await promise;
  assert.equal(timers[0].cleared, true);
});

test("a runtime error fails the run and terminates its Worker", async () => {
  const { executor, workers } = harness();
  const promise = executor.run(request());
  const worker = workers[0];
  worker.deliver({ type: "runtime-error", id: worker.sent[0].id, message: "trap: unreachable" });
  const outcome = await promise;
  assert.equal(outcome.state, "failed");
  assert.equal(outcome.error.kind, "runtime-error");
  assert.equal(worker.terminated, true);
});

test("a Worker that crashes is reported as terminated unexpectedly", async () => {
  const { executor, workers } = harness();
  const promise = executor.run(request());
  workers[0].crash("out of memory");
  const outcome = await promise;
  assert.equal(outcome.state, "failed");
  assert.equal(outcome.error.kind, "worker-terminated");
  assert.match(outcome.error.message, /out of memory/);
});

test("a malformed reply fails the run instead of being trusted", async () => {
  const { executor, workers } = harness();
  const promise = executor.run(request());
  workers[0].deliver({ type: "result", id: workers[0].sent[0].id, code: "zero", stdout: "text", stderr: bytes(""), ordered: [] });
  const outcome = await promise;
  assert.equal(outcome.state, "failed");
  assert.match(outcome.error.message, /invalid worker reply/);
});

test("an invalid request throws before any Worker is created", () => {
  const { executor, workers } = harness();
  assert.throws(() => executor.run({ command: ["--run"], entry: "missing.ostrin", files: { "main.ostrin": "x" } }), /invalid run/);
  assert.equal(workers.length, 0);
  assert.throws(() => executor.run({ ...request(), timeoutMs: 10 }), /timeout must be between/);
});

test("dispose terminates the active and the idle Worker", async () => {
  const { executor, workers } = harness();
  const first = executor.run(request());
  workers[0].deliver({ type: "result", id: workers[0].sent[0].id, code: 0, stdout: bytes(""), stderr: bytes(""), ordered: [] });
  await first;
  assert.equal(executor.hasIdleWorker, true);
  executor.dispose();
  assert.equal(workers[0].terminated, true);
  assert.equal(executor.hasIdleWorker, false);
});

test("protocol: parseWorkerReply accepts only well-formed replies", () => {
  assert.equal(parseWorkerReply({ type: "ready", id: "init" }).type, "ready");
  assert.throws(() => parseWorkerReply({ type: "ready", id: "other" }), /must carry id init/);
  assert.throws(() => parseWorkerReply({ type: "result", id: "bad id!", code: 0, stdout: bytes(""), stderr: bytes(""), ordered: [] }), /invalid run id/);
  assert.throws(() => parseWorkerReply({ type: "result", id: "a", code: 0.5, stdout: bytes(""), stderr: bytes(""), ordered: [] }), /integer/);
  assert.throws(() => parseWorkerReply({ type: "surprise", id: "a" }), /unknown worker reply/);
});

test("protocol: validateRunRequest rejects traversal, binary-looking text and oversized projects", () => {
  assert.deepEqual(validateRunRequest({ type: "run", id: "r1", command: ["--run", "main.ostrin"], entry: "main.ostrin", files: { "main.ostrin": "x" } }), []);
  assert.ok(validateRunRequest({ type: "run", id: "r1", command: ["--run"], entry: "main.ostrin", files: { "../main.ostrin": "x" } }).length > 0);
  assert.ok(validateRunRequest({ type: "run", id: "r1", command: ["--run"], entry: "main.ostrin", files: { "main.ostrin": "x".repeat(LIMITS.maxFileBytes + 1) } }).length > 0);
  assert.ok(validateRunRequest({ type: "run", id: "bad id", command: ["--run"], entry: "main.ostrin", files: { "main.ostrin": "x" } }).length > 0);
});

test("protocol: illegal transitions throw and terminal runs accept no replies", () => {
  const run = createRun("r1", { timeoutMs: 5000 });
  assert.throws(() => transition(run, "completed"), /illegal run transition queued -> completed/);
  const done = transition(transition(transition(run, "loading"), "running"), "completed");
  assert.equal(acceptsReply(done, { id: "r1" }), false, "a completed run accepts no further replies");
  assert.equal(acceptsReply(transition(run, "loading"), { id: "other" }), false);
});

test("a Worker that cannot be created fails the run and clears its timer and references", async () => {
  const timers = [];
  const executor = createExecutor({
    createWorker: () => {
      throw new Error("Worker is not supported here");
    },
    setTimer: (callback, ms) => {
      const timer = { callback, ms, cleared: false };
      timers.push(timer);
      return timer;
    },
    clearTimer: (timer) => {
      if (timer) timer.cleared = true;
    },
  });
  const outcome = await executor.run(request());
  assert.equal(outcome.state, "failed");
  assert.equal(outcome.error.kind, "worker-start");
  assert.match(outcome.error.message, /not supported/);
  assert.equal(timers[0].cleared, true, "the timeout is cleared");
  assert.equal(executor.activeRun, null, "no active run remains");
  assert.equal(executor.hasIdleWorker, false);
});

test("a Worker that rejects the request is terminated and the run fails", async () => {
  const workers = [];
  const executor = createExecutor({
    createWorker: () => {
      const worker = new FakeWorker();
      worker.postMessage = () => {
        throw new Error("DataCloneError");
      };
      workers.push(worker);
      return worker;
    },
    setTimer: () => ({ cleared: false }),
    clearTimer: () => {},
  });
  const outcome = await executor.run(request());
  assert.equal(outcome.state, "failed");
  assert.equal(workers[0].terminated, true);
  assert.equal(executor.activeRun, null);
});

test("a Worker that ends before it reports started fails the run without a started transition", async () => {
  const { executor, workers, changes } = harness();
  const promise = executor.run(request());
  workers[0].crash("terminated before started");
  const outcome = await promise;
  assert.equal(outcome.state, "failed");
  assert.equal(outcome.error.kind, "worker-terminated");
  assert.deepEqual(changes, ["loading", "failed"]);
});
