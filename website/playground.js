// Playground and live examples on top of the real compiler (see ostrin-runtime.js).
import { loadCompiler, runOstrinc } from "./ostrin-runtime.js";
import { serializeProject, createProject, validPath } from "./workbench-project.js";
import {
  addFile,
  commitEdit,
  createWorkbench,
  deleteFile,
  dirtyFiles,
  importProjectText,
  isDirty,
  manifestForLastRun,
  markSaved,
  projectFromSourceFile,
  recordRun,
  renameActive,
  runInput,
  selectFile,
  setEntry,
} from "./workbench-controller.js";
import { renderExplorer } from "./workbench-explorer.js";
import { createHistory, recordRun as recordHistoryEntry } from "./workbench-history.js";
import { captureRun } from "./workbench-history-capture.js";
import { renderHistoryPanel } from "./workbench-history-panel.js";
import { createExecutor } from "./workbench-execution.js";
import { orderedLines } from "./workbench-protocol.js";
import { parseResults, resultsTable, resultsExportText, RESULTS_PREFIX } from "./workbench-results.js";
import { createPresentationGuard } from "./workbench-presentation.js";

const EXAMPLES = {
  "Hello, quantities": `fn velocity(distance: Quantity<Length>, time: Quantity<Time>) -> Quantity<Length / Time> {
    distance / time
}

fn main() -> Void {
    speed = velocity(10 m, 2 s)
    print(speed)
}
`,
  "Standard library": `import std.math
import std.lists
import std.time

fn main() -> Void {
    numbers = [5, 3, 9, 1]
    print(lists.sorted(numbers))
    print(math.max(2.5, 1.5))
    print(math.gcd(12, 18))
    print(17 % 5)
    print(time.iso(time.date(2024, 2, 29)))
}
`,
  "Records and match": `record Point {
    x: Float
    y: Float
}

enum Shape {
    Circle(Float)
    Rect(Float, Float)
}

fn area(shape: Shape) -> Float {
    match shape {
        Circle(r) => 3.14159 * r * r,
        Rect(w, h) => w * h,
    }
}

fn main() -> Void {
    print(Point { x: 1.0, y: 2.0 })
    print(area(Circle(2.0)))
    print(area(Rect(3.0, 4.0)))
}
`,
  "Tests": `fn double(n: Int) -> Int {
    n * 2
}

fn test_double() -> Void {
    assert_eq(double(21), 42)
}

fn test_negative() -> Void {
    assert_eq(double(-3), -6)
}
`,
  "Concurrency": `fn main() -> Void {
    results = channel<Int>()
    producer = spawn {
        results.send(1)
        results.send(2)
        results.close()
    }
    for value in results {
        print(value)
    }
}
`,
  "Symbolic derivatives": `// std.symbolic: exact numbers, canonical expressions, assumptions, differentiation, substitution
// and numeric evaluation on a hash-consed store.
import std.symbolic

fn main() -> Void {
    s = symbolic.store()
    x = symbolic.symbol(s, "x")
    y = symbolic.symbol(s, "y")
    two = symbolic.number(s, 2)
    half = symbolic.number(s, 1, 2)

    // canonical form: commutativity, like terms and exact rationals
    print(symbolic.show(symbolic.add(x, x)))
    print(symbolic.show(symbolic.add(symbolic.mul(two, x), symbolic.mul(half, x))))
    print(symbolic.same(symbolic.add(x, y), symbolic.add(y, x)))
    print(symbolic.show(symbolic.mul(x, x)))
    print(symbolic.show(symbolic.sub(x, x)))

    // assumptions gate unsound simplifications
    print(symbolic.show(symbolic.div(x, x)))
    p = symbolic.symbol(s, "p", "positive")
    print(symbolic.show(symbolic.div(p, p)))

    // derivatives
    cube = symbolic.power(x, symbolic.number(s, 3))
    print("d(x^3)/dx = " + symbolic.diff_text(cube, x))
    print("d(x*sin(x))/dx = " + symbolic.diff_text(symbolic.mul(x, symbolic.call("sin", x)), x))
    print("d(exp(2x))/dx = " + symbolic.diff_text(symbolic.call("exp", symbolic.mul(two, x)), x))
    print("d(ln(x^2))/dx = " + symbolic.diff_text(symbolic.call("ln", symbolic.power(x, two)), x))
    print("d(x^x)/dx = " + symbolic.diff_text(symbolic.power(x, x), x))
    print("d(x^p)/dx = " + symbolic.diff_text(symbolic.power(p, p), p))
    print("d(y)/dx = " + symbolic.diff_text(y, x))

    // substitution and numeric evaluation
    poly = symbolic.add(symbolic.mul(symbolic.number(s, 3), symbolic.power(x, two)), symbolic.add(x, symbolic.number(s, 1)))
    print(symbolic.show(poly))
    print(symbolic.show(symbolic.subs(poly, x, symbolic.number(s, 2))))
    print(symbolic.eval_text(poly, ["x"], [2.0]))
    print(symbolic.eval_text(poly, ["z"], [2.0]))
    print(symbolic.show(symbolic.expand(symbolic.power(symbolic.add(x, y), two))))
}
`,
  "Symbolic model to figure": `// Symbolic modelling to a figure: a damped oscillator x(t) = A exp(-g t) cos(w t) is built as an
// expression, differentiated twice for velocity and acceleration, checked against the ODE
// x'' + 2 g x' + (g^2 + w^2) x = 0 at sampled times, and plotted from numeric evaluation of the
// symbolic results.
import std.symbolic
import std.viz

fn main() -> Void {
    s = symbolic.store()
    t = symbolic.symbol(s, "t")
    amp = symbolic.symbol(s, "A")
    g = symbolic.symbol(s, "g", "positive")
    w = symbolic.symbol(s, "w", "positive")
    decay = symbolic.call("exp", symbolic.neg(symbolic.mul(g, t)))
    x = symbolic.mul(amp, symbolic.mul(decay, symbolic.call("cos", symbolic.mul(w, t))))
    print("x(t) = " + symbolic.show(x))
    v = symbolic.diff(x, t).unwrap()
    a = symbolic.diff(v, t).unwrap()
    print("v(t) = " + symbolic.show(v))
    print("a(t) = " + symbolic.show(a))

    names = ["t", "A", "g", "w"]
    n = 240
    times = linspace(0.0, 12.0, n)
    mut position = zeros([n])
    mut velocity = zeros([n])
    mut worst = 0.0
    for i in 0 until n {
        values = [times[i], 1.0, 0.2, 2.0]
        xi = symbolic.eval_or(x, names, values, 0.0 / 0.0)
        vi = symbolic.eval_or(v, names, values, 0.0 / 0.0)
        ai = symbolic.eval_or(a, names, values, 0.0 / 0.0)
        position.set(i, xi)
        velocity.set(i, vi)
        residual = abs(ai + 2.0 * 0.2 * vi + (0.2 * 0.2 + 2.0 * 2.0) * xi)
        if residual > worst {
            worst = residual
        }
    }
    print("ODE residual below 1e-12: " + (worst < 1e-12).to_string())
    print("samples: " + n.to_string())
    fig = viz.figure("Damped oscillator from a symbolic model")
        .describe("x(t) = A exp(-g t) cos(w t) with A = 1, g = 0.2, w = 2, and its symbolic derivative")
        .labels("time t", "x and dx/dt")
        .line(times, position, label: "x(t)")
        .line(times, velocity, label: "dx/dt", dash: "6 4")
        .hline(0.0)
    svg = fig.svg()
    print("figure is svg: " + svg.starts_with("<svg").to_string())
    print(svg)
}
`,
  "Polished quadratic figure": `// std.viz · figure: y = x^2 - 2x - 3 = (x + 1)(x - 3) on a fixed grid of 41 points over [-2, 4].
// The roots x = -1 and x = 3 are exact and hard-coded (they match std.solve.solve_quadratic(1, -2, -3)),
// marked with dashed vertical lines and labels on the axis. Prints one standalone SVG.
import std.viz

fn main() -> Void {
    // 41 samples: x_i = -2 + 0.15 * i for i in 0..=40.
    xs = linspace(-2.0, 4.0, 41)
    ys = xs * xs - xs * 2.0 - 3.0
    mut fig = viz.figure("y = x^2 - 2x - 3 with its roots")
        .describe("y = x^2 - 2x - 3 over [-2, 4], 41 sampled points, roots at x = -1 and x = 3")
        .labels("x", "y")
        .xlim(-2.0, 4.0)
        .polished(caption: "y = x^2 - 2x - 3, roots at x = -1 and x = 3", label_size: 13, animate: true)
        .line(xs, ys, label: "x^2 - 2x - 3")
        .scatter(xs, ys, label: "samples", color: "#1f77b4", size: 3.0)
        .hline(0.0)
        .vline(-1.0, label: "root x = -1", color: "#d62728")
        .vline(3.0, label: "root x = 3", color: "#d62728")
    print(fig.svg())
}`,
  "Exact rationals (BigInt)": `// Typed exact rationals over BigInt: harmonic numbers beyond 64 bits, field identities, ordering,
// parsing and the typed errors for a zero denominator. The harmonic numbers H30, H60 and H100 and
// the other values were reproduced with an external BigInt.
import std.bigint

fn harmonic(n: Int) -> String {
    mut total = bigint.fraction_whole(0)
    for k in 1 to n {
        total = total + bigint.fraction_from_ints(1, k).unwrap()
    }
    bigint.fraction_to_string(total)
}

fn main() -> Void {
    print("H30 = " + harmonic(30))
    print("H60 = " + harmonic(60))
    print("H100 = " + harmonic(100))

    third = bigint.fraction_from_ints(1, 3).unwrap()
    sixth = bigint.fraction_from_ints(1, 6).unwrap()
    half = bigint.fraction_from_ints(1, 2).unwrap()
    print(third + sixth == half)
    print(bigint.fraction_to_string(third * sixth))
    print(bigint.fraction_to_string(half - third))
    print(third < half)
    print(half <= third)
    print(sixth != third)
    print(bigint.fraction_to_string(bigint.fraction_from_ints(6, 0 - 8).unwrap()))
    print(bigint.fraction_to_string(bigint.fraction_div(third, sixth).unwrap()))
    print(bigint.fraction_to_string(bigint.fraction_reciprocal(sixth).unwrap()))
    print(bigint.fraction_to_string(bigint.fraction_parse("123456789012345678901234567890/9876543210").unwrap()))
    print(bigint.fraction_to_float(third))

    match bigint.fraction_from_ints(1, 0) {
        Ok(r) => print("zero denominator accepted"),
        Err(message) => print("zero denominator: " + message),
    }
    match bigint.fraction_div(third, bigint.fraction_whole(0)) {
        Ok(r) => print("division by zero accepted"),
        Err(message) => print("division by zero: " + message),
    }
    match bigint.fraction_reciprocal(bigint.fraction_whole(0)) {
        Ok(r) => print("reciprocal of zero accepted"),
        Err(message) => print("reciprocal of zero: " + message),
    }
}
`,
  "Array operations": `// std.arrays: slicing, joining, axis reductions, argmax/argmin and meshgrid on Float arrays, with typed
// errors for ranges, shapes and empty or NaN input. Every value was reproduced with an independent
// implementation (see the integration test).
import std.arrays

fn show(label: String, result: Result<Array<Float>, String>) -> Void {
    match result {
        Ok(values) => print(label + ": " + values.to_string()),
        Err(message) => print(label + ": error " + message),
    }
}

fn main() -> Void {
    v = array([4.0, 8.0, 15.0, 16.0, 23.0, 42.0])
    m = array([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 10.0]])
    show("slice 1..4", arrays.slice(v, 1, 4))
    show("slice empty", arrays.slice(v, 2, 2))
    show("take nothing", arrays.take(v, []))
    show("slice outside", arrays.slice(v, 4, 9))
    show("slice2 rows 0..2 cols 1..3", arrays.slice2(m, 0, 2, 1, 3))
    show("slice2 outside", arrays.slice2(m, 0, 4, 0, 1))
    show("take", arrays.take(v, [5, 0, 0, 3]))
    show("take outside", arrays.take(v, [1, 6]))
    show("concat", arrays.concat(v, array([0.5, 1.5])))
    show("vstack", arrays.vstack(m, array([[0.0, 0.0, 1.0]])))
    show("vstack mismatch", arrays.vstack(m, array([[1.0, 2.0]])))
    show("hstack", arrays.hstack(m, array([[9.0], [9.0], [9.0]])))
    show("hstack mismatch", arrays.hstack(m, array([[1.0, 2.0]])))
    show("sum axis 0", arrays.sum_axis(m, 0))
    show("sum axis 1", arrays.sum_axis(m, 1))
    show("mean axis 0", arrays.mean_axis(m, 0))
    show("mean axis 1", arrays.mean_axis(m, 1))
    show("min axis 0", arrays.min_axis(m, 0))
    show("max axis 1", arrays.max_axis(m, 1))
    show("bad axis", arrays.sum_axis(m, 2))
    show("axis on a vector", arrays.sum_axis(v, 0))
    match arrays.argmax(v) {
        Ok(i) => print("argmax: " + i.to_string()),
        Err(message) => print("argmax: error " + message),
    }
    match arrays.argmin(array([3.0, 1.0, 2.0, 1.0])) {
        Ok(i) => print("argmin (first of ties): " + i.to_string()),
        Err(message) => print("argmin: error " + message),
    }
    match arrays.argmax(array([1.0, sqrt(0.0 - 1.0)])) {
        Ok(i) => print("argmax with NaN: " + i.to_string()),
        Err(message) => print("argmax with NaN: error " + message),
    }
    match arrays.meshgrid(array([1.0, 2.0, 3.0]), array([10.0, 20.0])) {
        Ok(grid) => {
            print("meshgrid x: " + grid.x.to_string())
            print("meshgrid y: " + grid.y.to_string())
        },
        Err(message) => print("meshgrid: error " + message),
    }
}
`,
  "Polynomials (exact)": `// std.polynomial over exact rationals: 40 generated integer polynomials, each put through every public
// operation, plus fixed edge cases. The inputs come from a 31-bit linear congruential generator so
// scripts/polynomial-oracle.mjs can regenerate the same cases in BigInt arithmetic and print the same
// rows; the two outputs are compared line by line.
import std.polynomial

fn next_state(s: Int) -> Int {
    (s * 1103515245 + 12345) % 2147483648
}

/// A value in lo..=hi from a generator state.
fn draw(s: Int, lo: Int, hi: Int) -> Int {
    lo + s % (hi - lo + 1)
}

fn int_mul(a: List<Int>, b: List<Int>) -> List<Int> {
    mut out: List<Int> = []
    for k in 0 until a.length() + b.length() - 1 {
        mut acc = 0
        for i in 0 until a.length() {
            j = k - i
            if j >= 0 and j < b.length() {
                acc = acc + a[i] * b[j]
            }
        }
        out.push(acc)
    }
    out
}

fn row(idx: Int, label: String, text: String) -> Void {
    print("case " + idx.to_string() + " " + label + " " + text)
}

fn join(items: List<String>) -> String {
    mut text = "["
    for i in 0 until items.length() {
        if i > 0 {
            text = text + ", "
        }
        text = text + items[i]
    }
    text + "]"
}

fn main() -> Void {
    mut state = 20260417
    for idx in 0 until 40 {
        // shared linear factor (fq x - fr)
        state = next_state(state)
        fq = draw(state, 1, 3)
        state = next_state(state)
        fr = draw(state, 0 - 4, 4)
        shared = [0 - fr, fq]

        // p: shared times a constant and one or two more linear factors, or shared times a quadratic
        state = next_state(state)
        mode = draw(state, 0, 2)
        mut p_list = shared
        if mode == 2 {
            state = next_state(state)
            qa = draw(state, 0 - 3, 3)
            state = next_state(state)
            qb = draw(state, 0 - 3, 3)
            p_list = int_mul(shared, [qb, qa, 1])
        } else {
            state = next_state(state)
            mut c = draw(state, 0 - 3, 3)
            if c == 0 {
                c = 2
            }
            p_list = int_mul(shared, [c])
            state = next_state(state)
            l1q = draw(state, 1, 3)
            state = next_state(state)
            l1r = draw(state, 0 - 4, 4)
            p_list = int_mul(p_list, [0 - l1r, l1q])
            if mode == 1 {
                state = next_state(state)
                l2q = draw(state, 1, 3)
                state = next_state(state)
                l2r = draw(state, 0 - 4, 4)
                p_list = int_mul(p_list, [0 - l2r, l2q])
            }
        }

        // q: a fresh linear factor, shared times a fresh linear factor, or a general quadratic
        state = next_state(state)
        qmode = draw(state, 0, 2)
        mut q_list = shared
        if qmode == 2 {
            state = next_state(state)
            a0 = draw(state, 0 - 4, 4)
            state = next_state(state)
            a1 = draw(state, 0 - 4, 4)
            state = next_state(state)
            mut a2 = draw(state, 0 - 4, 4)
            if a2 == 0 {
                a2 = 1
            }
            q_list = [a0, a1, a2]
        } else {
            state = next_state(state)
            l3q = draw(state, 1, 3)
            state = next_state(state)
            l3r = draw(state, 0 - 4, 4)
            q_list = [0 - l3r, l3q]
            if qmode == 1 {
                q_list = int_mul(shared, q_list)
            }
        }

        // evaluation point en/ed
        state = next_state(state)
        en = draw(state, 0 - 3, 3)
        state = next_state(state)
        ed = draw(state, 1, 3)
        xtext = en.to_string() + "/" + ed.to_string()

        p = polynomial.poly_from_ints(p_list)
        q = polynomial.poly_from_ints(q_list)
        row(idx, "p", polynomial.poly_to_string(p))
        row(idx, "q", polynomial.poly_to_string(q))
        row(idx, "add", polynomial.poly_to_string(polynomial.poly_add(p, q).unwrap()))
        row(idx, "sub", polynomial.poly_to_string(polynomial.poly_sub(p, q).unwrap()))
        row(idx, "mul", polynomial.poly_to_string(polynomial.poly_mul(p, q).unwrap()))
        d = polynomial.poly_divmod(p, q).unwrap()
        row(idx, "quotient", polynomial.poly_to_string(d.quotient))
        row(idx, "remainder", polynomial.poly_to_string(d.remainder))
        row(idx, "gcd", polynomial.poly_to_string(polynomial.poly_gcd(p, q).unwrap()))
        row(idx, "eval " + xtext, polynomial.poly_eval(p, xtext).unwrap())
        row(idx, "derivative", polynomial.poly_to_string(polynomial.poly_derivative(p).unwrap()))
        roots = polynomial.poly_roots_rational(p).unwrap()
        row(idx, "roots", join(roots))
        f = polynomial.poly_factor_linear(p).unwrap()
        mut factor_text = ""
        mut prod = f.cofactor
        for i in 0 until f.factors.length() {
            factor_text = factor_text + polynomial.poly_to_string(f.factors[i]) + " "
            prod = polynomial.poly_mul(prod, f.factors[i]).unwrap()
        }
        row(idx, "factors", factor_text + "cofactor " + polynomial.poly_to_string(f.cofactor))
        if polynomial.poly_to_string(prod) == polynomial.poly_to_string(p) {
            row(idx, "check", "ok")
        } else {
            row(idx, "check", "FAIL")
        }
        match polynomial.poly_divmod(p, polynomial.poly_from_ints([])) {
            Ok(x) => row(idx, "divzero", "accepted"),
            Err(message) => row(idx, "divzero", "error " + message),
        }
    }

    // Fixed edge cases.
    half_third = polynomial.poly_from_texts(["2/3", "1/3"]).unwrap()
    row(0 - 1, "rational-roots", join(polynomial.poly_roots_rational(half_third).unwrap()))
    match polynomial.poly_from_texts(["1/0"]) {
        Ok(x) => row(0 - 1, "bad-text", "accepted"),
        Err(message) => row(0 - 1, "bad-text", "error " + message),
    }
    huge = polynomial.poly_from_ints([10000000000000, 1])
    match polynomial.poly_roots_rational(huge) {
        Ok(x) => row(0 - 1, "huge-constant", join(x)),
        Err(message) => row(0 - 1, "huge-constant", "error " + message),
    }
    match polynomial.poly_roots_rational(polynomial.poly_from_ints([])) {
        Ok(x) => row(0 - 1, "zero-roots", join(x)),
        Err(message) => row(0 - 1, "zero-roots", "error " + message),
    }
    row(0 - 1, "constant-derivative", polynomial.poly_to_string(polynomial.poly_derivative(polynomial.poly_from_ints([5])).unwrap()))
    gcd_a = polynomial.poly_from_ints([1, 2])
    gcd_b = polynomial.poly_from_texts(["1/2", "1"]).unwrap()
    row(0 - 1, "gcd-rational", polynomial.poly_to_string(polynomial.poly_gcd(gcd_a, gcd_b).unwrap()))
}
`,
  "Exact equations": `// std.solve over the 109 cases of scripts/solve-oracle.mjs: 69 hand-picked cases, then 40 generated
// with the Park-Miller generator (seed 7). Each line is "id | op | inputs | result", the same format
// the oracle prints, and the interpreter and native binary must reproduce tests/fixtures/solve_core_oracle.txt
// exactly. The module's Err texts "all reals", "no solution" and "no real roots" are results in the
// oracle, so they are printed as results; other Err texts print with an "error: " prefix.
import std.solve

fn next(s: Int) -> Int {
    16807 * s % 2147483647
}

/// A coefficient in -30..30 from a generator state.
fn coef(s: Int) -> Int {
    s % 61 - 30
}

fn join(items: List<String>) -> String {
    mut text = ""
    for i in 0 until items.length() {
        if i > 0 {
            text = text + ", "
        }
        text = text + items[i]
    }
    text
}

fn join_ints(values: List<Int>) -> String {
    mut text = ""
    for i in 0 until values.length() {
        if i > 0 {
            text = text + ","
        }
        text = text + values[i].to_string()
    }
    text
}

fn id_of(n: Int) -> String {
    if n < 10 {
        "C00" + n.to_string()
    } else if n < 100 {
        "C0" + n.to_string()
    } else {
        "C" + n.to_string()
    }
}

fn emit(n: Int, op: String, inputs: String, result: String) -> Void {
    print(id_of(n) + " | " + op + " | " + inputs + " | " + result)
}

fn linear_text(r: Result<String, String>) -> String {
    match r {
        Ok(t) => t,
        Err(m) => "error: " + m,
    }
}

fn quadratic_text(r: Result<List<String>, String>) -> String {
    match r {
        Ok(items) => join(items),
        Err(m) => {
            if m == "all reals" or m == "no solution" or m == "no real roots" {
                m
            } else {
                "error: " + m
            }
        },
    }
}

fn rational_text(r: Result<List<String>, String>) -> String {
    match r {
        Ok(items) => {
            if items.length() == 0 {
                "none"
            } else {
                join(items)
            }
        },
        Err(m) => "error: " + m,
    }
}

fn lin(n: Int, a: Int, b: Int) -> Void {
    emit(n, "linear", "a=" + a.to_string() + ",b=" + b.to_string(), linear_text(solve.solve_linear(a, b)))
}

fn quad(n: Int, a: Int, b: Int, c: Int) -> Void {
    inputs = "a=" + a.to_string() + ",b=" + b.to_string() + ",c=" + c.to_string()
    emit(n, "quadratic", inputs, quadratic_text(solve.solve_quadratic(a, b, c)))
}

fn rat(n: Int, c: List<Int>) -> Void {
    emit(n, "rational_roots", "c=[" + join_ints(c) + "]", rational_text(solve.solve_rational_roots(c)))
}

fn main() -> Void {
    print("# solve-oracle: exact equation solving (independent BigInt oracle for std.solve)")
    print("# line format: id | op | inputs | result")
    print("# ops: linear, quadratic, rational_roots. See scripts/solve-oracle.mjs header for result rules.")
    print("# cases: 109 (69 hand-picked, then 40 Park-Miller seed 7)")

    lin(1, 0, 0)
    lin(2, 0, 5)
    lin(3, 0, 0 - 1)
    lin(4, 3, 0 - 6)
    lin(5, 0 - 4, 6)
    lin(6, 6, 0 - 4)
    lin(7, 0 - 7, 21)
    lin(8, 1, 0)
    lin(9, 10000, 0 - 9999)
    lin(10, 0 - 10000, 3)
    lin(11, 7, 10000)
    lin(12, 0 - 3, 0)

    quad(13, 0, 0, 0)
    quad(14, 0, 0, 4)
    quad(15, 0, 2, 0 - 6)
    quad(16, 1, 0 - 5, 6)
    quad(17, 1, 2, 1)
    quad(18, 2, 0, 0 - 8)
    quad(19, 1, 0, 1)
    quad(20, 3, 2, 5)
    quad(21, 1, 0, 0 - 2)
    quad(22, 1, 0, 0 - 3)
    quad(23, 1, 0 - 2, 0 - 2)
    quad(24, 2, 0 - 4, 0 - 2)
    quad(25, 1, 7, 1)
    quad(26, 0 - 1, 7, 0 - 1)
    quad(27, 1, 2, 0 - 17)
    quad(28, 1, 0, 0 - 5)
    quad(29, 0 - 2, 0 - 4, 6)
    quad(30, 0 - 3, 1, 2)
    quad(31, 10000, 10000, 0 - 10000)
    quad(32, 5000, 0 - 10000, 5000)
    quad(33, 7, 0 - 70, 175)
    quad(34, 1, 0 - 1, 0 - 1)
    quad(35, 4, 0, 0 - 45)
    quad(36, 9999, 0 - 10000, 1)
    quad(37, 1, 10000, 1)
    quad(38, 0 - 10000, 10000, 10000)
    quad(39, 1, 0 - 7, 12)
    quad(40, 2, 3, 0 - 2)
    quad(41, 3, 0, 0)

    rat(42, [])
    rat(43, [0, 0, 0])
    rat(44, [5])
    rat(45, [0, 1])
    rat(46, [0 - 2, 0, 1])
    rat(47, [0 - 6, 1, 1])
    rat(48, [0 - 1, 0, 1])
    rat(49, [0, 0, 1])
    rat(50, [0, 0 - 1, 0, 1])
    rat(51, [6, 0 - 5, 1])
    rat(52, [4, 0 - 4, 1])
    rat(53, [0 - 1, 3, 0 - 3, 1])
    rat(54, [0 - 1, 2])
    rat(55, [3, 0 - 2])
    rat(56, [0 - 2, 1, 6])
    rat(57, [0, 0, 0 - 2, 1])
    rat(58, [0 - 10000, 0, 0, 0, 1])
    rat(59, [0 - 10000, 9999, 1])
    rat(60, [0 - 6, 11, 0 - 6, 1])
    rat(61, [0 - 1, 0, 4])
    rat(62, [0 - 9, 0, 4])
    rat(63, [0 - 8, 0, 0, 1])
    rat(64, [0 - 10000, 0, 1])
    rat(65, [0, 0, 0, 7])
    rat(66, [7])
    rat(67, [1, 0, 0, 0, 0, 0, 1])
    rat(68, [0 - 1, 0, 0, 0, 0, 0, 1])
    rat(69, [12, 0 - 7, 1])

    // Generated cases 70..109: op = i mod 3, draws in the order the oracle uses.
    mut st = 7
    for i in 0 until 40 {
        op = i % 3
        if op == 0 {
            st = next(st)
            a = coef(st)
            st = next(st)
            b = coef(st)
            lin(70 + i, a, b)
        } else if op == 1 {
            st = next(st)
            a = coef(st)
            st = next(st)
            b = coef(st)
            st = next(st)
            c = coef(st)
            quad(70 + i, a, b, c)
        } else {
            st = next(st)
            c0 = coef(st)
            st = next(st)
            c1 = coef(st)
            st = next(st)
            c2 = coef(st)
            st = next(st)
            c3 = coef(st)
            rat(70 + i, [c0, c1, c2, c3])
        }
    }
}
`,
  "Reverse-mode gradients": `// Reverse-mode automatic differentiation (std.autodiff) over the four fixed test expressions.
//
// Each gradient comes from one backward pass over a tape. It is checked against a central difference
// (step h = 1e-6) of the plain Float evaluation of the same expression, with the tolerance
// |ad - fd| <= 1e-6 * max(1, |fd|). The forward value on the tape is also checked against the plain
// evaluation. Every number is printed with 9 decimals. scripts/autodiff-oracle.mjs prints the same
// lines (closed-form gradients, central differences), and tests/fixtures/autodiff_reverse_oracle.txt
// holds that output. The interpreter and native binary must reproduce it exactly.
import std.autodiff

/// A Float with exactly 9 decimals, rounded half up on |v|; the sign is shown only when the rounded
/// value is nonzero. Finite input only.
fn fixed9(v: Float) -> String {
    neg = v < 0.0
    scaled = floor(abs(v) * 1000000000.0 + 0.5)
    n = scaled as Int
    whole = n / 1000000000
    mut digits = (n % 1000000000).to_string()
    while digits.length() < 9 {
        digits = "0" + digits
    }
    sign = if neg and n != 0 { "-" } else { "" }
    sign + whole.to_string() + "." + digits
}

/// |a - b| <= 1e-6 * max(1, |b|).
fn close_enough(a: Float, b: Float) -> Bool {
    scale = if abs(b) > 1.0 { abs(b) } else { 1.0 }
    abs(a - b) <= 0.000001 * scale
}

fn plain_value(kind: Int, x: Float, y: Float) -> Float {
    match autodiff.expr_value(kind, x, y) {
        Ok(v) => v,
        Err(_) => 0.0,
    }
}

fn tape_value_of(kind: Int, x: Float, y: Float) -> Float {
    match autodiff.reverse_value_2(kind, x, y) {
        Ok(v) => v,
        Err(_) => 0.0,
    }
}

fn check(kind: Int, x: Float, y: Float) -> Void {
    head = "f=" + autodiff.expr_name(kind) + " x=" + fixed9(x) + " y=" + fixed9(y)
    match autodiff.reverse_grad_2(kind, x, y) {
        Err(m) => print(head + " error: " + m),
        Ok(g) => {
            h = 1e-6
            fdx = (plain_value(kind, x + h, y) - plain_value(kind, x - h, y)) / (2.0 * h)
            fdy = (plain_value(kind, x, y + h) - plain_value(kind, x, y - h)) / (2.0 * h)
            value_ok = close_enough(tape_value_of(kind, x, y), plain_value(kind, x, y))
            ok = close_enough(g[0], fdx) and close_enough(g[1], fdy) and value_ok
            status = if ok { "ok" } else { "MISMATCH" }
            print(head + " grad=[" + fixed9(g[0]) + ", " + fixed9(g[1]) + "] " + status)
        },
    }
}

fn check_error(label: String, r: Result<List<Float>, String>) -> Void {
    match r {
        Err(m) => print("case=" + label + " error: " + m),
        Ok(_) => print("case=" + label + " unexpected success"),
    }
}

fn main() -> Void {
    print("# autodiff_reverse: reverse-mode gradients (std.autodiff) checked against central differences")
    print("# step h = 1e-6; tolerance |ad - fd| <= 1e-6 * max(1, |fd|); numbers have 9 decimals")
    print("# expressions: 0 x*y + sin(x); 1 exp(x)*ln(y) - 2*(y/x); 2 (x-y)^3 + cos(y)*x; 3 -(x*x + 1)/(y*y + 2)")
    check(0, 0.7, -1.3)
    check(0, 2.0, 0.5)
    check(0, -0.4, 1.1)
    check(1, 0.9, 1.5)
    check(1, 1.3, 0.25)
    check(1, -0.6, 2.0)
    check(2, 0.4, -0.9)
    check(2, 1.5, 1.2)
    check(2, -2.0, 0.3)
    check(3, 0.8, -0.6)
    check(3, -1.5, 2.5)
    check(3, 0.0, 1.0)
    print("# non-finite and invalid inputs return Err")
    check_error("nan_input", autodiff.reverse_grad_2(0, sqrt(-1.0), 1.0))
    check_error("inf_input", autodiff.reverse_grad_2(0, 1.0, 1.0 / 0.0))
    check_error("ln_of_zero", autodiff.reverse_grad_2(1, 0.5, 0.0))
    check_error("unknown_kind", autodiff.reverse_grad_2(9, 1.0, 1.0))
}
`,
};

