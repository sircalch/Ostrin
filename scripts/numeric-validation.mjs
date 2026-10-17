#!/usr/bin/env node

// Validate scientific matrix algorithms using independent residual calculations.
// This tooling is intentionally outside the Ostrin runtime and has no production dependencies.
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const contractPath = path.join(repositoryRoot, "validation/numeric/matrix_algorithms.json");
const outputDirectory = path.join(repositoryRoot, "target/numeric-validation");
const parityAbsolute = 1e-13;
const parityRelative = 1e-12;

function run(command, args, label) {
  const result = spawnSync(command, args, { cwd: repositoryRoot, encoding: "utf8", timeout: 120_000 });
  if (result.error) throw new Error(`${label}: ${result.error.message}`);
  if (result.status !== 0) throw new Error(`${label} failed (${result.status}):\n${result.stdout}\n${result.stderr}`);
  return { stdout: result.stdout, stderr: result.stderr };
}

export function parseEmittedNumbers(output) {
  const values = [];
  for (const line of output.replaceAll("\r\n", "\n").split("\n")) {
    if (!line) continue;
    const value = Number(line.trim());
    if (!Number.isFinite(value)) throw new Error(`numeric output contains a non-finite value: ${line}`);
    values.push(value);
  }
  return values;
}

function takeVector(values, prefix, length) {
  if (values.offset + length > values.numbers.length) throw new Error(`numeric validation output ended before ${prefix}`);
  const result = values.numbers.slice(values.offset, values.offset + length);
  values.offset += length;
  return result;
}

function takeMatrix(values, prefix, rows, columns) {
  const flat = takeVector(values, prefix, rows * columns);
  return Array.from({ length: rows }, (_, row) => flat.slice(row * columns, (row + 1) * columns));
}

function transpose(a) {
  return Array.from({ length: a[0].length }, (_, column) => a.map((row) => row[column]));
}

function multiply(a, b) {
  return a.map((row) => b[0].map((_, column) =>
    row.reduce((sum, value, k) => sum + value * b[k][column], 0)));
}

function subtract(a, b) {
  return a.map((row, i) => row.map((value, j) => value - b[i][j]));
}

function identity(n) {
  return Array.from({ length: n }, (_, i) => Array.from({ length: n }, (_, j) => Number(i === j)));
}

function diagonal(values) {
  return values.map((value, i) => values.map((_, j) => Number(i === j) * value));
}

function norm2(values) {
  return Math.hypot(...values);
}

function normF(a) {
  return Math.hypot(...a.flat());
}

export function compareReports(interpreter, native) {
  let compared = 0;
  let maximumDifference = 0;
  function visit(left, right, path) {
    if (typeof left === "number" || typeof right === "number") {
      if (typeof left !== "number" || typeof right !== "number") {
        throw new Error(`interpreter/native report shape differs at ${path}`);
      }
      const difference = Math.abs(left - right);
      const scale = Math.max(Math.abs(left), Math.abs(right), 1);
      checkTolerance(`interpreter/native parity at ${path}`, difference, scale, parityAbsolute, parityRelative);
      compared += 1;
      maximumDifference = Math.max(maximumDifference, difference);
      return;
    }
    if (Array.isArray(left) || Array.isArray(right)) {
      if (!Array.isArray(left) || !Array.isArray(right) || left.length !== right.length) {
        throw new Error(`interpreter/native report shape differs at ${path}`);
      }
      left.forEach((value, index) => visit(value, right[index], `${path}[${index}]`));
      return;
    }
    if (left && typeof left === "object" || right && typeof right === "object") {
      if (!left || !right || Object.keys(left).sort().join() !== Object.keys(right).sort().join()) {
        throw new Error(`interpreter/native report shape differs at ${path}`);
      }
      for (const key of Object.keys(left)) visit(left[key], right[key], `${path}.${key}`);
      return;
    }
    if (left !== right) throw new Error(`interpreter/native report differs at ${path}`);
  }
  visit(interpreter, native, "report");
  return {
    comparedValues: compared,
    maximumAbsoluteDifference: maximumDifference,
    absolute: parityAbsolute,
    relative: parityRelative,
  };
}

