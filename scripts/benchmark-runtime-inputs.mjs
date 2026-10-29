import { isAbsolute, relative, resolve, sep } from "node:path";

const runtimeInputFiles = new Map([
  ["examples/viz_pendulum_calibration.ostrin", [
    { path: "experiments/pendulum-calibration.inputs.json", stagedAs: "data.json" },
  ]],
]);

export function repositoryRelativeWorkloadPath(repositoryRoot, sourcePath) {
  const root = resolve(repositoryRoot);
  const source = resolve(root, sourcePath.replaceAll("\\", "/"));
  const relativeSource = relative(root, source);
  if (!relativeSource || relativeSource === ".."
    || relativeSource.startsWith(`..${sep}`) || isAbsolute(relativeSource)) {
    return null;
  }
  return relativeSource.split(sep).join("/");
}

export function runtimeInputFilesFor(repositoryRoot, sourcePath) {
  const normalizedPath = repositoryRelativeWorkloadPath(repositoryRoot, sourcePath);
  return normalizedPath ? runtimeInputFiles.get(normalizedPath) || [] : [];
}

export function assertBenchmarkWorkloadSucceeded(stdout, stderr, label) {
  const diagnostic = `${stdout}\n${stderr}`.match(
    /(?:^|\r?\n)\s*(?:input|fit|measurement|uncertainty) error:[^\r\n]*/i,
  );
  if (diagnostic) {
    throw new Error(`${label} reported an analysis error instead of a benchmark result: ${diagnostic[0].trim()}`);
  }
}