const $ = (id) => document.getElementById(id);
const source = $("source");
const sourceStatus = $("source-status");
const output = $("output");
const status = $("status");
const buttons = ["run", "check", "test", "format"].map($).filter(Boolean);
const shareButton = $("share");
const saveButton = $("save");
const openButton = $("open");
const resetButton = $("reset");
const openInput = $("open-file");
const DRAFT_KEY = "ostrin.playground.draft";
const figureStore = new WeakMap();

// Workbench project state. `source` is the editor for the active file; the model is updated from it
// (commitEdit) before anything reads the project, so the textarea never holds the only copy.
const PROJECT_DRAFT_KEY = "ostrin.workbench.draft";
const explorer = $("file-explorer");
const exportProjectButton = $("export-project");
const importProjectButton = $("import-project");
const manifestButton = $("download-manifest");
const importProjectInput = $("import-project-file");
let workbench = createWorkbench(projectFromSourceFile("main.ostrin", EXAMPLES[Object.keys(EXAMPLES)[0]]));
let compilerIdentityPromise = null;
const cancelRunButton = $("cancel-run");
// Scientific run history of this page. Entries are kept in memory; each one is an immutable record of one run.
let history = createHistory();
let historySelectedId = null;
const historyPanel = $("run-history");
const projectIdOf = (project) => project.name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "project";
// Comparison selection: at most two runs. Choosing a third replaces the older of the two.
let historySelection = [];
let historyComparison = null;
function renderHistory() {
  renderHistoryPanel(historyPanel, history, { selectedId: historySelectedId, selection: historySelection, comparison: historyComparison }, {
    onSelect: (id) => {
      historySelectedId = id;
      historyComparison = null;
      renderHistory();
    },
    onClose: () => {
      historySelectedId = null;
      renderHistory();
    },
    onToggle: (id) => {
      historySelection = historySelection.includes(id)
        ? historySelection.filter((x) => x !== id)
        : [...historySelection, id].slice(-2);
      renderHistory();
    },
    onCompare: () => {
      if (historySelection.length !== 2) return;
      historyComparison = { a: historySelection[0], b: historySelection[1] };
      renderHistory();
    },
    onCloseCompare: () => {
      historyComparison = null;
      renderHistory();
    },
  });
}
// Records a finished run without blocking the publication of its results. A failure is reported, and the open
// project is never changed by it.
async function persistRunHistory(options) {
  try {
    const { entry, figureSvg } = await captureRun(options);
    history = recordHistoryEntry(history, entry, figureSvg);
    renderHistory();
  } catch (error) {
    reportError("Run history not saved: " + (error.message ?? error));
  }
}
renderHistory();
const outputDecoder = new TextDecoder();
// Runs of the project execute in a Worker, so the page stays responsive. One run is active at a time.
const executor = createExecutor({
  createWorker: () => new Worker(new URL("./workbench-worker.js", import.meta.url), { type: "module" }),
  onChange: renderRunState,
});
window.addEventListener("pagehide", () => executor.dispose());
// Interface-level validity: a result only reaches the screen while its ticket is the current one.
const presentation = createPresentationGuard();
const runStateMessages = { loading: "loading compiler…", running: "running…", cancelled: "cancelled", "timed-out": "timed out", failed: "failed" };
function renderRunState(run) {
  if (status && run.state in runStateMessages) status.textContent = runStateMessages[run.state];
  if (cancelRunButton) cancelRunButton.disabled = !["loading", "running"].includes(run.state);
}
// The structured results of the last run, when it printed a valid RESULTS line; used by the export button.
let lastResultsDocument = null;

