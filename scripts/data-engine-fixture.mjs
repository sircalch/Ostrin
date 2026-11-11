// Generates the synthetic calibration experiment used by examples/data_engine.ostrin and its exact reference.
//
// Usage: node scripts/data-engine-fixture.mjs --write
//        node scripts/data-engine-fixture.mjs --check      (verifies the committed files are current)
//
// The data are deterministic (a fixed linear congruential generator) and every value has at most four decimals,
// so each input is an exact rational. The reference is computed here with BigInt rational arithmetic and an
// algorithm that shares no code with the Ostrin implementation: exact normal equations for the regression,
// exact sums for the statistics. Only the final square roots are rounded, to double precision.
//
// Files written to examples/data/:
//   calibration.csv             the experiment (CSV, with a blank line, NA cells and quoted text)
//   calibration.reference.json  the reference values the Ostrin program must reproduce

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "examples", "data");
const csvPath = join(root, "calibration.csv");
const referencePath = join(root, "calibration.reference.json");

// ---------------------------------------------------------------- deterministic data

let seed = 20260711;
function nextUnit() {
  seed = (seed * 1103515245 + 12345) % 2147483648;
  return seed / 2147483648;
}

const operators = ["Ana", "Bo, the second", "Cy \"the quiet\"", "Dee"];
const rows = [];
for (let i = 1; i <= 36; i++) {
  const temperature = 10 + Math.round(nextUnit() * 3000) / 100; // 10.00 to 40.00
  const pressureRaw = Math.round((98 + nextUnit() * 4) * 10000) / 10000;
  const noise = (Math.round(nextUnit() * 2000) - 1000) / 100000; // within +/- 0.01
  const response = 0.5 + 0.02 * temperature + 0.001 * temperature * temperature + noise;
  rows.push({
    run: i,
    operator: operators[i % operators.length],
    temperature: temperature.toFixed(2),
    pressure: i % 9 === 0 ? "NA" : pressureRaw.toFixed(4),
    response: response.toFixed(4),
  });
}

