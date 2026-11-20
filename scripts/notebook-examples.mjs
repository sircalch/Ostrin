// Generates the three example notebooks in website/notebooks/ from the cell sources below.
//
// Usage: node scripts/notebook-examples.mjs --write    (writes the files and validates each one)
//        node scripts/notebook-examples.mjs --check    (verifies that the committed files are current)
//
// The notebooks contain code and text only, plus the attached data of the calibration experiment. They carry no
// recorded outputs: every output appears when a cell is run in the page. The sources are kept here, as text, so
// the examples can be reviewed and regenerated; scripts/notebook-examples.test.mjs runs every cell with the native
// compiler and checks its outputs against independent references.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  RATIONAL_TEXT,
  RATIONAL_PARAMETERS,
  RATIONAL_POLYNOMIALS,
  RATIONAL_DIVISION,
  RATIONAL_CONSTRUCTION,
  RATIONAL_DOMAIN,
  RATIONAL_SOLVE,
  RATIONAL_SOLVE_EXCLUDED,
  RATIONAL_IDENTITY,
  RATIONAL_PARTIAL,
  RATIONAL_VERIFY,
  RATIONAL_TABLE,
  RATIONAL_FIGURE,
  RATIONAL_INTERPRETATION,
} from "./notebook-f-cells.mjs";
import {
  addCell,
  attachResource,
  createNotebook,
  serializeNotebook,
  setDeps,
  setSource,
  validateNotebook,
} from "../website/workbench-notebook.js";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const outDir = join(root, "website", "notebooks");
const NOW = "2026-11-11T09:00:00.000Z";

// ---------------------------------------------------------------- A: experimental data analysis

const EXPERIMENT_TEXT = `# Experimental data analysis

The calibration experiment (36 measurements of a response against a temperature) is attached as data/calibration.csv.
Each code cell is compiled and run on its own; cells that need the reader use c1, which only defines functions.

Run All computes the statistics, the linear and quadratic regressions, the predictions and the residuals, and the two
figures. Editing the data or c1 makes every output stale until the cells are run again.`;

const EXPERIMENT_DEFINITIONS = `import std.data

// The reader for the attached experiment. A failure is printed and an empty-safe array is returned.
pub fn column(name: String) -> Array<Float> {
    match read_file("data/calibration.csv") {
        Ok(text) => {
            match data.read_table(text, ",", ["run", "operator", "temperature_c", "pressure_kpa", "response"], ["int", "text", "float", "float", "float"]) {
                Ok(table) => {
                    match data.floats(table, name) {
                        Ok(values) => values,
                        Err(message) => {
                            print("column " + name + ": " + message)
                            array([0.0])
                        },
                    }
                },
                Err(message) => {
                    print("invalid experiment: " + message)
                    array([0.0])
                },
            }
        },
        Err(message) => {
            print("cannot read data/calibration.csv")
            array([0.0])
        },
    }
}
`;

const EXPERIMENT_STATISTICS = `import std.data
import c1

fn main() -> Void {
    temperature = c1.column("temperature_c")
    response = c1.column("response")
    match data.describe(response) {
        Ok(s) => {
            print("OSTRIN-TABLE Descriptive statistics of the response")
            print("n,mean,sd,min,max,median")
            print(s.n.to_string() + "," + s.mean.to_string() + "," + s.sd.to_string() + "," + s.min.to_string() + "," + s.max.to_string() + "," + s.median.to_string())
            print("OSTRIN-TABLE-END")
        },
        Err(message) => print("error: " + message),
    }
    print("correlation of temperature and response: " + data.correlation(temperature, response).unwrap().to_string())
    match data.fit_polynomial(temperature, response, 1) {
        Ok(f) => print("linear fit: sse " + f.sse.to_string() + ", rmse " + f.rmse.to_string() + ", r2 " + f.r_squared.to_string()),
        Err(message) => print("error: " + message),
    }
    match data.fit_polynomial(temperature, response, 2) {
        Ok(f) => {
            print("quadratic fit: sse " + f.sse.to_string() + ", rmse " + f.rmse.to_string() + ", r2 " + f.r_squared.to_string())
            print("OSTRIN-TABLE Quadratic fit: observed, predicted and residual values")
            print(data.table_text(data.fit_table(temperature, response, f, "temperature_c", "response"), ",").trim())
            print("OSTRIN-TABLE-END")
        },
        Err(message) => print("error: " + message),
    }
}
`;