// Results become exportable only while they are the ones on screen. Every path that replaces the project, starts a
// run, or ends a run without valid results calls this first.
function clearResultsExport() {
  lastResultsDocument = null;
  const button = $("download-results");
  if (button) button.disabled = true;
}

// Renders the RESULTS table of a run at the top of the output panel. Only DOM APIs are used, so program text
// is never interpreted as HTML. A RESULTS line that is rejected is kept in the log and its reason is shown.
function renderResultsTable(outputNode, lines) {
  clearResultsExport();
  if (!lines || lines.length === 0) return;
  const parsed = parseResults(lines);
  if (!parsed.ok) {
    if (parsed.reason !== "no structured results in this run") {
      const note = document.createElement("p");
      note.className = "results-rejected";
      note.textContent = "Structured results were rejected: " + parsed.reason;
      outputNode.prepend(note);
    }
    return;
  }
  const button = $("download-results");
  const table = resultsTable(parsed.document);
  const wrap = document.createElement("div");
  wrap.className = "results-table";
  const caption = document.createElement("p");
  caption.className = "results-caption";
  caption.textContent = "Structured results: " + parsed.document.experiment;
  const element = document.createElement("table");
  const head = document.createElement("thead");
  const headRow = document.createElement("tr");
  for (const header of table.headers) {
    const th = document.createElement("th");
    th.scope = "col";
    th.textContent = header;
    headRow.append(th);
  }
  head.append(headRow);
  const body = document.createElement("tbody");
  for (const cells of table.rows) {
    const tr = document.createElement("tr");
    for (const cell of cells) {
      const td = document.createElement("td");
      td.textContent = cell;
      tr.append(td);
    }
    body.append(tr);
  }
  element.append(head, body);
  wrap.append(caption, element);
  outputNode.prepend(wrap);
  lastResultsDocument = parsed;
  if (button) button.disabled = false;
}