export function compareNumericOutputs(interpreter, native) {
  if (!Array.isArray(interpreter) || !Array.isArray(native) || interpreter.length !== native.length) {
    throw new Error(`interpreter/native output length differs (${interpreter?.length ?? "invalid"} vs ${native?.length ?? "invalid"})`);
  }
  let maximumDifference = 0;
  for (let index = 0; index < interpreter.length; index++) {
    const difference = Math.abs(interpreter[index] - native[index]);
    const scale = Math.max(Math.abs(interpreter[index]), Math.abs(native[index]), 1);
    checkTolerance(`interpreter/native emitted value ${index}`, difference, scale, parityAbsolute, parityRelative);
    maximumDifference = Math.max(maximumDifference, difference);
  }
  return {
    comparedValues: interpreter.length,
    maximumAbsoluteDifference: maximumDifference,
    absolute: parityAbsolute,
    relative: parityRelative,
  };
}

export function checkTolerance(label, observed, scale, absolute, relative) {
  const limit = absolute + relative * scale;
  if (!Number.isFinite(observed) || observed > limit) {
    throw new Error(`${label} residual ${observed} exceeds abs ${absolute} + rel ${relative} × scale ${scale} = ${limit}`);
  }
  return { value: observed, limit, scale };
}

function validateLinearSolve(values, id, contract) {
  const n = contract.solution.length;
  const x = takeVector(values, `${id}_solution`, n);
  const residual = subtract([contract.matrix.map((row, i) => row.reduce((sum, value, j) => sum + value * x[j], 0))], [contract.rhs]);
  const absoluteResidual = norm2(residual[0]);
  const scale = normF(contract.matrix) * norm2(x) + norm2(contract.rhs);
  const backwardError = absoluteResidual / scale;
  const forwardErrors = x.map((value, i) => Math.abs(value - contract.solution[i]));
  const maxForwardError = Math.max(...forwardErrors);
  for (let i = 0; i < n; i++) {
    checkTolerance(`${id} solution[${i}]`, forwardErrors[i], Math.abs(contract.solution[i]), contract.absolute, contract.relative);
  }
  return {
    solution: x,
    absoluteResidual: checkTolerance(`${id} absolute residual`, absoluteResidual, scale, contract.absolute, contract.relative),
    relativeBackwardError: checkTolerance(`${id} relative backward error`, backwardError, 1, 0, contract.relative),
    maximumForwardError: maxForwardError,
  };
}

function validateLu(values, contract, solutionReport) {
  const n = contract.solution.length;
  const lower = takeMatrix(values, "lu_l", n, n);
  const upper = takeMatrix(values, "lu_u", n, n);
  const permutation = takeVector(values, "lu_permutation", n);
  if (permutation.some((value) => !Number.isInteger(value))
    || new Set(permutation).size !== n
    || permutation.some((value) => value < 0 || value >= n)) {
    throw new Error("LU permutation is not a permutation of source rows");
  }
  const product = multiply(lower, upper);
  const permuted = permutation.map((sourceRow) => contract.matrix[sourceRow]);
  const residual = normF(subtract(product, permuted));
  let structureResidual = 0;
  for (let i = 0; i < n; i++) {
    structureResidual = Math.max(structureResidual, Math.abs(lower[i][i] - 1));
    for (let j = 0; j < n; j++) {
      if (j > i) structureResidual = Math.max(structureResidual, Math.abs(lower[i][j]));
      if (i > j) structureResidual = Math.max(structureResidual, Math.abs(upper[i][j]));
    }
  }
  return {
    ...solutionReport,
    relativeFactorizationResidual: checkTolerance("lu relative factorization", residual / normF(contract.matrix), 1, contract.absolute, contract.relative),
    triangularStructureResidual: checkTolerance("lu triangular structure", structureResidual, 1, contract.absolute, contract.relative),
  };
}