const EXPERIMENT_FIT_FIGURE = `import std.data
import std.viz
import c1

fn main() -> Void {
    temperature = c1.column("temperature_c")
    response = c1.column("response")
    match data.fit_polynomial(temperature, response, 2) {
        Ok(f) => {
            mut low = temperature[0]
            mut high = temperature[0]
            for i in 0 until temperature.length() {
                if temperature[i] < low {
                    low = temperature[i]
                }
                if temperature[i] > high {
                    high = temperature[i]
                }
            }
            grid = linspace(low, high, 101)
            curve = data.predict_all(f, grid)
            fig = viz.figure("Calibration: response against temperature")
                .describe("36 measurements and a quadratic least-squares fit, from the attached data")
                .labels("temperature / deg C", "response")
                .scatter(temperature, response, label: "measurements")
                .line(grid, curve, label: "quadratic fit")
            print(fig.svg())
        },
        Err(message) => print("error: " + message),
    }
}
`;

const EXPERIMENT_RESIDUAL_FIGURE = `import std.data
import std.viz
import c1

fn main() -> Void {
    temperature = c1.column("temperature_c")
    response = c1.column("response")
    match data.fit_polynomial(temperature, response, 2) {
        Ok(f) => {
            fig = viz.figure("Residuals of the quadratic fit")
                .describe("observed minus fitted response, one point per measurement")
                .labels("temperature / deg C", "residual")
                .scatter(temperature, f.residuals, label: "residuals")
                .hline(0.0, label: "zero")
            print(fig.svg())
        },
        Err(message) => print("error: " + message),
    }
}
`;

// ---------------------------------------------------------------- B: physical modelling

const PHYSICAL_TEXT = `# Physical modelling

Two dimensional models solved with the real RK4 solver of std.numeric and compared with their analytic solutions.

- Decay of a mass: dm/dt = -k m, with m0 = 2 kg and k = 0.35 / s.
- A damped oscillator: x'' + 2 zeta w x' + w^2 x = 0, with w = 2 / s, zeta = 0.1, x0 = 0.5 m, v0 = 0 m/s.

Every parameter is defined once, in c1, with its unit. To change a parameter, edit c1 and run the cells again: the
outputs that depended on it become stale until they are run. The unit of each parameter is checked by the compiler.`;

const PHYSICAL_PARAMETERS = `// The parameters of the model, each with its unit. The compiler checks every use against the unit.
pub fn initial_mass() -> Quantity<Mass> {
    2.0 kg
}

pub fn decay_rate() -> Quantity<Frequency> {
    0.35 / (1 s)
}

pub fn horizon() -> Quantity<Time> {
    12.0 s
}

pub fn oscillator_frequency() -> Quantity<Frequency> {
    2.0 / (1 s)
}

pub fn damping_ratio() -> Float {
    0.1
}

pub fn initial_position() -> Quantity<Length> {
    0.5 m
}

pub fn initial_velocity() -> Quantity<Length / Time> {
    0.0 m / (1 s)
}
`;

const PHYSICAL_DECAY = `import std.numeric
import c1

fn main() -> Void {
    m0 = c1.initial_mass() / (1 kg)
    rate = (c1.decay_rate() * (1 s)).value()
    t_end = c1.horizon() / (1 s)
    sol = numeric.rk4(fn(t, y) { array([0.0 - rate * y[0]]) }, array([m0]), 0.0, t_end, 1200)
    mass = sol.component(0)
    times = sol.t
    print("OSTRIN-TABLE Decay of the mass: RK4 against the analytic solution")
    print("time_s,mass_kg,analytic_mass_kg,error_kg")
    mut worst = 0.0
    for i in 0 until times.length() {
        exact = m0 * exp(0.0 - rate * times[i])
        error = abs(mass[i] - exact)
        if error > worst {
            worst = error
        }
        if i % 100 == 0 {
            print(times[i].to_string() + "," + mass[i].to_string() + "," + exact.to_string() + "," + error.to_string())
        }
    }
    print("OSTRIN-TABLE-END")
    print("largest error over the " + times.length().to_string() + " nodes: " + worst.to_string() + " kg")
}
`;

const PHYSICAL_DECAY_FIGURE = `import std.numeric
import std.viz
import c1

fn main() -> Void {
    m0 = c1.initial_mass() / (1 kg)
    rate = (c1.decay_rate() * (1 s)).value()
    t_end = c1.horizon() / (1 s)
    sol = numeric.rk4(fn(t, y) { array([0.0 - rate * y[0]]) }, array([m0]), 0.0, t_end, 1200)
    times = sol.t
    mut exact = zeros([times.length()])
    for i in 0 until times.length() {
        exact.set(i, m0 * exp(0.0 - rate * times[i]))
    }
    fig = viz.figure("First-order decay of a mass")
        .describe("m(t) = m0 exp(-k t) with the parameters of c1, RK4 with 1200 steps")
        .labels("time / s", "mass / kg")
        .line(times, sol.component(0), label: "RK4")
        .line(times, exact, label: "analytic", dash: "6 4")
    print(fig.svg())
}
`;