// [stream, line] pairs for rendering, in the order the program wrote them. The captured bytes are not changed.
function linesFromBytes(stdout, stderr, ordered) {
  if (Array.isArray(ordered)) return orderedLines(ordered);
  const split = (bytes, kind) => {
    const text = outputDecoder.decode(bytes);
    if (text === "") return [];
    const parts = text.endsWith("\n") ? text.slice(0, -1).split("\n") : text.split("\n");
    return parts.map((line) => [kind, line]);
  };
  return [...split(stdout, "out"), ...split(stderr, "err")];
}

function readProjectDraft() {
  try {
    return localStorage.getItem(PROJECT_DRAFT_KEY);
  } catch (_) {
    return null;
  }
}

function writeProjectDraft(text) {
  try {
    localStorage.setItem(PROJECT_DRAFT_KEY, text);
  } catch (_) {
    // The draft is a convenience only; saving the project file is the durable path.
  }
}

function clearProjectDraft() {
  try {
    localStorage.removeItem(PROJECT_DRAFT_KEY);
  } catch (_) {
    // Nothing to clear.
  }
}

// The identity line the same runtime prints for `--version`. Null when it cannot be read.
function compilerIdentity() {
  compilerIdentityPromise ??= runOstrinc({}, ["--version"])
    .then(({ code, lines }) => {
      const line = lines.find(([kind]) => kind === "out");
      return code === 0 && line ? line[1].trim() : null;
    })
    .catch(() => null);
  return compilerIdentityPromise;
}

