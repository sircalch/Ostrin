import { WASI } from "node:wasi";
import {
  closeSync,
  existsSync,
  mkdirSync,
  openSync,
  readFileSync,
  rmSync,
} from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const hostCompiler = process.env.OSTRIN_HOST_COMPILER
  || path.join(root, "compiler", "target", "release", `ostrinc${process.platform === "win32" ? ".exe" : ""}`);
const dist = path.join(root, "dist");
const scratch = path.join(root, "target", "wasi-program-check");

const programs = [
  {
    name: "hello",
    source: "examples/hello.ostrin",
    args: ["hello.wasm"],
    expected: "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\nhola desde Ostrin\n",
  },
  {
    name: "temperature_point_affine",
    source: "compiler/tests/fixtures/temperature_point_affine.ostrin.txt",
    args: ["temperature_point_affine.wasm"],
    expected: "10 degC (absolute)\n10 K\n0 K\n10 degC (absolute)\n0 degC (absolute)\n32 degF (absolute)\n310 K\n",
  },
  {
    name: "temperature_point_overflow",
    source: "compiler/tests/fixtures/temperature_point_overflow.ostrin",
    args: ["temperature_point_overflow.wasm"],
    expectedError: "absolute temperature arithmetic overflowed",
  },
  {
    name: "temperature_point_measurement_field",
    source: "compiler/tests/fixtures/temperature_point_measurement_field.ostrin.txt",
    args: ["temperature_point_measurement_field.wasm"],
    expected: "30 degC (absolute)\n30 degC (absolute)\nreplacement offline\n",
    leakCheck: true,
  },
  {
    name: "temperature_point_measurement_phis",
    source: "compiler/tests/fixtures/temperature_point_measurement_phis.ostrin.txt",
    args: ["temperature_point_measurement_phis.wasm"],
    expected: "20 degC (absolute)\n30 degC (absolute)\n42 degC (absolute)\nsensor 42\n20 degC (absolute)\nsensor 20\n",
    leakCheck: true,
  },
  {
    name: "temperature_point_lists",
    source: "examples/temperature_point_lists.ostrin",
    args: ["temperature_point_lists.wasm"],
    expected: "4\n68 degF (absolute)\n300 K (absolute)\n293.15 K (absolute)\n268.15 K (absolute)\n0 K\n-25 K\n4\nlab\n4\n5\n300 K (absolute)\n",
    leakCheck: true,
  },
  {
    name: "temperature_readings",
    source: "examples/temperature_readings.ostrin",
    args: ["temperature_readings.wasm"],
    expected: "20.3 degC (absolute)\n0.1\n68.54 degF (absolute)\n0.1\n0.8 K\n0\n0.8 K\n0.223606797749979\n",
    leakCheck: true,
  },
  {
    name: "temperature_statistics",
    source: "examples/temperature_statistics.ostrin",
    args: ["temperature_statistics.wasm"],
    expected: "300 K (absolute)\n-5 degC (absolute)\n15.462500000000006 degC (absolute)\n31.850000000000023 K\n288.61249999999995 K (absolute)\ntemperature list is empty\n",
    leakCheck: true,
  },
  {
    name: "numeric_adaptive_brent",
    source: "examples/numeric_adaptive_brent.ostrin",
    args: ["numeric_adaptive_brent.wasm"],
    expected: "sin: within 1e-9 = true, error estimate ok = true, converged = true, evaluations = 473\nsqrt: within 1e-9 = true, error estimate ok = true, converged = true, evaluations = 653\nexp reversed: within 1e-9 = true, error estimate ok = true, converged = true, evaluations = 129\nempty: within 1e-9 = true, error estimate ok = true, converged = true, evaluations = 0\nbad tolerance: adaptive_simpson needs a positive tolerance\ndepth limited: within 1e-9 = false, error estimate ok = false, converged = false, evaluations = 33\nnon-finite: adaptive_simpson integrand returned a non-finite value\nnan comparisons: false true false false\nwallis: within 1e-12 = true\nfixed point of cos: within 1e-12 = true\nsame sign: brent needs f(a) and f(b) with opposite signs\n",
  },
  {
    name: "symbolic_core",
    source: "examples/symbolic_core.ostrin",
    args: ["symbolic_core.wasm"],
    expected: "2*x\n5/2*x\ntrue\nx^2\n0\nx^0\n1\nd(x^3)/dx = 3*x^2\nd(x*sin(x))/dx = sin(x) + x*cos(x)\nd(exp(2x))/dx = 2*exp(2*x)\nd(ln(x^2))/dx = 2*x^(-1)\nd(x^x)/dx = error: differentiating a power with a variable exponent needs a positive base\nd(x^p)/dx = p^p*(1 + ln(p))\nd(y)/dx = 0\n1 + x + 3*x^2\n15\n15\nerror: no value for symbol x\nx^2 + y^2 + 2*x*y\n",
    leakCheck: true,
  },
  {
    name: "symbolic_validation",
    source: "examples/symbolic_validation.ostrin",
    args: ["symbolic_validation.wasm"],
    expected: "derivative checks: 28, failures: 0\nd(q^q) at 2: true\nexpansion and substitution agree: 8 of 8\ncommutative sum: true\ncommutative product: true\ndistributive law: true\nx - x: 0\nx * x: x^2\nH20 = 55835135/15519504\nH20 exact: true\nstatus of a healthy store: []\nstatus after overflow: exact rational arithmetic overflowed 64 bits\ndiff on a failed store: error: exact rational arithmetic overflowed 64 bits\nx / x with real x: x^0\nq / q with positive q: 1\nexp(ln(x)) with real x: exp(ln(x))\nexp(ln(q)) with positive q: q\nln(exp(x)): x\n(x^2)^(1/2) with real x: (x^2)^(1/2)\n(q^2)^(1/2) with positive q: q\nd(x^x)/dx: error: differentiating a power with a variable exponent needs a positive base\nd(tan(x))/dx: error: no derivative rule for function tan\ndiff by a number: error: diff needs a symbol to differentiate by\nmissing value: error: no value for symbol x\nln(-1): error: ln of a non-positive number\n",
    leakCheck: true,
    name: "numeric_polyfit",
    source: "examples/numeric_polyfit.ostrin",
    args: ["numeric_polyfit.wasm"],
    expected: "quadratic at 0: true\nquadratic at 2.5: true\nquadratic residual small: true\nline intercept: true\nline slope: true\nline residual positive: true\nlength mismatch: polyfit needs xs and ys with the same length\ntoo few: polyfit needs at least degree + 1 samples\nnegative degree: polyfit needs a non-negative degree\nrepeated x: polyfit could not determine the polynomial: qr found a linearly dependent column\nnon-finite: polyfit needs finite samples\n",
  },
  {
    name: "temperature_point_conversion_overflow",
    source: "compiler/tests/fixtures/temperature_point_conversion_overflow.ostrin",
    args: ["temperature_point_conversion_overflow.wasm"],
    expectedError: "absolute temperature arithmetic overflowed",
  },
  {
    name: "pkg_project",
    project: "examples/pkg_project/main_app",
    args: ["pkg_project.wasm"],
    expected: "hola, Ostrin\n",
  },
  {
    name: "wasi_io_contract",
    source: "examples/wasi_io_contract.ostrin",
    args: ["wasi_io_contract.wasm", "alpha", "beta"],
    env: { OSTRIN_WASI_TEST: "wasi-value" },
    expected: "2\nalpha\nbeta\nwasi-value\nfalse\ntrue\ntrue\nwasi file\n",
  },
  {
    name: "native_ir_file_io",
    source: "examples/native_ir_file_io.ostrin",
    args: ["native_ir_file_io.wasm"],
    expected: "true\ntrue\nhello from IR\ntrue\n",
  },
  {
    name: "native_ir_managed_consumers",
    source: "examples/native_ir_managed_consumers.ostrin",
    args: ["native_ir_managed_consumers.wasm"],
    expected: "native option\nnative option\noption fallback\nnative option\noption error\nnative result\nnative result\nresult fallback\ntrue\n",
  },
  {
    name: "native_ir_nested_wrappers",
    source: "examples/native_ir_nested_wrappers.ostrin",
    args: ["native_ir_nested_wrappers.wasm"],
    expected: "nested\ninner none\nok\nnested error\nnested\nouter fallback\nok\nok fallback\nnested\ndeep payload\ndeep payload\ninner fallback\ndeep payload\ninner fallback\nouter fallback\n",
  },
  {
    name: "managed_enum_result_error",
    source: "compiler/tests/fixtures/managed_enum_result_error.ostrin",
    args: ["managed_enum_result_error.wasm"],
    expected: "ok payload\nno payload\n",
  },
  {
    name: "managed_enum_phi",
    source: "compiler/tests/fixtures/managed_enum_phi.ostrin",
    args: ["managed_enum_phi.wasm"],
    expected: "first payload\nsecond payload\n",
  },
  {
    name: "managed_enum_loop_phi",
    source: "compiler/tests/fixtures/managed_enum_loop_phi.ostrin",
    args: ["managed_enum_loop_phi.wasm"],
    expected: "first payload\n",
  },
  {
    name: "native_dyn_trait_list",
    source: "compiler/tests/fixtures/native_dyn_trait_list.ostrin",
    args: ["native_dyn_trait_list.wasm"],
    expected: "12\n9\n3\n48\n48\n",
  },
  {
    name: "native_trait_defaults_ir",
    source: "examples/native_trait_defaults.ostrin",
    args: ["native_trait_defaults_ir.wasm"],
    expected: "hello rex/HEY\nhello tom/meow\nhello int box/HEY\n",
  },
  {
    name: "native_dyn_trait_record_field",
    source: "examples/native_dyn_trait_record_field.ostrin",
    args: ["native_dyn_trait_record_field.wasm"],
    expected: "12\n12\n9\n9\n",
  },
  {
    name: "native_dyn_trait_generic_field",
    source: "examples/native_dyn_trait_generic_field.ostrin",
    args: ["native_dyn_trait_generic_field.wasm"],
    expected: "first\nfirst\nsecond\n",
  },
  {
    name: "native_dyn_trait_wrappers",
    source: "examples/native_dyn_trait_wrappers.ostrin",
    args: ["native_dyn_trait_wrappers.wasm"],
    expected: "optional\nempty\nresult\nfailure\nnested\nempty\noptional\nempty\n",
  },
  {
    name: "native_dyn_trait_closure_capture",
    source: "examples/native_dyn_trait_closure_capture.ostrin",
    args: ["native_dyn_trait_closure_capture.wasm"],
    expected: "12\n",
  },
  {
    name: "native_dyn_trait_task_capture",
    source: "compiler/tests/fixtures/native_dyn_trait_task_capture.ostrin",
    args: ["native_dyn_trait_task_capture.wasm"],
    expected: "task payload\ntask payload\n",
  },
  {
    name: "native_dyn_trait_task_cancel",
    source: "compiler/tests/fixtures/native_dyn_trait_task_cancel.ostrin",
    args: ["native_dyn_trait_task_cancel.wasm"],
    expected: "true\nfalse\n",
  },
  {
    name: "managed_enum_closure_capture",
    source: "compiler/tests/fixtures/managed_enum_closure_capture.ostrin",
    args: ["managed_enum_closure_capture.wasm"],
    expected: "closure payload\n",
  },
  {
    name: "managed_enum_task_capture",
    source: "compiler/tests/fixtures/managed_enum_task_capture.ostrin",
    args: ["managed_enum_task_capture.wasm"],
    expected: "task payload\n",
  },
  {
    name: "managed_enum_task_cancel",
    source: "compiler/tests/fixtures/managed_enum_task_cancel.ostrin",
    args: ["managed_enum_task_cancel.wasm"],
    expected: "true\nfalse\n",
  },
  {
    name: "csv_parse",
    source: "examples/csv_parse.ostrin",
    args: ["csv_parse.wasm"],
    expected: "2\n3\na|b|c\n3\n1|2|3\n3\n2\nnombre|nota\n2\nPerez, Ana|9.5\n2\ndijo \"hola\"|7\n1\n3\nx|linea1\nlinea2|z\n2\n3\n||\n1\n\n0\ncabecera: invalid digit found in string\n3\n",
  },
  {
    name: "dataframe",
    source: "examples/dataframe.ostrin",
    args: ["dataframe.wasm"],
    expected: "5\nciudad,temp,lluvia\ntemp: n=5 mean=16.5 std=5.244044240850758 min=10 max=23.5\nlluvia: n=5 mean=2.1 std=1.7146428199482247 min=0 max=4.5\n2\ntemp: n=2 mean=22.5 std=1 min=21.5 max=23.5\n-0.9675629734578927\n",
  },
  {
    name: "quantity_arrays",
    source: "examples/quantity_arrays.ostrin",
    args: ["quantity_arrays.wasm"],
    expected: "[0, 1, 2, 3, 4] s\n[0, 3, 12, 27, 48] m\n48 m\n[0, 10.799999999999999, 43.199999999999996, 97.19999999999999, 172.79999999999998] km/h\n[0, 0.009, 0.144, 0.729, 2.304] kJ\n0.048 km\n18 m\n17.69745744450315 m\n313.2 m^2\n[1000, 1003, 1012, 1027, 1048] m\n[false, false, true, true, true]\n[0, 0.003, 0.012, 0.027, 0.048]\n[-3, -12] m\nm\n[0 m, 3 m, 12 m, 27 m, 48 m]\n[1, 0.5, 0.3333333333333333, 0.25] 1/s\n[0, 100, 200, 300] m\n",
  },
  {
    name: "measurement_covariance",
    source: "examples/measurement_covariance.ostrin",
    args: ["measurement_covariance.wasm"],
    expected: "correlated sum variance = 7\ncorrelated sum uncertainty = 2.6458\ncorrelated difference variance = 3\ncorrelated difference uncertainty = 1.7321\nscaled correlated variance = 28\nwithin-tolerance symmetric variance = 7\nindependent sum uncertainty = 2.2361\nrank-deficient cancellation uncertainty = 0\nsubnormal covariance cancellation variance = 0\nunequal subnormal covariance midpoint variance = 2.75\nwide-scale covariance cancellation variance = 0\nrank-three compensated variance = 1\nrank-three reordered compensated variance = 1\nrank-three 1e16 compensated variance = 1\nrank-three 1e16 reordered compensated variance = 1\npower-of-two gradient normalization variance = 4\nsqrt-three rank-one cancellation variance = 0\nunresolved positive pivot = covariance correlation is numerically unresolved\nnear-perfect covariance residual variance = covariance correlation is numerically unresolved\nsubnormal variance ratio covariance variance = 3\nsubnormal geometric covariance scale = covariance correlation is numerically unresolved\nreview subnormal geometric covariance = covariance correlation is numerically unresolved\nrounded geometric covariance = covariance correlation is numerically unresolved\nrounded root product covariance = covariance correlation is numerically unresolved\ndirect contribution cancellation variance = 16\nadjusted contribution cancellation = propagated covariance projection is numerically unresolved\nequal rounded adjustment cancellation = propagated covariance projection is numerically unresolved\nsubnormal rounded source squares = propagated covariance projection is numerically unresolved\nrounded factor projection = propagated covariance projection is numerically unresolved\nsubnormal contribution normalization = propagated covariance contribution normalization loses subnormal precision\nnonuniform rank-one covariance variance = 0.01\nlarge finite PSD covariance = accepted\nindefinite covariance = covariance matrix must be positive semidefinite\nlarge indefinite covariance = covariance matrix must be positive semidefinite\nnonsymmetric covariance = covariance matrix must be symmetric within tolerance\nduplicate covariance id = covariance source ids must be unique\nnon-finite covariance = covariance entries must be finite\nexcessive covariance tolerance = covariance relative tolerance must not exceed 1e-6\nunderflowed source variance = measurement source variance underflows Float range\nsubnormal source variance = covariance diagonal must match the measurement source sigma\nsource variance mismatch = covariance diagonal must match the measurement source sigma\nmissing source = covariance model is missing a measurement source id\nunknown uncertainty = covariance propagation needs known measurement uncertainty\n",
  },
  {
    name: "measurement_quantity_correlation",
    source: "examples/measurement_lab.ostrin",
    args: ["measurement_quantity_correlation.wasm"],
    expected: "exact zero uncertainty (m) = 0\nunknown reading = sensor offline\nshared readings\n8 m\nstandard uncertainty (m) = 0.2\nproduct dimension = Length^2\nproduct contribution\n8000 cm^2\nproduct standard uncertainty = 0.8\nsame-source ratio = 1 ± 0\ncross-dimension ratio = 5 m/s\nspeed dimension = Length*Time^-1\nspeed standard uncertainty (m/s) = 0.5024937810560445\ndimensionless scalar covariance\nsum variance = 7\nsum uncertainty = 2.6458\ndimensioned source correlation\nproduct dimension = Length*Time\nproduct standard uncertainty = 0.6082762530298219\ninvalid correlation diagonal = correlation matrix diagonal must equal one\nmissing correlation source = correlation model is missing a measurement source id\nnear-perfect contributions\nnear-length = 10000000000000000\nnear-time = -10000000000000000\nrounded perfect correlation = correlation coefficient is numerically unresolved\nunit-conversion cancellation = correlated measurement uncertainty is numerically unresolved after unit conversion\n",
  },
  {
    name: "native_ir_function_values",
    source: "examples/native_ir_function_values.ostrin",
    args: ["native_ir_function_values.wasm"],
    expected: "5\n10\n",
  },
  {
    name: "native_function_values",
    source: "examples/native_function_values.ostrin",
    args: ["native_function_values.wasm"],
    expected: "6\n15\n8\n49\n7\n16\n12\n23\n12\nhola mundo\naccion\n408\n2.5\nh1\n6\n",
  },
  {
    name: "native_ir_try_captured_handler",
    source: "examples/native_ir_try_captured_handler.ostrin",
    args: ["native_ir_try_captured_handler.wasm"],
    expected: "VALUE!\nhandled: FAILURE\n",
  },
  {
    name: "native_generic_iterator",
    source: "examples/native_generic_iterator.ostrin",
    args: ["native_generic_iterator.wasm"],
    expected: "7\n7\n7\n",
  },
  {
    name: "native_generic_iterator_nested",
    source: "examples/native_generic_iterator_nested.ostrin",
    args: ["native_generic_iterator_nested.wasm"],
    expected: "a|b\na|b\n",
  },
];