const PHYSICAL_OSCILLATOR = `import std.numeric
import c1

fn main() -> Void {
    w = (c1.oscillator_frequency() * (1 s)).value()
    zeta = c1.damping_ratio()
    x0 = c1.initial_position() / (1 m)
    v0 = c1.initial_velocity() / (1 m / (1 s))
    t_end = c1.horizon() / (1 s)
    wd = w * sqrt(1.0 - zeta * zeta)
    sol = numeric.rk4(fn(t, y) { array([y[1], 0.0 - 2.0 * zeta * w * y[1] - w * w * y[0]]) }, array([x0, v0]), 0.0, t_end, 2400)
    position = sol.component(0)
    times = sol.t
    print("OSTRIN-TABLE Damped oscillator: RK4 against the analytic solution")
    print("time_s,position_m,analytic_position_m,error_m")
    mut worst = 0.0
    for i in 0 until times.length() {
        tt = times[i]
        exact = exp(0.0 - zeta * w * tt) * (x0 * cos(wd * tt) + ((v0 + zeta * w * x0) / wd) * sin(wd * tt))
        error = abs(position[i] - exact)
        if error > worst {
            worst = error
        }
        if i % 200 == 0 {
            print(tt.to_string() + "," + position[i].to_string() + "," + exact.to_string() + "," + error.to_string())
        }
    }
    print("OSTRIN-TABLE-END")
    print("largest error over the " + times.length().to_string() + " nodes: " + worst.to_string() + " m")
}
`;

const PHYSICAL_OSCILLATOR_FIGURE = `import std.numeric
import std.viz
import c1

fn main() -> Void {
    w = (c1.oscillator_frequency() * (1 s)).value()
    zeta = c1.damping_ratio()
    x0 = c1.initial_position() / (1 m)
    v0 = c1.initial_velocity() / (1 m / (1 s))
    t_end = c1.horizon() / (1 s)
    wd = w * sqrt(1.0 - zeta * zeta)
    sol = numeric.rk4(fn(t, y) { array([y[1], 0.0 - 2.0 * zeta * w * y[1] - w * w * y[0]]) }, array([x0, v0]), 0.0, t_end, 2400)
    times = sol.t
    mut exact = zeros([times.length()])
    for i in 0 until times.length() {
        tt = times[i]
        exact.set(i, exp(0.0 - zeta * w * tt) * (x0 * cos(wd * tt) + ((v0 + zeta * w * x0) / wd) * sin(wd * tt)))
    }
    fig = viz.figure("Damped oscillator")
        .describe("x'' + 2 zeta w x' + w^2 x = 0 with the parameters of c1")
        .labels("time / s", "position / m")
        .line(times, sol.component(0), label: "RK4")
        .line(times, exact, label: "analytic", dash: "6 4")
        .hline(0.0)
    print(fig.svg())
}
`;

// ---------------------------------------------------------------- C: symbolic mathematics

const SYMBOLIC_TEXT = `# Symbolic mathematics

The expression f(x) = 3x^2 - 2x + 1 is built with std.symbolic. Its derivative, substitution and numeric values are
computed by the program. The roots and the factorization come from std.symbolic's solver, which handles the cases it
supports and reports the others. Each cell builds its own symbols: values do not pass between cells.`;

const SYMBOLIC_BUILD = `import std.symbolic

fn main() -> Void {
    s = symbolic.store()
    x = symbolic.symbol(s, "x")
    two = symbolic.number(s, 2)
    f = symbolic.add(symbolic.mul(symbolic.number(s, 3), symbolic.power(x, two)), symbolic.add(symbolic.mul(symbolic.number(s, 0 - 2), x), symbolic.number(s, 1)))
    print("f(x) = " + symbolic.show(f))
    print("expanded: " + symbolic.show(symbolic.expand(f)))
    print("f(2) by substitution: " + symbolic.show(symbolic.expand(symbolic.subs(f, x, two))))
    match symbolic.diff(f, x) {
        Ok(df) => {
            print("f'(x) = " + symbolic.show(df))
            print("OSTRIN-TABLE f and its derivative at sample points")
            print("x,f,f_prime,f_check")
            for i in 0 until 9 {
                xv = (i as Float) * 0.5
                names = ["x"]
                fv = symbolic.eval_or(f, names, [xv], 0.0 / 0.0)
                dv = symbolic.eval_or(df, names, [xv], 0.0 / 0.0)
                print(xv.to_string() + "," + fv.to_string() + "," + dv.to_string() + "," + (3.0 * xv * xv - 2.0 * xv + 1.0).to_string())
            }
            print("OSTRIN-TABLE-END")
        },
        Err(message) => print("error: " + message),
    }
}
`;