function validateCholesky(values, contract, solutionReport) {
  const n = contract.solution.length;
  const lower = takeMatrix(values, "cholesky_l", n, n);
  const reconstructed = multiply(lower, transpose(lower));
  const residual = normF(subtract(reconstructed, contract.matrix));
  let structureResidual = 0;
  for (let i = 0; i < n; i++) {
    if (!(lower[i][i] > 0)) throw new Error(`cholesky diagonal ${i} must be positive`);
    for (let j = i + 1; j < n; j++) structureResidual = Math.max(structureResidual, Math.abs(lower[i][j]));
  }
  return {
    ...solutionReport,
    relativeFactorizationResidual: checkTolerance("cholesky relative factorization", residual / normF(contract.matrix), 1, contract.absolute, contract.relative),
    triangularStructureResidual: checkTolerance("cholesky triangular structure", structureResidual, 1, contract.absolute, contract.relative),
  };
}

function validateQr(values, id, contract) {
  const [m, n] = [contract.matrix.length, contract.matrix[0].length];
  const q = takeMatrix(values, `${id}_q`, m, n);
  const r = takeMatrix(values, `${id}_r`, n, n);
  const aNorm = normF(contract.matrix);
  const reconstruction = normF(subtract(multiply(q, r), contract.matrix));
  const orthogonality = normF(subtract(multiply(transpose(q), q), identity(n)));
  let structureResidual = 0;
  for (let i = 0; i < n; i++) for (let j = 0; j < i; j++) structureResidual = Math.max(structureResidual, Math.abs(r[i][j]));
  return {
    relativeReconstructionResidual: checkTolerance(`${id} relative reconstruction`, reconstruction / aNorm, 1, contract.absolute, contract.relative),
    orthogonalityResidual: checkTolerance(`${id} orthogonality`, orthogonality, 1, contract.absolute, contract.relative),
    triangularStructureResidual: checkTolerance(`${id} triangular structure`, structureResidual, 1, contract.absolute, contract.relative),
  };
}

function validateSvd(values, id, contract) {
  const [m, n] = [contract.matrix.length, contract.matrix[0].length];
  const u = takeMatrix(values, `${id}_u`, m, n);
  const s = takeVector(values, `${id}_s`, n);
  const vt = takeMatrix(values, `${id}_vt`, n, n);
  const aNorm = normF(contract.matrix);
  const reconstructed = multiply(multiply(u, diagonal(s)), vt);
  const reconstruction = normF(subtract(reconstructed, contract.matrix));
  const uOrthogonality = normF(subtract(multiply(transpose(u), u), identity(n)));
  const vOrthogonality = normF(subtract(multiply(vt, transpose(vt)), identity(n)));
  for (let i = 0; i < n; i++) {
    if (s[i] < 0 || (i > 0 && s[i] > s[i - 1])) throw new Error(`${id} singular values must be non-negative and descending`);
    checkTolerance(`${id} singular value[${i}]`, Math.abs(s[i] - contract.singular_values[i]), contract.singular_values[i], contract.absolute, contract.relative);
  }
  return {
    relativeReconstructionResidual: checkTolerance(`${id} relative reconstruction`, reconstruction / aNorm, 1, contract.absolute, contract.relative),
    uOrthogonalityResidual: checkTolerance(`${id} U orthogonality`, uOrthogonality, 1, contract.absolute, contract.relative),
    vOrthogonalityResidual: checkTolerance(`${id} V orthogonality`, vOrthogonality, 1, contract.absolute, contract.relative),
    singularValues: s,
  };
}