function syncEditorFromModel() {
  if (!source) return;
  source.value = workbench.project.files[workbench.activePath] ?? "";
  const label = $("active-file-name");
  if (label) label.textContent = workbench.activePath;
}

function refreshExplorer() {
  if (!explorer) return;
  renderExplorer(explorer, workbench, {
    onSelect: (path) => selectWorkbenchFile(path),
    onFocus: (path) => explorer.querySelector(`[data-path="${CSS.escape(path)}"]`)?.focus(),
    onNew: () => createWorkbenchFile(),
    onRename: () => renameWorkbenchFile(),
    onDelete: () => deleteWorkbenchFile(),
    onEntry: () => setWorkbenchEntry(),
  });
  if (exportProjectButton) exportProjectButton.disabled = false;
  if (manifestButton) manifestButton.disabled = workbench.lastRun === null;
  if (status && isDirty(workbench)) status.textContent = `unsaved changes: ${dirtyFiles(workbench).join(", ")}`;
}

function persistProjectDraft() {
  try {
    writeProjectDraft(serializeProject(commitEdit(workbench, source.value).project));
  } catch (_) {
    // A project that is temporarily invalid (for example, an oversized file) is simply not drafted.
  }
}

function reportError(message) {
  if (status) status.textContent = message;
}

function selectWorkbenchFile(path) {
  try {
    workbench = selectFile(workbench, path, source.value);
    syncEditorFromModel();
    clearSourceDiagnostic(source, sourceStatus);
    refreshExplorer();
  } catch (error) {
    reportError(String(error.message ?? error));
  }
}

function createWorkbenchFile() {
  const name = window.prompt("New file name (for example helpers.ostrin or lib/geometry.ostrin)", "helpers.ostrin");
  if (name === null) return;
  const path = name.trim();
  try {
    workbench = addFile(commitEdit(workbench, source.value), path, `pub fn value() -> Int {\n    0\n}\n`);
    syncEditorFromModel();
    refreshExplorer();
    persistProjectDraft();
  } catch (error) {
    reportError(String(error.message ?? error));
  }
}

function renameWorkbenchFile() {
  const name = window.prompt("New name for the selected file", workbench.activePath);
  if (name === null) return;
  try {
    workbench = renameActive(workbench, name.trim(), source.value);
    refreshExplorer();
    persistProjectDraft();
  } catch (error) {
    reportError(String(error.message ?? error));
  }
}

function deleteWorkbenchFile() {
  if (!window.confirm(`Delete ${workbench.activePath}? This cannot be undone.`)) return;
  try {
    workbench = deleteFile(commitEdit(workbench, source.value), workbench.activePath);
    syncEditorFromModel();
    refreshExplorer();
    persistProjectDraft();
  } catch (error) {
    reportError(String(error.message ?? error));
  }
}

function setWorkbenchEntry() {
  try {
    workbench = setEntry(commitEdit(workbench, source.value), workbench.activePath);
    refreshExplorer();
    persistProjectDraft();
  } catch (error) {
    reportError(String(error.message ?? error));
  }
}

function downloadProject() {
  try {
    const text = serializeProject(commitEdit(workbench, source.value).project);
    download(`${workbench.project.name.replace(/[^A-Za-z0-9_-]+/g, "-") || "project"}.ostrin.project.json`, text, "application/json");
    workbench = markSaved(commitEdit(workbench, source.value));
    refreshExplorer();
    clearProjectDraft();
  } catch (error) {
    reportError(String(error.message ?? error));
  }
}

// Unsaved edits are not silently replaced: the user confirms before any other project is loaded.
function confirmDiscard() {
  // The editor text is part of the unsaved state until it is committed to the model.
  workbench = commitEdit(workbench, source.value);
  return !isDirty(workbench) || window.confirm("Discard the unsaved changes in this project?");
}

async function importProjectFile(file) {
  if (!confirmDiscard()) return;
  if (file.size > 2 * 1024 * 1024) {
    reportError("project file too large (2 MB limit)");
    return;
  }
  const text = await file.text();
  let next;
  try {
    // A project document is recognised by its content (a JSON object), not by the file name: a renamed or
    // temporary file must still import as a project. Anything else is one source file, as before.
    next = /\.json$/i.test(file.name) || text.trimStart().startsWith("{")
      ? importProjectText(workbench, text)
      : createWorkbench(projectFromSourceFile(file.name, text));
  } catch (error) {
    // The current project stays as it was: nothing above replaced the state.
    reportError(`Import refused: ${error.message ?? error}`);
    return;
  }
  // Only a successful import replaces the project, so only then does a running program become stale.
  executor.cancel();
  presentation.changeProject();
  workbench = next;
  clearResultsExport();
  syncEditorFromModel();
  if (output) output.textContent = `Opened ${file.name}. Nothing has run yet; press Run to execute the entry.`;
  clearSourceDiagnostic(source, sourceStatus);
  refreshExplorer();
  persistProjectDraft();
}

function downloadManifest() {
  if (!workbench.lastRun) {
    reportError("Run the project first; the manifest describes the last run.");
    return;
  }
  manifestForLastRun(workbench)
    .then((manifest) => download("run-manifest.json", JSON.stringify(manifest, null, 2) + "\n", "application/json"))
    .catch((error) => reportError(String(error.message ?? error)));
}

// Browser storage can be missing or throw (private windows, blocked site data); the page must work
// without it, so every access is wrapped.
function readDraft() {
  try {
    return localStorage.getItem(DRAFT_KEY);
  } catch (_) {
    return null;
  }
}

function writeDraft(text) {
  try {
    localStorage.setItem(DRAFT_KEY, text);
  } catch (_) {
    // Persistence is a convenience only.
  }
}

function clearDraft() {
  try {
    localStorage.removeItem(DRAFT_KEY);
  } catch (_) {
    // Nothing to clear.
  }
}

function download(filename, text, type) {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

// A program that prints one standalone SVG document (std.viz figures do) is shown as an image, with the
// remaining text kept as output and the SVG source available to download. The image is loaded through a
// data URL, so scripts inside an SVG never run in the page.
function extractFigure(lines) {
  const text = lines.filter(([kind]) => kind === "out").map(([, line]) => line).join("\n");
  // Several figures (or stray markup) cannot be one image; keep the ordinary text output instead.
  if (text.split("<svg").length !== 2 || text.split("</svg>").length !== 2) return null;
  const start = text.indexOf("<svg");
  const close = text.lastIndexOf("</svg>");
  if (start < 0 || close < start) return null;
  const end = close + "</svg>".length;
  const svg = text.slice(start, end);
  if (svg.length > 5 * 1024 * 1024) return null;
  return { svg, before: text.slice(0, start).trim(), after: text.slice(end).trim() };
}

function figureHtml(figure) {
  const url = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(figure.svg);
  const text = [figure.before, figure.after].filter(Boolean).map(escapeHtml).join("\n");
  return (text ? `<span>${text}</span>\n` : "") +
    `<span class="figure-preview"><img alt="Figure produced by the program" src="${url}"></span>\n` +
    `<span class="figure-actions"><button type="button" class="button-quiet" data-download-svg>Download SVG</button></span>`;
}

const loadModule = loadCompiler;

function escapeHtml(text) {
  return text.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]);
}