const SYMBOLIC_SOLVE = `import std.symbolic

fn main() -> Void {
    s = symbolic.store()
    x = symbolic.symbol(s, "x")
    two = symbolic.number(s, 2)
    p = symbolic.add(symbolic.power(x, two), symbolic.add(symbolic.mul(symbolic.number(s, 0 - 3), x), symbolic.number(s, 2)))
    print("p(x) = " + symbolic.show(p))
    print("roots of p: " + symbolic.roots_text(p, x))
    match symbolic.factor_rational(p, x) {
        Ok(f) => print("factorization: " + symbolic.show(f)),
        Err(message) => print("factorization not available: " + message),
    }
    q = symbolic.add(symbolic.power(x, two), symbolic.number(s, 1))
    print("roots of x^2 + 1: " + symbolic.roots_text(q, x))
    print("the roots of 2x^2 + 3x - 2 by the same solver: " + symbolic.roots_text(symbolic.add(symbolic.mul(symbolic.number(s, 2), symbolic.power(x, two)), symbolic.add(symbolic.mul(symbolic.number(s, 3), x), symbolic.number(s, 0 - 2))), x))
}
`;

const SYMBOLIC_FIGURE = `import std.symbolic
import std.viz

fn main() -> Void {
    s = symbolic.store()
    x = symbolic.symbol(s, "x")
    two = symbolic.number(s, 2)
    f = symbolic.add(symbolic.mul(symbolic.number(s, 3), symbolic.power(x, two)), symbolic.add(symbolic.mul(symbolic.number(s, 0 - 2), x), symbolic.number(s, 1)))
    match symbolic.diff(f, x) {
        Ok(df) => {
            grid = linspace(0.0 - 2.0, 3.0, 101)
            mut fv = zeros([101])
            mut dv = zeros([101])
            names = ["x"]
            for i in 0 until 101 {
                fv.set(i, symbolic.eval_or(f, names, [grid[i]], 0.0 / 0.0))
                dv.set(i, symbolic.eval_or(df, names, [grid[i]], 0.0 / 0.0))
            }
            fig = viz.figure("f(x) = 3x^2 - 2x + 1 and its derivative")
                .describe("values computed by evaluating the symbolic expressions on a grid")
                .labels("x", "value")
                .line(grid, fv, label: "f(x)")
                .line(grid, dv, label: "f'(x)", dash: "6 4")
                .hline(0.0)
            print(fig.svg())
        },
        Err(message) => print("error: " + message),
    }
}
`;

// ---------------------------------------------------------------- assembly

// ---------------------------------------------------------------- D: symbolic to numeric

const NUMERIC_TEXT = `# Symbolic to numeric

The polynomial f(x) = a x^3 - b x + c is built with std.symbolic from the parameters in c1. Its derivative is computed
symbolically, sampled on an interval, compared with central differences, checked for counterexamples, and plotted.

Change a value in c1 and run it: the cells that depend on c1 become stale. Run them again and every table, check and
figure shows the new values. Each code cell builds its own expression, so no value passes between cells except
through c1.`;

const NUMERIC_PARAMETERS = `// The parameters of f(x) = a x^3 - b x + c. Each function returns one integer coefficient; the cells that depend on
// this cell read them. Change a value, then run the cells again.
pub fn coefficient_a() -> Int {
    return 2
}

pub fn coefficient_b() -> Int {
    return 3
}

pub fn coefficient_c() -> Int {
    return 1
}`;

const NUMERIC_TRANSFORM = `import std.symbolic
import c1

fn main() -> Void {
    s = symbolic.store()
    x = symbolic.symbol(s, "x")
    three = symbolic.number(s, 3)
    f = symbolic.add(symbolic.mul(symbolic.number(s, c1.coefficient_a()), symbolic.power(x, three)), symbolic.add(symbolic.mul(symbolic.number(s, 0 - c1.coefficient_b()), x), symbolic.number(s, c1.coefficient_c())))
    print("f(x) = " + symbolic.show(f))
    print("expanded: " + symbolic.show(symbolic.expand(f)))
    match symbolic.diff(f, x) {
        Ok(df) => print("f'(x) = " + symbolic.show(df)),
        Err(message) => print("derivative not available: " + message),
    }
    print("f(1/2) exactly: " + symbolic.show(symbolic.expand(symbolic.subs(f, x, symbolic.number(s, 1, 2)))))
}`;