function validateEigen(values, id, contract) {
  const n = contract.matrix.length;
  const eigenvalues = takeVector(values, `${id}_values`, n);
  const eigenvectors = takeMatrix(values, `${id}_vectors`, n, n);
  const aNorm = normF(contract.matrix);
  const vNorm = normF(eigenvectors);
  const av = multiply(contract.matrix, eigenvectors);
  const vlambda = multiply(eigenvectors, diagonal(eigenvalues));
  const residual = normF(subtract(av, vlambda));
  const orthogonality = normF(subtract(multiply(transpose(eigenvectors), eigenvectors), identity(n)));
  for (let i = 0; i < n; i++) {
    if (i > 0 && eigenvalues[i] < eigenvalues[i - 1]) throw new Error(`${id} eigenvalues must be ascending`);
    checkTolerance(`${id} eigenvalue[${i}]`, Math.abs(eigenvalues[i] - contract.eigenvalues[i]), Math.abs(contract.eigenvalues[i]), contract.absolute, contract.relative);
  }
  return {
    relativeEigenResidual: checkTolerance(`${id} relative eigen residual`, residual / (aNorm * vNorm), 1, contract.absolute, contract.relative),
    orthogonalityResidual: checkTolerance(`${id} orthogonality`, orthogonality, 1, contract.absolute, contract.relative),
    eigenvalues,
  };
}

export function directDft(signal) {
  const re = [];
  const im = [];
  for (let k = 0; k < signal.length; k++) {
    let real = 0;
    let imaginary = 0;
    for (let j = 0; j < signal.length; j++) {
      const angle = -2 * Math.PI * ((k * j) % signal.length) / signal.length;
      real += signal[j] * Math.cos(angle);
      imaginary += signal[j] * Math.sin(angle);
    }
    re.push(real);
    im.push(imaginary);
  }
  return { re, im };
}

export function validateFft(values, contract) {
  const reports = {};
  for (const [id, testCase] of Object.entries(contract.cases)) {
    const n = testCase.signal.length;
    const re = takeVector(values, `${id} real spectrum`, n);
    const im = takeVector(values, `${id} imaginary spectrum`, n);
    const reconstructed = takeVector(values, `${id} inverse transform`, n);
    const reference = directDft(testCase.signal);
    let spectralResidual = 0;
    for (let k = 0; k < n; k++) {
      const realDifference = Math.abs(re[k] - reference.re[k]);
      const imaginaryDifference = Math.abs(im[k] - reference.im[k]);
      checkTolerance(`${id} real DFT bin ${k}`, realDifference, Math.abs(reference.re[k]), testCase.absolute, testCase.relative);
      checkTolerance(`${id} imaginary DFT bin ${k}`, imaginaryDifference, Math.abs(reference.im[k]), testCase.absolute, testCase.relative);
      spectralResidual = Math.max(spectralResidual, realDifference, imaginaryDifference);
    }
    let inputEnergy = 0;
    let spectralEnergy = 0;
    let maximumReconstructionError = 0;
    for (let i = 0; i < n; i++) {
      inputEnergy += testCase.signal[i] ** 2;
      spectralEnergy += re[i] ** 2 + im[i] ** 2;
      const difference = Math.abs(reconstructed[i] - testCase.signal[i]);
      checkTolerance(`${id} inverse reconstruction sample ${i}`, difference, Math.abs(testCase.signal[i]), testCase.absolute, testCase.relative);
      maximumReconstructionError = Math.max(maximumReconstructionError, difference);
    }
    const parsevalResidual = Math.abs(spectralEnergy / n - inputEnergy);
    checkTolerance(`${id} Parseval energy`, parsevalResidual, inputEnergy, testCase.absolute, testCase.relative);
    reports[id] = { spectralResidual, maximumReconstructionError, parsevalResidual, inputEnergy, spectralEnergy };
  }
  return reports;
}

