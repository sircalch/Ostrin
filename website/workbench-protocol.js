// Message protocol and run lifecycle for the Workbench execution Worker.
//
// Pure data code: no DOM, no Worker, no WebAssembly. The controller and the Worker both use these
// functions to validate every message, and the controller uses the state machine to reject transitions
// that cannot happen (for example a result for a run that was cancelled or superseded).
//
// Requests (page -> Worker):
//   { type: "run", id, command: string[], entry: string, files: { [path]: text } }
//   { type: "ping", id }                      used once to measure Worker start-up latency
// Replies (Worker -> page), every reply carries the id of the run it belongs to:
//   { type: "ready", id: "init" }             the compiler module is compiled and the Worker is idle
//   { type: "started", id }                  the run has begun instantiating the program
//   { type: "result", id, code, stdout: Uint8Array, stderr: Uint8Array }
//   { type: "runtime-error", id, message }    the runtime trapped or the module could not start
// Lifecycle states of one run: queued -> loading -> running -> completed | failed | cancelled | timed-out.
// `cancelled` and `timed-out` are set by the page when it terminates the Worker; a late reply from a
// terminated Worker is never accepted.

export const PROTOCOL_VERSION = 1;
export const RUN_STATES = Object.freeze(["queued", "loading", "running", "completed", "failed", "cancelled", "timed-out"]);
export const TERMINAL_STATES = Object.freeze(["completed", "failed", "cancelled", "timed-out"]);

const TRANSITIONS = Object.freeze({
  queued: ["loading", "cancelled"],
  loading: ["running", "failed", "cancelled", "timed-out"],
  running: ["completed", "failed", "cancelled", "timed-out"],
  completed: [],
  failed: [],
  cancelled: [],
  "timed-out": [],
});

export const LIMITS = Object.freeze({
  maxRunIdLength: 64,
  maxCommandArgs: 8,
  maxFiles: 32,
  maxFileBytes: 256 * 1024,
  maxTotalBytes: 2 * 1024 * 1024,
  defaultTimeoutMs: 20_000,
  minTimeoutMs: 1_000,
  maxTimeoutMs: 120_000,
});

const ID_PATTERN = /^[A-Za-z0-9_-]+$/;
const PATH_PATTERN = /^[A-Za-z0-9_][A-Za-z0-9_-]*(\/[A-Za-z0-9_][A-Za-z0-9_-]*)*\.ostrin$/;

export function isRunId(value) {
  return typeof value === "string" && value.length > 0 && value.length <= LIMITS.maxRunIdLength && ID_PATTERN.test(value);
}

// Returns a list of problems; an empty list means the request is valid and can be sent.
export function validateRunRequest(request) {
  const problems = [];
  if (!request || typeof request !== "object") return ["request is not an object"];
  if (request.type !== "run") problems.push('type must be "run"');
  if (!isRunId(request.id)) problems.push("id must be 1 to 64 letters, digits, '_' or '-'");
  if (!Array.isArray(request.command) || request.command.length === 0 || request.command.length > LIMITS.maxCommandArgs) {
    problems.push(`command must be a list of 1 to ${LIMITS.maxCommandArgs} arguments`);
  } else if (request.command.some((arg) => typeof arg !== "string" || arg.length > 200 || arg.includes("\u0000"))) {
    problems.push("every command argument must be a short string without NUL");
  }
  if (!request.files || typeof request.files !== "object" || Array.isArray(request.files)) {
    problems.push("files must be an object");
    return problems;
  }
  const paths = Object.keys(request.files);
  if (paths.length === 0 || paths.length > LIMITS.maxFiles) problems.push(`files must have 1 to ${LIMITS.maxFiles} entries`);
  if (typeof request.entry !== "string" || !paths.includes(request.entry)) problems.push("entry must be one of the files");
  let total = 0;
  for (const path of paths) {
    if (!PATH_PATTERN.test(path) || path.includes("..")) {
      problems.push(`invalid file path ${JSON.stringify(path)}`);
      continue;
    }
    const text = request.files[path];
    if (typeof text !== "string") {
      problems.push(`${path} is not text`);
      continue;
    }
    const bytes = new TextEncoder().encode(text).length;
    total += bytes;
    if (bytes > LIMITS.maxFileBytes) problems.push(`${path} is larger than ${LIMITS.maxFileBytes} bytes`);
  }
  if (total > LIMITS.maxTotalBytes) problems.push(`files total ${total} bytes; the limit is ${LIMITS.maxTotalBytes}`);
  return problems;
}

// Validates a reply from the Worker. Returns the reply, or throws with the reason.
export function parseWorkerReply(message) {
  if (!message || typeof message !== "object") throw new Error("worker reply is not an object");
  if (message.type === "ready") {
    if (message.id !== "init") throw new Error("ready reply must carry id init");
    return message;
  }
  if (!isRunId(message.id)) throw new Error("worker reply has an invalid run id");
  if (message.type === "started") return message;
  if (message.type === "result") {
    if (!Number.isInteger(message.code)) throw new Error("result exit code must be an integer");
    if (!(message.stdout instanceof Uint8Array) || !(message.stderr instanceof Uint8Array)) {
      throw new Error("result stdout and stderr must be byte arrays");
    }
    return message;
  }
  if (message.type === "runtime-error") {
    if (typeof message.message !== "string") throw new Error("runtime-error must carry a message");
    return message;
  }
  throw new Error(`unknown worker reply type ${JSON.stringify(message.type)}`);
}

export function canTransition(from, to) {
  return (TRANSITIONS[from] ?? []).includes(to);
}

// One run as a small state machine. Every transition is checked; an impossible one throws.
export function createRun(id, { timeoutMs = LIMITS.defaultTimeoutMs } = {}) {
  if (!isRunId(id)) throw new Error("invalid run id");
  if (!Number.isInteger(timeoutMs) || timeoutMs < LIMITS.minTimeoutMs || timeoutMs > LIMITS.maxTimeoutMs) {
    throw new Error(`timeout must be between ${LIMITS.minTimeoutMs} and ${LIMITS.maxTimeoutMs} ms`);
  }
  return { id, timeoutMs, state: "queued", result: null, error: null };
}

export function transition(run, to, detail = {}) {
  if (!canTransition(run.state, to)) throw new Error(`illegal run transition ${run.state} -> ${to}`);
  return { ...run, state: to, ...detail };
}

// Whether a reply may update the run it names. A reply is accepted only while its run is still live,
// so a result from a cancelled, timed-out or replaced run can never become the current result.
export function acceptsReply(run, reply) {
  if (!run || run.id !== reply.id) return false;
  return !TERMINAL_STATES.includes(run.state);
}