function csvField(text) {
  return /[",\n]/.test(text) || text !== text.trim() ? `"${text.replaceAll('"', '""')}"` : text;
}

const header = "run,operator,temperature_c,pressure_kpa,response";
const lines = [header];
rows.forEach((row, index) => {
  lines.push([row.run, csvField(row.operator), row.temperature, row.pressure, row.response].join(","));
  if (index === 11) lines.push(""); // a blank line the reader must skip
});
const csv = lines.join("\n") + "\n";

// ---------------------------------------------------------------- exact reference (BigInt rationals)

const gcd = (a, b) => {
  a = a < 0n ? -a : a;
  b = b < 0n ? -b : b;
  while (b !== 0n) [a, b] = [b, a % b];
  return a;
};
class Q {
  constructor(n, d = 1n) {
    if (d === 0n) throw new Error("zero denominator");
    if (d < 0n) { n = -n; d = -d; }
    const g = gcd(n, d) || 1n;
    this.n = n / g;
    this.d = d / g;
  }
  static of(decimalText) {
    const negative = decimalText.startsWith("-");
    const body = negative ? decimalText.slice(1) : decimalText;
    const [whole, frac = ""] = body.split(".");
    const q = new Q(BigInt(whole + frac), 10n ** BigInt(frac.length));
    return negative ? q.neg() : q;
  }
  add(o) { return new Q(this.n * o.d + o.n * this.d, this.d * o.d); }
  sub(o) { return new Q(this.n * o.d - o.n * this.d, this.d * o.d); }
  mul(o) { return new Q(this.n * o.n, this.d * o.d); }
  div(o) { return new Q(this.n * o.d, this.d * o.n); }
  neg() { return new Q(-this.n, this.d); }
  toNumber() { return Number(this.n) / Number(this.d); }
}
const zero = new Q(0n);

function solveExact(matrix, vector) {
  // Gaussian elimination with exact rationals and partial pivoting by nonzero entry.
  const n = vector.length;
  const a = matrix.map((row, i) => [...row, vector[i]]);
  for (let col = 0; col < n; col++) {
    let pivot = col;
    while (pivot < n && a[pivot][col].n === 0n) pivot++;
    if (pivot === n) throw new Error("singular system");
    [a[col], a[pivot]] = [a[pivot], a[col]];
    for (let r = 0; r < n; r++) {
      if (r === col || a[r][col].n === 0n) continue;
      const factor = a[r][col].div(a[col][col]);
      for (let c = col; c <= n; c++) a[r][c] = a[r][c].sub(factor.mul(a[col][c]));
    }
  }
  return a.map((row, i) => row[n].div(row[i]));
}

function exactFit(xs, ys, degree) {
  const n = degree + 1;
  const normal = Array.from({ length: n }, () => Array.from({ length: n }, () => zero));
  const rhs = Array.from({ length: n }, () => zero);
  for (let i = 0; i < xs.length; i++) {
    const powers = [new Q(1n)];
    for (let k = 1; k <= 2 * degree; k++) powers.push(powers[k - 1].mul(xs[i]));
    for (let r = 0; r < n; r++) {
      rhs[r] = rhs[r].add(powers[r].mul(ys[i]));
      for (let c = 0; c < n; c++) normal[r][c] = normal[r][c].add(powers[r + c]);
    }
  }
  const coefficients = solveExact(normal, rhs); // ascending: coefficients[k] multiplies x^k
  let sse = zero;
  let sumY = zero;
  for (let i = 0; i < xs.length; i++) sumY = sumY.add(ys[i]);
  const meanY = sumY.div(new Q(BigInt(ys.length)));
  let sst = zero;
  for (let i = 0; i < xs.length; i++) {
    let yhat = zero;
    let p = new Q(1n);
    for (let k = 0; k < n; k++) {
      yhat = yhat.add(coefficients[k].mul(p));
      p = p.mul(xs[i]);
    }
    const r = ys[i].sub(yhat);
    sse = sse.add(r.mul(r));
    const dev = ys[i].sub(meanY);
    sst = sst.add(dev.mul(dev));
  }
  return { coefficients, sse, sst, meanY };
}

const valid = rows.filter((r) => r.pressure !== "NA");
// Regression on every row with a response and a temperature (pressure is not used in the fit).
const T = rows.map((r) => Q.of(r.temperature));
const Y = rows.map((r) => Q.of(r.response));
const linear = exactFit(T, Y, 1);
const quadratic = exactFit(T, Y, 2);

// Descriptive statistics (exact sums; sample variance with n - 1; sd rounded at the end).
function exactSample(values) {
  const n = BigInt(values.length);
  let sum = zero;
  for (const v of values) sum = sum.add(v);
  const mean = sum.div(new Q(n));
  let ss = zero;
  for (const v of values) ss = ss.add(v.sub(mean).mul(v.sub(mean)));
  const variance = ss.div(new Q(n - 1n));
  const sorted = [...values].sort((a, b) => {
    const diff = a.n * b.d - b.n * a.d;
    return diff < 0n ? -1 : diff > 0n ? 1 : 0;
  });
  const middle = values.length / 2;
  const median = values.length % 2 === 1 ? sorted[Math.floor(middle)] : sorted[middle - 1].add(sorted[middle]).div(new Q(2n));
  return { n: values.length, mean, variance, median, min: sorted[0], max: sorted[sorted.length - 1] };
}

const responseStats = exactSample(Y);
const temperatureStats = exactSample(T);
// Pearson correlation: exact cross sums, rounded once at the end (through the exact squares of the sums).
const meanT = temperatureStats.mean;
const meanY = responseStats.mean;
let sxy = zero;
let sxx = zero;
let syy = zero;
for (let i = 0; i < T.length; i++) {
  const dx = T[i].sub(meanT);
  const dy = Y[i].sub(meanY);
  sxy = sxy.add(dx.mul(dy));
  sxx = sxx.add(dx.mul(dx));
  syy = syy.add(dy.mul(dy));
}
const correlation = sxy.toNumber() / Math.sqrt(sxx.toNumber() * syy.toNumber());

// Grouped means of the response by operator (rows with an operator, in order of first appearance).
const groups = new Map();
for (const row of rows) {
  if (!groups.has(row.operator)) groups.set(row.operator, []);
  groups.get(row.operator).push(Y[rows.indexOf(row)]);
}
const groupMeans = [...groups.entries()].map(([name, values]) => ({
  operator: name,
  n: values.length,
  mean: exactSample(values).mean.toNumber(),
}));

const reference = {
  schema: "ostrin.data-reference/v1",
  method: "exact rational arithmetic (BigInt); normal equations solved exactly; square roots rounded once",
  source: "examples/data/calibration.csv",
  rows: rows.length,
  rows_with_missing_pressure: rows.length - valid.length,
  linear: {
    intercept: linear.coefficients[0].toNumber(),
    slope: linear.coefficients[1].toNumber(),
    sse: linear.sse.toNumber(),
    rmse: Math.sqrt(linear.sse.toNumber() / rows.length),
    r_squared: 1 - linear.sse.toNumber() / linear.sst.toNumber(),
  },
  quadratic: {
    intercept: quadratic.coefficients[0].toNumber(),
    linear: quadratic.coefficients[1].toNumber(),
    quadratic: quadratic.coefficients[2].toNumber(),
    sse: quadratic.sse.toNumber(),
    rmse: Math.sqrt(quadratic.sse.toNumber() / rows.length),
    r_squared: 1 - quadratic.sse.toNumber() / quadratic.sst.toNumber(),
  },
  response: {
    n: responseStats.n,
    mean: responseStats.mean.toNumber(),
    sd: Math.sqrt(responseStats.variance.toNumber()),
    min: responseStats.min.toNumber(),
    max: responseStats.max.toNumber(),
    median: responseStats.median.toNumber(),
  },
  temperature: {
    n: temperatureStats.n,
    mean: temperatureStats.mean.toNumber(),
    sd: Math.sqrt(temperatureStats.variance.toNumber()),
  },
  correlation_temperature_response: correlation,
  group_means: groupMeans,
};

// ---------------------------------------------------------------- write or check

const json = JSON.stringify(reference, null, 2) + "\n";
if (process.argv.includes("--write")) {
  writeFileSync(csvPath, csv);
  writeFileSync(referencePath, json);
  console.log(`wrote ${csvPath} and ${referencePath}`);
} else if (process.argv.includes("--check")) {
  const committedCsv = readFileSync(csvPath, "utf8");
  const committedJson = readFileSync(referencePath, "utf8");
  if (committedCsv !== csv || committedJson !== json) {
    console.error("data-engine fixture is stale: run node scripts/data-engine-fixture.mjs --write");
    process.exit(1);
  }
  console.log("data-engine fixture: ok");
} else {
  console.log(json);
}
