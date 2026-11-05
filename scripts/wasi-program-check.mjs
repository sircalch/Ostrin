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
    expected: "derivative checks: 28, failures: 0\nd(q^q) at 2: true\nexpansion and substitution agree: 8 of 8\ncommutative sum: true\ncommutative product: true\ndistributive law: true\nx - x: 0\nx * x: x^2\nH20 = 55835135/15519504\nH20 exact: true\nstatus of a healthy store: []\n2^62 * 4 = 18446744073709551616\n2^200 = 1606938044258990275541962092341162602522202993782792835301376\nH60 = 15117092380124150817026911/3230237388259077233637600\nd(huge * x^3)/dx = 370370367037037036703703703670*x^2\nhuge as float: 123456789012345680000000000000\n1/huge * huge = 1\nstatus after big arithmetic: []\nstatus after division by zero: division by zero\ndiff on a failed store: error: division by zero\nx / x with real x: x^0\nq / q with positive q: 1\nexp(ln(x)) with real x: exp(ln(x))\nexp(ln(q)) with positive q: q\nln(exp(x)): x\n(x^2)^(1/2) with real x: (x^2)^(1/2)\n(q^2)^(1/2) with positive q: q\nd(x^x)/dx: error: differentiating a power with a variable exponent needs a positive base\nd(tan(x))/dx: error: no derivative rule for function tan\ndiff by a number: error: diff needs a symbol to differentiate by\nmissing value: error: no value for symbol x\nln(-1): error: ln of a non-positive number\n",
    leakCheck: true,
    name: "numeric_polyfit",
    source: "examples/numeric_polyfit.ostrin",
    args: ["numeric_polyfit.wasm"],
    expected: "quadratic at 0: true\nquadratic at 2.5: true\nquadratic residual small: true\nline intercept: true\nline slope: true\nline residual positive: true\nlength mismatch: polyfit needs xs and ys with the same length\ntoo few: polyfit needs at least degree + 1 samples\nnegative degree: polyfit needs a non-negative degree\nrepeated x: polyfit could not determine the polynomial: qr found a linearly dependent column\nnon-finite: polyfit needs finite samples\n",
  },
  {
    name: "recursive_enum",
    source: "examples/recursive_enum.ostrin",
    args: ["recursive_enum.wasm"],
    expected: "(2 * x + 5)\n25\n(3 * y + 3 * y)\n24\nz\n20\n28\n3\n2\n",
  },
  {
    name: "exit_status_result",
    source: "examples/exit_status_result.ostrin",
    args: ["exit_status_result.wasm"],
    expectedError: "error: value too large",
  },
  {
    name: "bigint_core",
    source: "examples/bigint_core.ostrin",
    args: ["bigint_core.wasm"],
    expected: "1267650600228229401496703205376\n265252859812191058636308480000000\n-123456789012345678901234567890\n-1001123595505\n617283949493688452\ntrue\n262144\nOk(9223372036854775807)\nErr(bigint does not fit in Int)\n",
    leakCheck: true,
  },
  {
    name: "bigint_validation",
    source: "examples/bigint_validation.ostrin",
    args: ["bigint_validation.wasm"],
    expected: "case 0: sum=33/556927 diff=33/556911 prod=33/455352 quot=31/069614 rem=2/-7 gcd=1/1\ncase 1: sum=29/739700 diff=29/099420 prod=46/061600 quot=14/474565 rem=16/658660 gcd=2/20\ncase 2: sum=8/261062 diff=8/541296 prod=14/997943 quot=4/-152 rem=6/103395 gcd=2/61\ncase 3: sum=12/706965 diff=12/904173 prod=18/325676 quot=8/099111 rem=5/64525 gcd=1/1\ncase 4: sum=60/829090 diff=60/810174 prod=89/079456 quot=32/738611 rem=29/036794 gcd=1/2\ncase 5: sum=42/406659 diff=42/699699 prod=58/287080 quot=26/099959 rem=17/560499 gcd=1/1\ncase 6: sum=40/056217 diff=40/787693 prod=66/522210 quot=14/016169 rem=26/039677 gcd=1/1\ncase 7: sum=27/542546 diff=27/238516 prod=33/569965 quot=21/657742 rem=6/240401 gcd=1/7\ncase 8: sum=36/290908 diff=37/101738 prod=42/210955 quot=1/0 rem=7/094585 gcd=1/1\ncase 9: sum=11/000000 diff=11/852578 prod=22/311521 quot=3/-16 rem=10/605665 gcd=1/1\ncase 10: sum=55/836428 diff=55/503304 prod=87/220692 quot=21/565543 rem=34/696700 gcd=1/2\ncase 11: sum=47/121270 diff=47/812702 prod=71/531976 quot=26/572031 rem=23/197790 gcd=1/2\ncase 12: sum=35/961221 diff=35/568575 prod=46/070054 quot=22/240888 rem=13/910074 gcd=1/1\ncase 13: sum=61/800543 diff=61/193189 prod=66/723718 quot=56/092128 rem=7/151522 gcd=1/1\ncase 14: sum=31/698631 diff=30/698699 prod=33/754610 quot=1/0 rem=2/34 gcd=1/1\ncase 15: sum=46/079375 diff=46/950617 prod=81/927484 quot=10/377250 rem=36/537246 gcd=1/3\ncase 16: sum=36/240950 diff=36/624078 prod=64/111896 quot=11/391539 rem=27/655518 gcd=1/2\ncase 17: sum=57/769711 diff=57/796395 prod=69/506874 quot=43/141042 rem=14/565417 gcd=1/1\ncase 18: sum=49/740373 diff=49/616237 prod=70/465260 quot=30/672031 rem=20/298413 gcd=1/1\ncase 19: sum=20/308247 diff=21/721745 prod=37/091996 quot=1/0 rem=17/793251 gcd=1/1\ncase 20: sum=40/898018 diff=40/763046 prod=50/717448 quot=29/783568 rem=12/700580 gcd=1/2\ncase 21: sum=56/383615 diff=56/881597 prod=94/200546 quot=21/665348 rem=37/968738 gcd=1/1\ncase 22: sum=16/392054 diff=15/000000 prod=27/584729 quot=2/10 rem=14/735757 gcd=1/1\ncase 23: sum=50/730184 diff=50/502920 prod=87/036864 quot=13/456512 rem=36/244968 gcd=1/8\ncase 24: sum=27/065220 diff=28/563202 prod=48/711101 quot=1/0 rem=21/248991 gcd=1/1\ncase 25: sum=58/366890 diff=58/052262 prod=76/238864 quot=41/601201 rem=17/875462 gcd=1/2\ncase 26: sum=39/445946 diff=38/446052 prod=41/637947 quot=1/0 rem=2/53 gcd=1/1\ncase 27: sum=23/179797 diff=24/952533 prod=37/461280 quot=1/0 rem=14/113632 gcd=1/1\ncase 28: sum=53/083986 diff=53/661260 prod=57/215149 quot=47/955715 rem=6/-83078 gcd=1/1\ncase 29: sum=44/721898 diff=44/721846 prod=44/768672 quot=42/873918 rem=2/-4 gcd=1/2\ncase 30: sum=27/721284 diff=28/697868 prod=51/284192 quot=1/0 rem=24/488292 gcd=1/4\ncase 31: sum=8/439860 diff=8/129058 prod=15/786941 quot=2/-1 rem=8/439860 gcd=1/1\ncase 32: sum=53/231926 diff=53/701192 prod=72/862153 quot=35/237302 rem=19/346725 gcd=1/1\ncase 33: sum=41/283616 diff=40/616496 prod=71/320640 quot=1/0 rem=30/666440 gcd=1/8\ncase 34: sum=60/324584 diff=60/657464 prod=83/034560 quot=38/997790 rem=24/823424 gcd=1/8\ncase 35: sum=29/817106 diff=28/083006 prod=55/945200 quot=1/0 rem=26/132950 gcd=1/2\ncase 36: sum=49/266167 diff=49/554325 prod=62/333434 quot=36/745187 rem=14/612473 gcd=1/1\ncase 37: sum=18/083006 diff=18/817106 prod=30/945200 quot=5/10030 rem=14/461556 gcd=1/2\ncase 38: sum=37/768054 diff=37/436868 prod=54/324373 quot=20/243443 rem=17/145762 gcd=1/1\ncase 39: sum=41/831131 diff=41/323457 prod=72/922922 quot=10/810232 rem=30/937478 gcd=1/1\nidentities held: 40 of 40\ndivision by zero: bigint division by zero\nbad digit: bigint found a non-digit character\nempty: bigint needs at least one digit\nnegative power: bigint power needs a non-negative exponent\nmin int: -9223372036854775808\nzero is not negative: 0\n",
    leakCheck: true,
  },
  {
    name: "bigint_operators",
    source: "examples/bigint_operators.ostrin",
    args: ["bigint_operators.wasm"],
    expected: "-864197532086419753208641975320\n1111111110111111111011111111100\n-121932631137021795226185032733622923332237463801111263526900\n-121932631137021795226185032732511812222126352690100152415800\ntrue\ntrue\nfalse\ntrue\ntrue\nfalse\nring identity: 30 of 30\ncommutative sum: 30 of 30\norder laws: 30 of 30\npairs with x < y: 19\n",
    leakCheck: true,
  },
  {
    name: "bigint_rational",
    source: "examples/bigint_rational.ostrin",
    args: ["bigint_rational.wasm"],
    expected: "H30 = 9304682830147/2329089562800\nH60 = 15117092380124150817026911/3230237388259077233637600\nH100 = 14466636279520351160221518043104131447711/2788815009188499086581352357412492142272\ntrue\n1/18\n1/6\ntrue\nfalse\ntrue\n-3/4\n2\n6\n1371742100137174210013717421/109739369\n0.3333333333333333\nzero denominator: rational with a zero denominator\ndivision by zero: rational division by zero\nreciprocal of zero: rational reciprocal of zero\n",
    name: "symbolic_units",
    source: "examples/symbolic_units.ostrin",
    args: ["symbolic_units.wasm"],
    expected: "E = 1/2*m*v^2\ndim E = Length^2*Mass*Time^-2\ndE/dv = m*v\ndim dE/dv = Length*Mass*Time^-1\ndim d(E/L)/dL = Mass*Time^-2\nT = 2*pi*(L*g^(-1))^(1/2)\ndim L/g = Time^2\ndim T = Time\ndim dT/dL = Length^-1*Time\nE / E = 1 with dim 1\nm + v: error: cannot add terms with different dimensions: Mass and Length*Time^-1\nsin(L): error: sin needs a dimensionless argument, got Length\nsin(L / L): 1\nm^v: error: an exponent must be dimensionless, got Length*Time^-1\nL^(1/3): Length^(1/3)\nk: Length^2*Time^-1\nw: Base2^2*Ángulo^-1\nw^2: Base2^4*Ángulo^-2\nleading digit: dimension '2D' has an unexpected character at position 0\nbad dimension: a dimension exponent after ^ must be an integer or n/d\n",
    leakCheck: true,
  },
  {
    name: "native_record_unary_reflected",
    source: "examples/native_record_unary_reflected.ostrin",
    args: ["native_record_unary_reflected.wasm"],
    expected: "-25\n27\n-22\n100\n25\n-21\n286\n12\n",
    leakCheck: true,
  },
  {
    name: "symbolic_operators",
    source: "examples/symbolic_operators.ostrin",
    args: ["symbolic_operators.wasm"],
    expected: "x + y\n5*x\n1 - x\n-x\n0\n(x + y)*(x - y)\nx^2 - y^2\nx^0\n1\ny\nproduct rule: true\nsum rule: true\nd(x^3 + 2x) = 2 + 3*x^2\nd(f g) = -10 + 5*x^4 - 9*x^2\nchain rule: true\nquotient rule evaluates: -5.125\nnumeric check: 0\n",
    leakCheck: true,
  },
  {
    name: "array_operations",
    source: "examples/array_operations.ostrin",
    args: ["array_operations.wasm"],
    expected: "slice 1..4: [8, 15, 16]\nslice empty: error slice range 2..2 is empty or outside 0..6\ntake nothing: error take needs at least one index\nslice outside: error slice range 4..9 is empty or outside 0..6\nslice2 rows 0..2 cols 1..3: [[2, 3], [5, 6]]\nslice2 outside: error slice2 range is empty or outside 3x3\ntake: [42, 4, 4, 16]\ntake outside: error take index 6 is outside 0..6\nconcat: [4, 8, 15, 16, 23, 42, 0.5, 1.5]\nvstack: [[1, 2, 3], [4, 5, 6], [7, 8, 10], [0, 0, 1]]\nvstack mismatch: error vstack needs the same number of columns\nhstack: [[1, 2, 3, 9], [4, 5, 6, 9], [7, 8, 10, 9]]\nhstack mismatch: error hstack needs the same number of rows\nsum axis 0: [12, 15, 19]\nsum axis 1: [6, 15, 25]\nmean axis 0: [4, 5, 6.333333333333333]\nmean axis 1: [2, 5, 8.333333333333334]\nmin axis 0: [1, 2, 3]\nmax axis 1: [3, 6, 10]\nbad axis: error axis must be 0 (down the rows) or 1 (along the columns)\naxis on a vector: error axis reductions need a two-dimensional array\nargmax: 5\nargmin (first of ties): 1\nargmax with NaN: error argmax and argmin need an array without NaN\nmeshgrid x: [[1, 2, 3], [1, 2, 3]]\nmeshgrid y: [[10, 10, 10], [20, 20, 20]]\n",
    leakCheck: true,
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