export function validateRun(output, contract) {
  const parsed = parseEmittedNumbers(output);
  const values = { numbers: parsed, offset: 0 };
  const reports = {};
  reports.lu = validateLu(values, contract.cases.lu,
    validateLinearSolve(values, "lu", contract.cases.lu));
  reports.cholesky = validateCholesky(values, contract.cases.cholesky,
    validateLinearSolve(values, "cholesky", contract.cases.cholesky));
  reports.qr = validateQr(values, "qr", contract.cases.qr);
  reports.svd = validateSvd(values, "svd", contract.cases.svd);
  reports.eigen = validateEigen(values, "eigen", contract.cases.eigen);
  reports.fft = validateFft(values, contract.cases.fft);
  if (values.offset !== values.numbers.length) {
    throw new Error(`numeric validation emitted ${values.numbers.length - values.offset} unexpected trailing values`);
  }
  return reports;
}

export function validateNumerics({ root = repositoryRoot, compiler, contract } = {}) {
  contract ??= JSON.parse(readFileSync(path.join(root, "validation/numeric/matrix_algorithms.json"), "utf8"));
  compiler ??= process.env.OSTRIN_COMPILER || ["release", "debug"]
    .map((profile) => path.join(root, "compiler/target", profile, process.platform === "win32" ? "ostrinc.exe" : "ostrinc"))
    .find(existsSync);
  if (!compiler || !existsSync(compiler)) throw new Error("Ostrin compiler not found; build compiler/Cargo.toml first");
  if (contract.schema !== "ostrin.numeric-validation/v1") throw new Error("unsupported numerical validation contract schema");
  const source = path.resolve(root, contract.fixture);
  const relative = path.relative(root, source);
  if (!relative || relative.startsWith("..") || path.isAbsolute(relative) || !existsSync(source)) {
    throw new Error(`numerical fixture is missing or outside the repository: ${contract.fixture}`);
  }

  mkdirSync(outputDirectory, { recursive: true });
  const executable = path.join(outputDirectory, `numeric-validation${process.platform === "win32" ? ".exe" : ""}`);
  rmSync(executable, { force: true });
  const interpreter = run(compiler, ["--run", source], "interpreter numerical fixture");
  const compile = run(compiler, ["--compile", "--leak-check", "--out", executable, source], "native numerical fixture compile");
  const native = run(executable, [], "native numerical fixture");
  rmSync(executable, { force: true });
  if (!native.stderr.includes("live_allocations=0")) throw new Error(`native numerical validation leaked allocations:\n${native.stderr}`);

  const interpreterValues = parseEmittedNumbers(interpreter.stdout);
  const nativeValues = parseEmittedNumbers(native.stdout);
  const interpreterReports = validateRun(interpreter.stdout, contract);
  const nativeReports = validateRun(native.stdout, contract);
  const ci = process.env.GITHUB_ACTIONS === "true" && process.env.GITHUB_RUN_ID ? {
    provider: "github-actions",
    run_id: process.env.GITHUB_RUN_ID,
    run_url: process.env.GITHUB_SERVER_URL && process.env.GITHUB_REPOSITORY
      ? `${process.env.GITHUB_SERVER_URL}/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}`
      : undefined,
    commit: process.env.GITHUB_SHA ?? "unknown",
    workflow: process.env.GITHUB_WORKFLOW ?? "unknown",
  } : undefined;
  return {
    schema: contract.schema,
    reference: contract.reference,
    fixture: contract.fixture,
    environment: {
      platform: process.platform,
      architecture: process.arch,
      node: process.version,
    },
    leakCheck: "live_allocations=0",
    nativeCompileDiagnostics: compile.stderr.trim(),
    parity: compareReports(interpreterReports, nativeReports),
    emittedOutputParity: compareNumericOutputs(interpreterValues, nativeValues),
    runs: { interpreter: interpreterReports, native: nativeReports },
    ...(ci ? { ci } : {}),
  };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const report = validateNumerics();
    console.log(JSON.stringify(report, null, 2));
  } catch (error) {
    console.error(`numeric-validation: ${error.message}`);
    process.exitCode = 1;
  }
}