const NUMERIC_TABLE = `import std.symbolic
import c1

fn main() -> Void {
    s = symbolic.store()
    x = symbolic.symbol(s, "x")
    three = symbolic.number(s, 3)
    f = symbolic.add(symbolic.mul(symbolic.number(s, c1.coefficient_a()), symbolic.power(x, three)), symbolic.add(symbolic.mul(symbolic.number(s, 0 - c1.coefficient_b()), x), symbolic.number(s, c1.coefficient_c())))
    match symbolic.diff(f, x) {
        Ok(df) => {
            xs = linspace(0.0 - 2.0, 2.0, 9)
            h = 0.000001
            mut up = zeros([9])
            mut down = zeros([9])
            for i in 0 until 9 {
                up.set(i, xs[i] + h)
                down.set(i, xs[i] - h)
            }
            values = symbolic.sample(f, x, xs)
            slopes = symbolic.sample(df, x, xs)
            ahead = symbolic.sample(f, x, up)
            behind = symbolic.sample(f, x, down)
            match values {
                Ok(fv) => {
                    match slopes {
                        Ok(dv) => {
                            print("OSTRIN-TABLE f and its derivative on [-2, 2]")
                            print("x,f(x),f'(x),central difference")
                            match ahead {
                                Ok(uv) => {
                                    match behind {
                                        Ok(bv) => {
                                            for i in 0 until 9 {
                                                quotient = (uv[i] - bv[i]) / (2.0 * h)
                                                print(xs[i].to_string() + "," + fv[i].to_string() + "," + dv[i].to_string() + "," + quotient.to_string())
                                            }
                                        },
                                        Err(message) => print("sample: " + message),
                                    }
                                },
                                Err(message) => print("sample: " + message),
                            }
                            print("OSTRIN-TABLE-END")
                        },
                        Err(message) => print("sample: " + message),
                    }
                },
                Err(message) => print("sample: " + message),
            }
        },
        Err(message) => print("derivative not available: " + message),
    }
}`;

const NUMERIC_CHECKS = `import std.symbolic
import c1

fn main() -> Void {
    s = symbolic.store()
    x = symbolic.symbol(s, "x")
    three = symbolic.number(s, 3)
    f = symbolic.add(symbolic.mul(symbolic.number(s, c1.coefficient_a()), symbolic.power(x, three)), symbolic.add(symbolic.mul(symbolic.number(s, 0 - c1.coefficient_b()), x), symbolic.number(s, c1.coefficient_c())))
    xs = linspace(0.0 - 2.0, 2.0, 41)
    match symbolic.derivative_mismatches(f, x, xs, 0.000001, 0.000001) {
        Ok(points) => print("derivative against central differences: " + points.length().to_string() + " mismatched points of 41"),
        Err(message) => print("derivative check refused: " + message),
    }
    match symbolic.derivative_mismatches(f, x, xs, 0.0000000000000001, 0.000001) {
        Ok(points) => print("a step of 1e-16 gave " + points.length().to_string() + " mismatches"),
        Err(message) => print("a step of 1e-16 is refused, as it should be: " + message),
    }
    // The identity (x + 1)^3 = x^3 + 3x^2 + 3x + 1 holds, so it has no counterexample on the grid.
    x_plus = symbolic.add(x, symbolic.number(s, 1))
    cube = symbolic.power(x_plus, three)
    expanded = symbolic.add(symbolic.power(x, three), symbolic.add(symbolic.mul(symbolic.number(s, 3), symbolic.power(x, symbolic.number(s, 2))), symbolic.add(symbolic.mul(symbolic.number(s, 3), x), symbolic.number(s, 1))))
    match symbolic.disagreements(cube, expanded, x, xs, 0.000000000001) {
        Ok(points) => print("(x + 1)^3 = x^3 + 3x^2 + 3x + 1: " + points.length().to_string() + " counterexamples on the grid"),
        Err(message) => print("identity check refused: " + message),
    }
}`;

const NUMERIC_FIGURE = `import std.symbolic
import c1
import std.viz

fn main() -> Void {
    s = symbolic.store()
    x = symbolic.symbol(s, "x")
    three = symbolic.number(s, 3)
    f = symbolic.add(symbolic.mul(symbolic.number(s, c1.coefficient_a()), symbolic.power(x, three)), symbolic.add(symbolic.mul(symbolic.number(s, 0 - c1.coefficient_b()), x), symbolic.number(s, c1.coefficient_c())))
    xs = linspace(0.0 - 2.0, 2.0, 81)
    match symbolic.diff(f, x) {
        Ok(df) => {
            match symbolic.sample(f, x, xs) {
                Ok(fv) => {
                    match symbolic.sample(df, x, xs) {
                        Ok(dv) => {
                            fig = viz.figure("f and its derivative, from the symbolic expressions")
                                .describe("f(x) = a x^3 - b x + c with the parameters of c1; the derivative is computed symbolically")
                                .labels("x", "value")
                                .line(xs, fv, label: "f(x)")
                                .line(xs, dv, label: "f'(x)", dash: "6 4")
                                .hline(0.0)
                            print(fig.svg())
                        },
                        Err(message) => print("sample: " + message),
                    }
                },
                Err(message) => print("sample: " + message),
            }
        },
        Err(message) => print("derivative not available: " + message),
    }
}`;

// ---------------------------------------------------------------- E: assumptions and conditions