async function invoke(sourceText, flags) {
  const normalizedFlags = Array.isArray(flags) ? flags : [flags];
  return runOstrinc({ "main.ostrin": sourceText }, [...normalizedFlags, "main.ostrin"]);
}

function parseDiagnostic(text) {
  try {
    const value = JSON.parse(text);
    if (value && typeof value === "object" && typeof value.message === "string") return value;
  } catch (_) {
    // Compiler output from runtime traps and older builds can still be plain text.
  }
  return null;
}

function renderDiagnostic(diagnostic) {
  const severity = diagnostic.severity === "warning" ? "warning" : "error";
  const code = diagnostic.code ? `OSTRIN-${diagnostic.code}` : "Ostrin diagnostic";
  const file = typeof diagnostic.file === "string" ? diagnostic.file : "main.ostrin";
  const position = [diagnostic.line, diagnostic.column]
    .filter((value) => Number.isInteger(value))
    .join(":");
  const location = position ? `${file}:${position}` : file;
  return `<span class="diagnostic ${severity}"><span class="diagnostic-head"><span class="diagnostic-code">${escapeHtml(code)}</span> <span class="diagnostic-location">${escapeHtml(location)}</span></span><br><span class="diagnostic-message">${escapeHtml(diagnostic.message)}</span></span>`;
}

function renderLine([kind, text]) {
  const diagnostic = parseDiagnostic(text);
  if (diagnostic) return renderDiagnostic(diagnostic);
  return `<span class="${kind === "err" ? "err" : ""}">${escapeHtml(text)}</span>`;
}

function diagnosticSummary(diagnostics) {
  const errors = diagnostics.filter(({ severity }) => severity !== "warning").length;
  const warnings = diagnostics.length - errors;
  const parts = [];
  if (errors) parts.push(`${errors} error${errors === 1 ? "" : "s"}`);
  if (warnings) parts.push(`${warnings} warning${warnings === 1 ? "" : "s"}`);
  return parts.join(", ");
}

function clearSourceDiagnostic(sourceNode, locationNode = null) {
  sourceNode?.classList.remove("has-diagnostic");
  if (locationNode) locationNode.textContent = "";
}

function focusDiagnosticLine(sourceNode, diagnostic, locationNode = null) {
  if (!sourceNode || !Number.isInteger(diagnostic.line) || diagnostic.line < 1) return;
  const lines = sourceNode.value.split("\n");
  const lineIndex = diagnostic.line - 1;
  if (lineIndex >= lines.length) return;
  const start = lines.slice(0, lineIndex).reduce((offset, line) => offset + line.length + 1, 0);
  const end = start + lines[lineIndex].length;
  sourceNode.classList.add("has-diagnostic");
  sourceNode.focus({ preventScroll: true });
  sourceNode.setSelectionRange(start, end);
  if (locationNode) {
    const column = Number.isInteger(diagnostic.column) ? ` · column ${diagnostic.column}` : "";
    locationNode.textContent = `line ${diagnostic.line}${column}`;
  }
}

async function execute(flag, sourceNode = source, outputNode = output, statusNode = status, controls = buttons) {
  if (!sourceNode || !outputNode || !statusNode) return;
  // Only the newest Workbench run may publish anything: output, figure, evidence, status or controls.
  // Everything before publish() is computed into local values and touches no shared state.
  const ticket = sourceNode === source ? presentation.begin() : null;
  const current = () => ticket === null || presentation.isCurrent(ticket);
  const publish = (action) => {
    if (current()) action();
  };
  controls.forEach((b) => (b.disabled = true));
  statusNode.textContent = "running…";
  const started = performance.now();
  const startedAt = new Date().toISOString();
  let runId = null;
  try {
    const flags = ["--check", "--run"].includes(flag) ? [flag, "--json"] : [flag];
    const isWorkbench = sourceNode === source;
    let runInputSnapshot = null;
    let rawRun = null;
    let command = null;
    let code;
    let lines;
    if (isWorkbench) {
      // Format works on the active file; every other command runs the project from its entry.
      workbench = commitEdit(workbench, sourceNode.value);
      runInputSnapshot = runInput(workbench);
      const args = flag === "--fmt" ? [flag, workbench.activePath] : [...flags, runInputSnapshot.entry];
      command = args.join(" ");
      // The previous results are no longer the ones on screen once a new run starts.
      clearResultsExport();
      const outcome = await executor.run({ command: args, entry: runInputSnapshot.entry, files: runInputSnapshot.files });
      runId = outcome.id;
      if (!current()) return;
      if (outcome.state !== "completed") {
        // Cancelled, timed-out or failed runs show no results; the editor keeps its text.
        const message = outcome.state === "cancelled"
          ? "Run cancelled. Your edits are kept."
          : outcome.state === "timed-out"
            ? "The run timed out: " + (outcome.error?.message ?? "")
            : "The run failed: " + (outcome.error?.message ?? "unknown error");
        publish(() => {
          figureStore.delete(outputNode);
          clearResultsExport();
          outputNode.textContent = message;
          statusNode.textContent = runStateMessages[outcome.state] ?? outcome.state;
          // An incomplete run is recorded as incomplete evidence: no output, no results, no figure.
          if (isWorkbench) {
            persistRunHistory({
              id: outcome.id,
              input: runInputSnapshot,
              status: outcome.state,
              command,
              startedAt,
              durationMs: performance.now() - started,
              projectId: projectIdOf(runInputSnapshot.project),
            });
          }
        });
        return;
      }
      code = outcome.result.code;
      lines = linesFromBytes(outcome.result.stdout, outcome.result.stderr, outcome.result.ordered);
      rawRun = outcome.result;
    } else {
      ({ code, lines } = await invoke(sourceNode.value, flags));
    }
    // Prepared locally: none of this is visible until publish() runs.
    const diagnostics = lines.map(([, text]) => parseDiagnostic(text)).filter(Boolean);
    // The structured RESULTS line is not part of the log; it is rendered as a table. Its bytes stay in the run.
    // Only a valid RESULTS line leaves the log; a rejected one stays visible with its reason.
    const structuredOk = parseResults(lines).ok;
    const logLines = lines.filter(([kind, text]) => !(structuredOk && kind === "out" && text.startsWith(RESULTS_PREFIX)));
    let html = logLines.map(renderLine).join("\n");
    // The figure's surrounding text comes from the log lines only, never from the structured RESULTS line.
    const figure = flag === "--run" && code === 0 ? extractFigure(logLines) : null;
    if (figure) {
      const errors = lines.filter(([kind]) => kind !== "out").map(renderLine).join("\n");
      html = (errors ? errors + "\n" : "") + figureHtml(figure);
    }
    if (flag === "--fmt" && code === 0) {
      // --fmt prints the formatted source: put it back in the editor.
      html = '<span class="ok">formatted</span>';
    } else if (code === 0 && !html && flag === "--check") {
      html = '<span class="ok">no errors</span>';
    }
    // Only checks and runs are evidence. The identity lookup is the only await left, so the ticket is checked
    // again after it, before anything is published.
    const identity = isWorkbench && flag !== "--fmt" ? await compilerIdentity() : null;
    if (!current()) return;
    const elapsed = Math.round(performance.now() - started);
    const summary = diagnostics.length ? diagnosticSummary(diagnostics) : (code === 0 ? "ok" : "exit " + code);
    publish(() => {
      if (figure) figureStore.set(outputNode, figure.svg);
      else figureStore.delete(outputNode);
      if (isWorkbench && flag !== "--fmt") {
        workbench = recordRun(workbench, runInputSnapshot, {
          command,
          code,
          lines,
          raw: rawRun,
          figureSvg: figure ? figure.svg : null,
          compilerIdentity: identity,
        });
        refreshExplorer();
        persistRunHistory({
          id: runId,
          input: runInputSnapshot,
          status: "completed",
          command,
          startedAt,
          durationMs: performance.now() - started,
          exitCode: code,
          compilerIdentity: identity,
          lines,
          stdoutBytes: rawRun.stdout,
          stderrBytes: rawRun.stderr,
          figureSvg: figure ? figure.svg : null,
          projectId: projectIdOf(runInputSnapshot.project),
        });
      }
      if (flag === "--fmt" && code === 0) {
        sourceNode.value = lines.filter(([kind]) => kind === "out").map(([, text]) => text).join("\n") + "\n";
      }
      outputNode.innerHTML = html || '<span class="dim">(no output)</span>';
      // Only Workbench runs own the results export; a standalone demo run never touches it.
      if (isWorkbench) renderResultsTable(outputNode, flag === "--run" ? lines : []);
      // Measurement hook for the benchmark harness: the timing of the last Workbench run, read-only for pages.
      if (rawRun) window.__ostrinLastRun = { ...rawRun.timing, wallMs: performance.now() - started, command };
      statusNode.textContent = `${summary} · ${elapsed} ms`;
      if (diagnostics.length) focusDiagnosticLine(sourceNode, diagnostics[0], sourceNode === source ? sourceStatus : null);
      else clearSourceDiagnostic(sourceNode, sourceNode === source ? sourceStatus : null);
    });
  } catch (error) {
    // An exception from a stale run must not replace the output or status of a newer one.
    publish(() => {
      figureStore.delete(outputNode);
      if (sourceNode === source) clearResultsExport();
      outputNode.innerHTML = `<span class="err">${escapeHtml(String(error))}</span>`;
      statusNode.textContent = "failed to start";
      clearSourceDiagnostic(sourceNode, sourceNode === source ? sourceStatus : null);
    });
  } finally {
    // A stale run must not re-enable controls while a newer run is active. When no run is active (for example
    // after a project change cancelled this one), nothing else would re-enable them, so this run does.
    if (current() || executor.activeRun === null) controls.forEach((b) => (b.disabled = false));
    if (ticket !== null) presentation.end(ticket);
  }
}