function fail(message) {
  throw new Error(`wasi-program-check: ${message}`);
}

function runCompiler(args, label) {
  if (!existsSync(hostCompiler)) {
    fail(`host compiler is missing at ${hostCompiler}; build compiler/target/release/ostrinc first`);
  }
  const result = spawnSync(hostCompiler, args, {
    cwd: root,
    env: process.env,
    encoding: "utf8",
    stdio: "inherit",
  });
  if (result.error) fail(`${label} could not start: ${result.error.message}`);
  if (result.status !== 0) fail(`${label} exited with ${result.status}`);
}

async function runWasi(program) {
  const stdoutPath = path.join(scratch, `${program.name}.stdout`);
  const stderrPath = path.join(scratch, `${program.name}.stderr`);
  rmSync(stdoutPath, { force: true });
  rmSync(stderrPath, { force: true });
  let stdoutFd;
  let stderrFd;
  try {
    stdoutFd = openSync(stdoutPath, "w");
    stderrFd = openSync(stderrPath, "w");
    const wasi = new WASI({
      version: "preview1",
      args: program.args,
      env: program.env ?? {},
      preopens: { ".": root },
      stdout: stdoutFd,
      stderr: stderrFd,
      returnOnExit: true,
    });
    const module = await WebAssembly.compile(readFileSync(path.join(dist, `${program.name}.wasm`)));
    const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
    const exitCode = wasi.start(instance);
    closeSync(stdoutFd);
    stdoutFd = undefined;
    closeSync(stderrFd);
    stderrFd = undefined;
    const output = readFileSync(stdoutPath, "utf8").replaceAll("\r\n", "\n");
    const error = readFileSync(stderrPath, "utf8").replaceAll("\r\n", "\n");
    if (program.expectedError) {
      if (exitCode === 0 || !error.includes(program.expectedError)) {
        fail(`${program.name} expected runtime failure containing '${program.expectedError}', got exit ${exitCode}: ${error}`);
      }
      return;
    }
    if (exitCode !== 0) fail(`${program.name} exited with ${exitCode}: ${error}`);
    if (output !== program.expected) {
      fail(`${program.name} output mismatch\nexpected:\n${program.expected}\nactual:\n${output}\nstderr:\n${error}`);
    }
    if (program.leakCheck && !/\blive_allocations=0\b/.test(error)) {
      fail(`${program.name} omitted a successful WASI leak-check: ${error}`);
    }
  } finally {
    if (stdoutFd !== undefined) closeSync(stdoutFd);
    if (stderrFd !== undefined) closeSync(stderrFd);
    rmSync(stdoutPath, { force: true });
    rmSync(stderrPath, { force: true });
  }
}

async function main() {
  mkdirSync(dist, { recursive: true });
  mkdirSync(scratch, { recursive: true });
  rmSync(path.join(root, "target", "ostrin-wasi-contract.txt"), { force: true });
  rmSync(path.join(root, "target", "ostrin-ir-file-io.txt"), { force: true });
  for (const program of programs) {
    const output = path.join(dist, `${program.name}.wasm`);
    rmSync(output, { force: true });
    const args = ["--compile", "--target", "wasm32-wasi", "--out", output];
    if (program.leakCheck) args.push("--leak-check");
    if (program.project) {
      args.push("--project", path.join(root, program.project));
    } else {
      args.push(path.join(root, program.source));
    }
    runCompiler(args, `compile ${program.name}`);
    await runWasi(program);
  }
  rmSync(path.join(root, "target", "ostrin-wasi-contract.txt"), { force: true });
  rmSync(path.join(root, "target", "ostrin-ir-file-io.txt"), { force: true });
  console.log(`wasi-program-check: ok (${programs.length} programs, compiler + package + args/env + file I/O + ownership)`);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