const CONDITIONAL_TEXT = `# Assumptions and conditions

The same expressions are simplified under different contexts. A context is a value: each row below is computed with a
context built from the one before it, and the expression itself is never changed. Each result says what it is valid for:
the conditions it needs, whether the context proves them, and why the rule applies or is refused.

Run the cells in order. Cell c1 builds the table of results; c2 checks them against values computed without the rules,
using the numeric bridge; c3 draws sqrt(x^2) under the contexts. Changing a context in c1 changes the table after a run.`;

const CONDITIONAL_TABLE = `import std.symbolic

fn main() -> Void {
    s = symbolic.store()
    x = symbolic.symbol(s, "x")
    two = symbolic.number(s, 2)
    numerator = symbolic.add(symbolic.power(x, two), symbolic.number(s, 0 - 1))
    denominator = symbolic.sub(x, symbolic.number(s, 1))
    none = symbolic.context()
    positive = symbolic.assume(none, "x", "positive").unwrap()
    nonnegative = symbolic.assume(none, "x", "nonnegative").unwrap()
    nonpositive = symbolic.assume(none, "x", "nonpositive").unwrap()
    complex = symbolic.assume(none, "x", "complex").unwrap()
    contexts = [none, positive, nonnegative, nonpositive, complex]
    names = ["no assumptions", "x positive", "x nonnegative", "x nonpositive", "x complex"]
    print("OSTRIN-TABLE The same expressions under five contexts")
    print("rule,context,result,status,conditions")
    for i in 0 until 5 {
        sq = symbolic.sqrt_of_square(contexts[i], x)
        mut conditions = ""
        for c in sq.required {
            conditions = conditions + c + "; "
        }
        print("sqrt(x^2)," + names[i] + "," + sq.transformed + "," + sq.status + "," + conditions)
    }
    for i in 0 until 5 {
        cq = symbolic.simplify_quotient(contexts[i], numerator, denominator, x)
        mut conditions = ""
        for c in cq.required {
            conditions = conditions + c + "; "
        }
        print("(x^2 - 1)/(x - 1)," + names[i] + "," + cq.transformed + "," + cq.status + "," + conditions)
    }
    for i in 0 until 5 {
        sx = symbolic.simplify_self_quotient(contexts[i], x)
        mut conditions = ""
        for c in sx.required {
            conditions = conditions + c + "; "
        }
        print("x / x," + names[i] + "," + sx.transformed + "," + sx.status + "," + conditions)
    }
    for i in 0 until 5 {
        lg = symbolic.expand_log_product(contexts[i], x, x)
        mut conditions = ""
        for c in lg.required {
            conditions = conditions + c + "; "
        }
        print("ln(x x)," + names[i] + "," + lg.transformed + "," + lg.status + "," + conditions)
    }
    print("OSTRIN-TABLE-END")
}`;

const CONDITIONAL_CHECKS = `import std.symbolic

fn main() -> Void {
    s = symbolic.store()
    x = symbolic.symbol(s, "x")
    two = symbolic.number(s, 2)
    grid = linspace(0.0 - 3.0, 3.0, 41)
    mut reference = zeros([41])
    for i in 0 until 41 {
        reference.set(i, sqrt(grid[i] * grid[i]))
    }
    none = symbolic.context()
    nonnegative = symbolic.assume(none, "x", "nonnegative").unwrap()
    real_rule = symbolic.sqrt_of_square(none, x)
    nonneg_rule = symbolic.sqrt_of_square(nonnegative, x)
    match symbolic.sample(real_rule.value, x, grid) {
        Ok(values) => {
            mut worst = 0.0
            for i in 0 until 41 {
                mut d = values[i] - reference[i]
                if d < 0.0 {
                    d = 0.0 - d
                }
                if d > worst {
                    worst = d
                }
            }
            print("abs(x) against sqrt(x*x) on the grid: largest difference " + worst.to_string())
        },
        Err(message) => print("sample: " + message),
    }
    match symbolic.sample(nonneg_rule.value, x, grid) {
        Ok(values) => {
            mut counterexamples = 0
            for i in 0 until 41 {
                mut d = values[i] - reference[i]
                if d < 0.0 {
                    d = 0.0 - d
                }
                if d > 0.000000000001 {
                    counterexamples = counterexamples + 1
                }
            }
            print("x against sqrt(x*x) under x >= 0: " + counterexamples.to_string() + " counterexamples on the grid, the negative points")
        },
        Err(message) => print("sample: " + message),
    }
    numerator = symbolic.add(symbolic.power(x, two), symbolic.number(s, 0 - 1))
    denominator = symbolic.sub(x, symbolic.number(s, 1))
    cancelled = symbolic.simplify_quotient(none, numerator, denominator, x)
    match symbolic.sample(symbolic.div(numerator, denominator), x, linspace(1.0, 1.0, 1)) {
        Ok(values) => print("the original at x = 1 has the value " + values[0].to_string()),
        Err(message) => print("the original at x = 1 is undefined: " + message),
    }
    match symbolic.sample(cancelled.value, x, linspace(1.0, 1.0, 1)) {
        Ok(values) => print("the cancelled form 1 + x at x = 1 is " + values[0].to_string() + ": the point x = 1 is the one the condition keeps"),
        Err(message) => print("the cancelled form: " + message),
    }
}`;