const select = $("example");
if (select && source) {
  for (const name of Object.keys(EXAMPLES)) select.append(new Option(name, name));
  let currentExample = select.value;
  select.addEventListener("change", () => {
    if (!confirmDiscard()) {
      select.value = currentExample;
      return;
    }
    currentExample = select.value;
    loadExampleProject(select.value);
    clearDraft();
    clearProjectDraft();
    if (output) output.textContent = "";
    if (status) status.textContent = "";
    clearSourceDiagnostic(source, sourceStatus);
  });
  const sharedCode = new URLSearchParams(location.search).get("code");
  if (sharedCode !== null) {
    workbench = createWorkbench(projectFromSourceFile("main.ostrin", sharedCode));
    syncEditorFromModel();
    if (output) output.textContent = "Shared source loaded. Press Run (Ctrl + Enter).";
  } else {
    const projectDraft = readProjectDraft();
    const legacyDraft = projectDraft === null ? readDraft() : null;
    if (projectDraft !== null && restoreProjectDraft(projectDraft)) {
      if (output) output.textContent = "Restored your unsaved project. Export it to keep a copy, or press Reset to return to the selected example.";
    } else if (legacyDraft) {
      workbench = createWorkbench(projectFromSourceFile("main.ostrin", legacyDraft));
      syncEditorFromModel();
      if (output) output.textContent = "Restored your last draft. Press Reset to return to the selected example.";
    } else {
      loadExampleProject(select.value);
    }
  }
  clearSourceDiagnostic(source, sourceStatus);
  refreshExplorer();
}

function loadExampleProject(name) {
  executor.cancel();
  clearResultsExport();
  presentation.changeProject();
  workbench = createWorkbench({ ...projectFromSourceFile("main.ostrin", EXAMPLES[name]), name });
  syncEditorFromModel();
  refreshExplorer();
}

// A restored draft is shown as unsaved: its saved baseline is empty, so every file is marked.
function restoreProjectDraft(text) {
  try {
    const restored = importProjectText(workbench, text);
    workbench = { ...restored, saved: { name: "", entry: "", files: {} } };
    syncEditorFromModel();
    return true;
  } catch (_) {
    clearProjectDraft();
    return false;
  }
}

output?.addEventListener("click", (event) => {
  if (event.target instanceof Element && event.target.closest("[data-download-svg]")) {
    const svg = figureStore.get(output);
    if (svg) download("figure.svg", svg, "image/svg+xml");
  }
});
saveButton?.addEventListener("click", () => {
  if (!source) return;
  // Saves the active file as a single .ostrin file, as before; the project is saved from its own button.
  download(workbench.activePath, commitEdit(workbench, source.value).project.files[workbench.activePath], "text/plain");
});
openButton?.addEventListener("click", () => openInput?.click());
openInput?.addEventListener("change", async () => {
  const file = openInput.files?.[0];
  openInput.value = "";
  if (!file || !source) return;
  await importProjectFile(file);
});
resetButton?.addEventListener("click", () => {
  if (!source || !select) return;
  if (!confirmDiscard()) return;
  loadExampleProject(select.value);
  clearDraft();
  clearProjectDraft();
  if (output) output.textContent = "";
  status.textContent = "";
  clearSourceDiagnostic(source, sourceStatus);
});
cancelRunButton?.addEventListener("click", () => {
  // Terminates the active Worker. The run's result is then reported as cancelled; no late result is shown.
  executor.cancel();
});
exportProjectButton?.addEventListener("click", downloadProject);
importProjectButton?.addEventListener("click", () => importProjectInput?.click());
importProjectInput?.addEventListener("change", async () => {
  const file = importProjectInput.files?.[0];
  importProjectInput.value = "";
  if (file) await importProjectFile(file);
});
manifestButton?.addEventListener("click", downloadManifest);
async function loadDemo(documentPath, label) {
  if (!confirmDiscard()) return;
  try {
    const response = await fetch(documentPath, { cache: "no-cache" });
    if (!response.ok) throw new Error("HTTP " + response.status);
    executor.cancel();
    presentation.changeProject();
    clearResultsExport();
    workbench = importProjectText(workbench, await response.text());
    syncEditorFromModel();
    clearSourceDiagnostic(source, sourceStatus);
    if (output) output.textContent = "Loaded " + label + ". Nothing has run yet: press Run to execute main.ostrin.";
    refreshExplorer();
    persistProjectDraft();
  } catch (error) {
    reportError("Demo not loaded: " + (error.message ?? error));
  }
}
$("demo-project")?.addEventListener("click", () => loadDemo("workbench-demos/cubic-root.ostrin.project.json", "the cubic root demonstration"));
$("demo-sweep")?.addEventListener("click", () => loadDemo("workbench-demos/parameter-sweep.ostrin.project.json", "the parameter sweep"));
$("demo-sweep-even")?.addEventListener("click", () => loadDemo("workbench-demos/parameter-sweep-even.ostrin.project.json", "the parameter sweep for c = 2, 4, 6, 8"));
$("download-results")?.addEventListener("click", () => {
  if (!lastResultsDocument) return;
  // The file is the exact payload of the RESULTS line that was validated and shown.
  download("results.json", resultsExportText(lastResultsDocument), "application/json");
});
let draftTimer = 0;
source?.addEventListener("input", () => {
  clearTimeout(draftTimer);
  draftTimer = setTimeout(persistProjectDraft, 400);
});
$("run")?.addEventListener("click", () => execute("--run"));
$("check")?.addEventListener("click", () => execute("--check"));
$("test")?.addEventListener("click", () => execute("--test"));
$("format")?.addEventListener("click", () => execute("--fmt"));
shareButton?.addEventListener("click", async () => {
  if (!source) return;
  const url = new URL(location.href);
  url.search = "";
  url.searchParams.set("code", source.value);
  try {
    await navigator.clipboard.writeText(url.href);
    status.textContent = "link copied";
  } catch (_) {
    window.prompt("Copy this playground link", url.href);
  }
});
source?.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
    event.preventDefault();
    execute("--run");
  } else if (event.key === "Tab") {
    event.preventDefault();
    const { selectionStart: start, selectionEnd: end } = source;
    source.setRangeText("    ", start, end, "end");
  }
});

const liveExamples = {
  quantities: { code: `fn velocity(distance: Quantity<Length>, time: Quantity<Time>) -> Quantity<Length / Time> {
    distance / time
}

fn double<D: Dimension>(x: Quantity<D>) -> Quantity<D> {
    x * 2
}

fn main() -> Void {
    a = 5 nm
    b = 2 m
    total = a + b
    print(total)

    v = velocity(10 m, 2 s)
    print(v)

    doubled = double(3 kg)
    print(doubled)

    ratio = b / a
    print(ratio)

    mut n = 0
    for i in 1 to 5 {
        n = n + i
    }
    print(n)
}
` },
  standard: { code: `import std.math
import std.lists
import std.strings
import std.time
import std.json
import std.maps

fn main() -> Void {
    print(math.min(3, 9))
    print(math.max(2.5, 1.5))
    print(math.clamp(15, 0, 10))
    print(math.gcd(12, 18))
    print(math.lcm(4, 6))
    print(math.pow_int(2, 10))
    print(lists.contains([1, 2, 3], 2))
    print(lists.index_of(["a", "b"], "b"))
    print(lists.reversed([1, 2, 3]))
    print(lists.sorted([3, 1, 2, 5, 4]))
    print(lists.sorted(["pear", "apple", "fig"]))
    print(lists.take([1, 2, 3, 4], 2))
    print(lists.concat([1], [2, 3]))
    print(lists.range_list(2, 6))
    print(lists.max_of([4, 9, 2]))
    print(lists.min_of([4, 9, 2]))
    print(strings.repeat("ab", 3))
    print(strings.pad_left("7", 3, "0"))
    print(strings.count_of("a,b,c", ","))
    print(strings.trim("  Ostrin  "))
    print(strings.split("a,b,c", ","))
    print(strings.lines("a\nb"))
    print(strings.is_blank("   "))
    print(strings.format_text("{} + {} = {}", ["2", "3", "5"]))
    print(strings.char_at("Ostrin", 1))
    print(strings.slice("Ostrin", 1, 4))
    print(strings.char_at("Ostrin", 1).codepoint().unwrap())
    letter = strings.char_at("Ostrin", 1)
    print(letter.codepoint().unwrap())
    print(time.iso(time.date(2024, 2, 29)))
    print(time.day_of_week(time.date(2024, 1, 1)))
    print(time.parse_iso("2024-02-29").unwrap().day)
    json_value = json.parse("{\"ok\":true,\"items\":[1,2]}").unwrap()
    print(json.stringify(json_value))
    print(json.object_keys(json_value).unwrap())
    scores = ["ann": 3, "bob": 5]
    print(maps.count(scores))
    print(maps.is_empty(scores))
    print(maps.contains_key(scores, "ann"))
    print(maps.contains_key(scores, "zed"))
    print(maps.get_or(scores, "ann", 0))
    print(maps.get_or(scores, "zed", 0))
    map_keys = maps.keys(scores)
    print(map_keys.length())
    drop(map_keys)
    map_values = maps.values(scores)
    print(map_values.length())
    drop(map_values)
    print(strings.format_float(3.14159, 2).unwrap())
    print(strings.format_float(-0.125, 3).unwrap())
    print(strings.format_float(2.0, 0).unwrap())
    print(strings.format_float(1.0, 19).is_err())
}
` },
  records: { code: `enum Inner {
    Ready(Int)
    Empty
}

enum Outer {
    Wrapped(Inner)
    None
}

record Point {
    x: Int
    y: Int
}

fn inspect(value: Outer) -> Int {
    match value {
        Wrapped(Ready(40 to 50)) => 42,
        Wrapped(Ready(value)) => value,
        _ => 0,
    }
}

fn read_x(point: Point) -> Int {
    match point {
        Point(x: x, y: _) => x,
    }
}

fn main() -> Void {
    print(inspect(Wrapped(Ready(42))))
    print(inspect(Wrapped(Empty)))
    print(read_x(Point { x: 5, y: 9 }))
}
` },
  concurrency: { code: `fn main() -> Void {
    ch = channel<Int>()

    producer = spawn {
        for i in 1 to 5 {
            ch.send(i)
        }
        ch.close()
    }

    for value in ch {
        print(value)
    }

    task = spawn {
        3 + 4
    }
    print(task.join())
}
` },
  temperature: { code: `// Statistics over lists of absolute temperatures: scale conversion, hottest and coldest points,
// the temperature span as an interval, and the affine mean. Points are never summed.
import std.measurements

fn show(result: Result<TemperaturePoint, String>) -> Void {
    match result {
        Ok(point) => print(point),
        Err(message) => print(message),
    }
}

fn main() -> Void {
    readings = [
        temperature_point(20.0, "degC"),
        temperature_point(68.0, "degF"),
        temperature_point(300.0, "K"),
        temperature_point(-5.0, "degC")
    ]
    show(measurements.temperature_points_hottest(readings))
    show(measurements.temperature_points_coldest(readings))
    show(measurements.temperature_points_mean(readings))
    match measurements.temperature_points_span(readings) {
        Ok(span) => print(span),
        Err(message) => print(message),
    }
    kelvin = measurements.temperature_points_to(readings, "K")
    show(measurements.temperature_points_mean(kelvin))
    empty: List<TemperaturePoint> = []
    show(measurements.temperature_points_mean(empty))
}
` },
  numeric: { code: `// Adaptive Simpson quadrature with an error estimate, and Brent's root finder, checked against
// closed-form values: the integral of sin over [0, pi] is 2, of sqrt over [0, 1] is 2/3 and of
// exp over [0, 1] is e - 1; Wallis' cubic x^3 - 2x - 5 has its root at 2.0945514815423265 and the
// fixed point of cos is 0.7390851332151607.
import std.numeric

fn sine(x: Float) -> Float { sin(x) }
fn root(x: Float) -> Float { sqrt(x) }
fn growth(x: Float) -> Float { exp(x) }
fn wallis(x: Float) -> Float { x * x * x - 2.0 * x - 5.0 }
fn fixed_cos(x: Float) -> Float { cos(x) - x }
fn bad_root(x: Float) -> Float { sqrt(x) }

fn quad_line(name: String, value: Float, error: Float, converged: Bool, evaluations: Int, exact: Float) -> Void {
    print(name + ": within 1e-9 = " + (abs(value - exact) < 1e-9).to_string() + ", error estimate ok = " + (error < 1e-8).to_string() + ", converged = " + converged.to_string() + ", evaluations = " + evaluations.to_string())
}

fn root_line(name: String, result: Result<Float, String>, exact: Float) -> Void {
    match result {
        Ok(x) => print(name + ": within 1e-12 = " + (abs(x - exact) < 1e-12).to_string()),
        Err(message) => print(name + ": " + message),
    }
}

fn main() -> Void {
    match numeric.adaptive_simpson(sine, 0.0, 3.141592653589793) {
        Ok(q) => quad_line("sin", q.value, q.error, q.converged, q.evaluations, 2.0),
        Err(message) => print("sin: " + message),
    }
    match numeric.adaptive_simpson(root, 0.0, 1.0, 1e-10, 40) {
        Ok(q) => quad_line("sqrt", q.value, q.error, q.converged, q.evaluations, 2.0 / 3.0),
        Err(message) => print("sqrt: " + message),
    }
    match numeric.adaptive_simpson(growth, 1.0, 0.0) {
        Ok(q) => quad_line("exp reversed", q.value, q.error, q.converged, q.evaluations, -(2.718281828459045 - 1.0)),
        Err(message) => print("exp reversed: " + message),
    }
    match numeric.adaptive_simpson(sine, 1.0, 1.0) {
        Ok(q) => quad_line("empty", q.value, q.error, q.converged, q.evaluations, 0.0),
        Err(message) => print("empty: " + message),
    }
    match numeric.adaptive_simpson(sine, 0.0, 1.0, 0.0) {
        Ok(q) => quad_line("bad tolerance", q.value, q.error, q.converged, q.evaluations, 0.0),
        Err(message) => print("bad tolerance: " + message),
    }
    match numeric.adaptive_simpson(root, 0.0, 1.0, 1e-14, 3) {
        Ok(q) => quad_line("depth limited", q.value, q.error, q.converged, q.evaluations, 2.0 / 3.0),
        Err(message) => print("depth limited: " + message),
    }
    match numeric.adaptive_simpson(bad_root, -1.0, 1.0) {
        Ok(q) => quad_line("non-finite", q.value, q.error, q.converged, q.evaluations, 0.0),
        Err(message) => print("non-finite: " + message),
    }
    nan = sqrt(-1.0)
    print("nan comparisons: " + (nan == nan).to_string() + " " + (nan != nan).to_string() + " " + (nan <= 1.0).to_string() + " " + (nan >= 1.0).to_string())
    root_line("wallis", numeric.brent(wallis, 2.0, 3.0), 2.0945514815423265)
    root_line("fixed point of cos", numeric.brent(fixed_cos, 0.0, 1.0), 0.7390851332151607)
    root_line("same sign", numeric.brent(wallis, 3.0, 4.0), 0.0)
}
` },
};

const liveControls = [];
for (const card of document.querySelectorAll("[data-live-example]")) {
  const definition = liveExamples[card.dataset.liveExample];
  const liveSource = card.querySelector("[data-live-source]");
  const liveOutput = card.querySelector("[data-live-output]");
  const liveStatus = card.querySelector("[data-live-status]");
  if (!definition || !liveSource || !liveOutput || !liveStatus) continue;
  const run = card.querySelector('[data-live-action="run"]');
  const check = card.querySelector('[data-live-action="check"]');
  const reset = card.querySelector('[data-live-action="reset"]');
  const copy = card.querySelector('[data-live-action="copy"]');
  liveSource.value = definition.code;
  const controls = [run, check, reset, copy].filter(Boolean);
  liveControls.push(...controls);
  run?.addEventListener("click", () => execute("--run", liveSource, liveOutput, liveStatus, controls));
  check?.addEventListener("click", () => execute("--check", liveSource, liveOutput, liveStatus, controls));
  reset?.addEventListener("click", () => {
    liveSource.value = definition.code;
    liveOutput.innerHTML = '<span class="dim">Ready. Press Run.</span>';
    liveStatus.textContent = "";
    clearSourceDiagnostic(liveSource);
  });
  copy?.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(liveSource.value);
      liveStatus.textContent = "copied";
    } catch (_) {
      liveStatus.textContent = "select the source to copy";
    }
  });
  liveOutput.innerHTML = '<span class="dim">Ready. Press Run.</span>';
}

const allControls = [...buttons, ...liveControls];
if (allControls.length) {
  allControls.forEach((b) => (b.disabled = true));
  loadModule().then(
    () => {
      if (output) output.innerHTML = '<span class="dim">Ready. Press Run (Ctrl + Enter).</span>';
      document.querySelectorAll("[data-live-output]").forEach((node) => {
        node.innerHTML = '<span class="dim">Ready. Press Run.</span>';
      });
      allControls.forEach((b) => (b.disabled = false));
    },
    (error) => {
      const message = `Could not load ostrinc.wasm: ${escapeHtml(String(error))}`;
      if (output) output.innerHTML = `<span class="err">${message}\nBuild it with: cargo build --manifest-path compiler/Cargo.toml --target wasm32-wasip1 --release</span>`;
      document.querySelectorAll("[data-live-output]").forEach((node) => {
        node.innerHTML = `<span class="err">${message}</span>`;
      });
    },
  );
}