const CONDITIONAL_FIGURE = `import std.symbolic
import std.viz

fn main() -> Void {
    s = symbolic.store()
    x = symbolic.symbol(s, "x")
    grid = linspace(0.0 - 3.0, 3.0, 61)
    mut reference = zeros([61])
    for i in 0 until 61 {
        reference.set(i, sqrt(grid[i] * grid[i]))
    }
    none = symbolic.context()
    nonnegative = symbolic.assume(none, "x", "nonnegative").unwrap()
    real_rule = symbolic.sqrt_of_square(none, x)
    nonneg_rule = symbolic.sqrt_of_square(nonnegative, x)
    match symbolic.sample(real_rule.value, x, grid) {
        Ok(real_values) => {
            match symbolic.sample(nonneg_rule.value, x, grid) {
                Ok(nonneg_values) => {
                    fig = viz.figure("sqrt(x^2) under two contexts")
                        .describe("sqrt(x^2) is abs(x) for real x; under x >= 0 the rule gives x, which differs for x < 0")
                        .labels("x", "value")
                        .line(grid, reference, label: "sqrt(x*x)")
                        .line(grid, real_values, label: "abs(x), real x", dash: "6 4")
                        .line(grid, nonneg_values, label: "x, under x >= 0")
                    print(fig.svg())
                },
                Err(message) => print("sample: " + message),
            }
        },
        Err(message) => print("sample: " + message),
    }
}`;

const CONDITIONAL_CHAIN = `import std.symbolic

fn main() -> Void {
    s = symbolic.store()
    x = symbolic.symbol(s, "x")
    two = symbolic.number(s, 2)
    numerator = symbolic.add(symbolic.power(x, two), symbolic.number(s, 0 - 1))
    denominator = symbolic.sub(x, symbolic.number(s, 1))
    quotient = symbolic.div(numerator, denominator)
    root = symbolic.power(symbolic.power(x, two), symbolic.number(s, 1, 2))
    whole = symbolic.add(quotient, root)
    none = symbolic.context()
    positive = symbolic.assume(none, "x", "positive").unwrap()
    contexts = [none, positive]
    names = ["no assumptions", "x positive"]
    // The points at which the derivation and the original are compared. Edit them and run the cell again.
    points = [2.0, 0.0 - 2.0, 1.0]
    mut rows: List<String> = []
    for k in 0 until 2 {
        ctx = contexts[k]
        first = symbolic.chain_then(symbolic.chain_start(whole, ctx), ctx, quotient, symbolic.simplify_quotient(ctx, numerator, denominator, x))
        second = symbolic.chain_then(first, ctx, root, symbolic.sqrt_of_square(ctx, x))
        mut pending = ""
        for u in second.unknown {
            pending = pending + u + "; "
        }
        print("derivation under " + names[k] + ": " + second.status + ", steps " + second.steps.length().to_string() + ", pending " + pending)
        for p in 0 until points.length() {
            value = [points[p]]
            mut original_text = ""
            match symbolic.eval(whole, ["x"], value) {
                Ok(v) => {
                    original_text = v.to_string()
                },
                Err(message) => {
                    original_text = "undefined"
                },
            }
            mut derived_text = ""
            match symbolic.chain_evaluate(second, ["x"], value) {
                Ok(v) => {
                    derived_text = v.to_string()
                },
                Err(message) => {
                    derived_text = "refused: " + message
                },
            }
            rows.push(names[k] + "," + points[p].to_string() + "," + original_text + "," + derived_text)
        }
    }
    print("OSTRIN-TABLE E = (x^2 - 1)/(x - 1) + sqrt(x^2), derived step by step and compared with the original")
    print("context,x,original,derivation")
    for row in rows {
        print(row)
    }
    print("OSTRIN-TABLE-END")
}`;

function build({ id, title, text, cells, resources = [], closing = null }) {
  let doc = createNotebook({ id, title, now: NOW });
  doc = addCell(doc, "text", { source: text });
  for (const cell of cells) {
    doc = addCell(doc, cell.kind === "text" ? "text" : "code", { source: cell.source });
    const added = doc.cells[doc.cells.length - 1];
    if (cell.deps) doc = setDeps(doc, added.id, cell.deps);
  }
  if (closing) doc = addCell(doc, "text", { source: closing });
  for (const resource of resources) doc = attachResource(doc, resource.path, resource.text);
  const problems = validateNotebook(doc);
  if (problems.length > 0) throw new Error(`notebook ${id} is invalid: ${problems.join("; ")}`);
  return doc;
}

export function notebooks() {
  const calibration = readFileSync(join(root, "examples", "data", "calibration.csv"), "utf8");
  return {
    experimental: build({
      id: "experimental",
      title: "A · Experimental data analysis",
      text: EXPERIMENT_TEXT,
      resources: [{ path: "data/calibration.csv", text: calibration }],
      cells: [
        { source: EXPERIMENT_DEFINITIONS },
        { source: EXPERIMENT_STATISTICS, deps: ["c1"] },
        { source: EXPERIMENT_FIT_FIGURE, deps: ["c1"] },
        { source: EXPERIMENT_RESIDUAL_FIGURE, deps: ["c1"] },
      ],
    }),
    physical: build({
      id: "physical",
      title: "B · Physical modelling",
      text: PHYSICAL_TEXT,
      cells: [
        { source: PHYSICAL_PARAMETERS },
        { source: PHYSICAL_DECAY, deps: ["c1"] },
        { source: PHYSICAL_DECAY_FIGURE, deps: ["c1"] },
        { source: PHYSICAL_OSCILLATOR, deps: ["c1"] },
        { source: PHYSICAL_OSCILLATOR_FIGURE, deps: ["c1"] },
      ],
    }),
    rational: build({
      id: "rational",
      title: "F · Exact algebra and rational equations",
      text: RATIONAL_TEXT,
      closing: RATIONAL_INTERPRETATION,
      cells: [
        { source: RATIONAL_PARAMETERS },
        { source: RATIONAL_POLYNOMIALS },
        { source: RATIONAL_DIVISION },
        { source: RATIONAL_CONSTRUCTION },
        { source: RATIONAL_DOMAIN },
        { source: RATIONAL_SOLVE, deps: ["c1"] },
        { source: RATIONAL_SOLVE_EXCLUDED, deps: ["c1"] },
        { source: RATIONAL_IDENTITY },
        { source: RATIONAL_PARTIAL },
        { source: RATIONAL_VERIFY, deps: ["c1"] },
        { source: RATIONAL_TABLE },
        { source: RATIONAL_FIGURE },
      ],
    }),
    symbolic: build({
      id: "symbolic",
      title: "C · Symbolic mathematics",
      text: SYMBOLIC_TEXT,
      cells: [{ source: SYMBOLIC_BUILD }, { source: SYMBOLIC_SOLVE }, { source: SYMBOLIC_FIGURE }],
    }),
    conditional: build({
      id: "conditional",
      title: "E · Assumptions and conditions",
      text: CONDITIONAL_TEXT,
      cells: [{ source: CONDITIONAL_TABLE }, { source: CONDITIONAL_CHECKS }, { source: CONDITIONAL_FIGURE }, { source: CONDITIONAL_CHAIN }],
    }),
    numeric: build({
      id: "numeric",
      title: "D · Symbolic to numeric",
      text: NUMERIC_TEXT,
      cells: [
        { source: NUMERIC_PARAMETERS },
        { source: NUMERIC_TRANSFORM, deps: ["c1"] },
        { source: NUMERIC_TABLE, deps: ["c1"] },
        { source: NUMERIC_CHECKS, deps: ["c1"] },
        { source: NUMERIC_FIGURE, deps: ["c1"] },
      ],
    }),
  };
}

const FILES = {
  experimental: "experimental.ostrin.notebook.json",
  physical: "physical.ostrin.notebook.json",
  symbolic: "symbolic.ostrin.notebook.json",
  numeric: "numeric.ostrin.notebook.json",
  rational: "rational.ostrin.notebook.json",
  conditional: "conditional.ostrin.notebook.json",
};

if (import.meta.url === `file://${process.argv[1].replace(/\\/g, "/")}` || process.argv[1]?.endsWith("notebook-examples.mjs")) {
  const docs = notebooks();
  if (process.argv.includes("--write")) {
    for (const [name, doc] of Object.entries(docs)) {
      writeFileSync(join(outDir, FILES[name]), serializeNotebook(doc));
      console.log(`wrote website/notebooks/${FILES[name]} (${doc.cells.length} cells)`);
    }
  } else if (process.argv.includes("--check")) {
    let stale = false;
    for (const [name, doc] of Object.entries(docs)) {
      const committed = readFileSync(join(outDir, FILES[name]), "utf8");
      if (committed !== serializeNotebook(doc)) {
        console.error(`stale: website/notebooks/${FILES[name]}`);
        stale = true;
      }
    }
    if (stale) process.exit(1);
    console.log("notebook examples: ok");
  } else {
    console.log(Object.keys(docs).join("\n"));
  }
}
