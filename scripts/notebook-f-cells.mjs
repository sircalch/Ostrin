// Cell sources of notebook F, "exact algebra and rational equations" (website/notebooks/rational.ostrin.notebook.json).
// Imported by scripts/notebook-examples.mjs. Every number the notebook shows is printed by one of these programs when the
// cell runs; nothing here carries a result.

export const RATIONAL_TEXT = `# Exact algebra and rational equations

This notebook works in exact arithmetic. Polynomials and rational functions are built from exact coefficient texts, and
every number printed by a cell comes from an Ostrin program running in the page.

- f(x) = (x^2 - 1)/(x - 1) equals x + 1 wherever it is defined. x = 1 is not in its domain, and the domain keeps that
  condition: a simplification does not remove it.
- An equation R1(x) = R2(x) is solved on the domain of both sides. Each solution is checked against the original
  expressions, and each excluded point is listed with the requirement that excludes it.
- "partial" means that part of the equation is outside the exact methods of the library. It is not a complete solution.

The parameters are in c1. Change a value there and run the cells that import c1: their outputs become stale and must be
run again.`;

export const RATIONAL_PARAMETERS = `// The parameters of the equations. Change a value, then run the cells that import c1 again.
pub fn rhs_valid() -> String {
    return "3"
}

pub fn rhs_excluded() -> String {
    return "2"
}
`;

export const RATIONAL_POLYNOMIALS = `import std.polynomial
import std.mpolynomial

fn main() -> Void {
    p = polynomial.poly_from_texts(["-1", "0", "1"]).unwrap()
    q = polynomial.poly_from_texts(["-1", "1"]).unwrap()
    print("p, coefficients from the lowest degree: " + polynomial.poly_to_string(p) + "   degree " + polynomial.poly_degree(p).to_string())
    print("q, coefficients from the lowest degree: " + polynomial.poly_to_string(q) + "   degree " + polynomial.poly_degree(q).to_string())
    big = mpolynomial.mpoly_parse(["x", "y"], ["1|2,1", "1|1,2", "1|0,2"]).unwrap()
    print("F(x, y) = " + mpolynomial.mpoly_to_text(big))
    print("total degree of F: " + mpolynomial.mpoly_total_degree(big).to_string())
}
`;

export const RATIONAL_DIVISION = `import std.mpolynomial

fn main() -> Void {
    big = mpolynomial.mpoly_parse(["x", "y"], ["1|2,1", "1|1,2", "1|0,2"]).unwrap()
    g1 = mpolynomial.mpoly_parse(["x", "y"], ["1|1,1", "-1|0,0"]).unwrap()
    g2 = mpolynomial.mpoly_parse(["x", "y"], ["1|0,2", "-1|0,0"]).unwrap()
    divisors = [g1, g2]
    print("F = " + mpolynomial.mpoly_to_text(big))
    print("divisors, in this order (lexicographic, x > y): g1 = " + mpolynomial.mpoly_to_text(g1) + " ; g2 = " + mpolynomial.mpoly_to_text(g2))
    match mpolynomial.mpoly_divide(big, divisors) {
        Ok(d) => {
            print("q1 = " + mpolynomial.mpoly_to_text(d.quotients[0]))
            print("q2 = " + mpolynomial.mpoly_to_text(d.quotients[1]))
            print("r = " + mpolynomial.mpoly_to_text(d.remainder))
            match mpolynomial.mpoly_division_holds(big, divisors, d) {
                Ok(holds) => print("F = q1 g1 + q2 g2 + r holds exactly: " + holds.to_string()),
                Err(message) => print("check refused: " + message),
            }
        },
        Err(message) => print("division refused: " + message),
    }
    print("a zero remainder with several divisors is not, by itself, a test of membership in the ideal they generate")
}
`;

export const RATIONAL_CONSTRUCTION = `import std.rational

fn main() -> Void {
    f = rational.rational_new(["-1", "0", "1"], ["-1", "1"]).unwrap()
    print("f(x) = (x^2 - 1)/(x - 1), as entered")
    print("numerator in lowest terms (coefficient|degree): " + rational.rational_numerator_lines(f).join(" | "))
    print("denominator in lowest terms (coefficient|degree): " + rational.rational_denominator_lines(f).join(" | "))
    print("domain requirements kept: " + rational.rational_requirement_texts(f).join(" ; "))
}
`;

export const RATIONAL_DOMAIN = `import std.rational

fn main() -> Void {
    f = rational.rational_new(["-1", "0", "1"], ["-1", "1"]).unwrap()
    print("requirements that stay with the simplified form: " + rational.rational_requirement_texts(f).join(" and "))
    match rational.rational_evaluate(f, "2") {
        Ok(v) => print("f(2) = " + v),
        Err(message) => print("f(2): " + message),
    }
    match rational.rational_evaluate(f, "1") {
        Ok(v) => print("f(1) = " + v),
        Err(message) => print("f(1) is not evaluated: " + message),
    }
}
`;

export const RATIONAL_SOLVE = `import std.rational
import c1

fn main() -> Void {
    f = rational.rational_new(["-1", "0", "1"], ["-1", "1"]).unwrap()
    g = rational.rational_new([c1.rhs_valid()], ["1"]).unwrap()
    s = rational.rational_solve(f, g).unwrap()
    print("(x^2 - 1)/(x - 1) = " + c1.rhs_valid() + ": " + s.status)
    print("  solutions: " + s.solutions.join(" | "))
    print("  excluded: " + s.excluded.join(" | "))
    print("  pending: " + s.pending.join(" | "))
    print("  completeness: " + s.completeness)
}
`;

export const RATIONAL_SOLVE_EXCLUDED = `import std.rational
import c1

fn main() -> Void {
    f = rational.rational_new(["-1", "0", "1"], ["-1", "1"]).unwrap()
    g = rational.rational_new([c1.rhs_excluded()], ["1"]).unwrap()
    s = rational.rational_solve(f, g).unwrap()
    print("(x^2 - 1)/(x - 1) = " + c1.rhs_excluded() + ": " + s.status)
    print("  solutions: " + s.solutions.join(" | "))
    print("  rejected: " + s.rejected.join(" | "))
    print("  excluded: " + s.excluded.join(" | "))
    print("  completeness: " + s.completeness)
}
`;

export const RATIONAL_IDENTITY = `import std.rational

fn main() -> Void {
    f = rational.rational_new(["-2", "0", "1"], ["-2", "0", "1"]).unwrap()
    g = rational.rational_new(["1"], ["1"]).unwrap()
    s = rational.rational_solve(f, g).unwrap()
    print("(x^2 - 2)/(x^2 - 2) = 1: " + s.status)
    print("  domain: " + s.domain.join(" ; "))
    print("  excluded (not enumerated when irrational): " + s.excluded.join(" | "))
    print("  completeness: " + s.completeness)
}
`;

export const RATIONAL_PARTIAL = `import std.rational

fn main() -> Void {
    f = rational.rational_new(["1", "1", "0", "1"], ["1"]).unwrap()
    g = rational.rational_new([], ["1"]).unwrap()
    s = rational.rational_solve(f, g).unwrap()
    print("x^3 + x + 1 = 0: " + s.status)
    print("  solutions: " + s.solutions.join(" | "))
    print("  pending: " + s.pending.join(" | "))
    print("  completeness: " + s.completeness)
}
`;

export const RATIONAL_VERIFY = `import std.rational
import c1

fn main() -> Void {
    f = rational.rational_new(["-1", "0", "1"], ["-1", "1"]).unwrap()
    g = rational.rational_new([c1.rhs_valid()], ["1"]).unwrap()
    s = rational.rational_solve(f, g).unwrap()
    for root in s.solutions {
        lhs = rational.rational_evaluate(f, root).unwrap()
        rhs = rational.rational_evaluate(g, root).unwrap()
        print("x = " + root + ": f = " + lhs + ", g = " + rhs + ", equal: " + (lhs == rhs).to_string())
    }
    match rational.rational_evaluate(f, "1") {
        Ok(v) => print("x = 1: f = " + v),
        Err(message) => print("x = 1 is excluded and not evaluated: " + message),
    }
    print("exclusions listed by the solver: " + s.excluded.join(" | "))
}
`;

export const RATIONAL_TABLE = `import std.rational

fn main() -> Void {
    f = rational.rational_new(["-1", "0", "1"], ["-1", "1"]).unwrap()
    print("OSTRIN-TABLE f(x) = (x^2 - 1)/(x - 1) at exact sample points")
    print("x,f(x),status")
    for i in 0 until 13 {
        k = i - 4
        mut text = k.to_string() + "/2"
        if k % 2 == 0 {
            text = (k / 2).to_string()
        }
        match rational.rational_evaluate(f, text) {
            Ok(v) => print(text + "," + v + ",defined"),
            Err(message) => print(text + ",,excluded"),
        }
    }
    print("OSTRIN-TABLE-END")
}
`;

export const RATIONAL_FIGURE = `import std.rational
import std.viz
import std.arrays
import std.strings

fn value_of(text: String) -> Float {
    parts = strings.split(text, "/")
    if parts.length() == 1 {
        return (parse_int(parts[0]).unwrap()) as Float
    }
    return (parse_int(parts[0]).unwrap() as Float) / (parse_int(parts[1]).unwrap() as Float)
}

fn main() -> Void {
    f = rational.rational_new(["-1", "0", "1"], ["-1", "1"]).unwrap()
    g = rational.rational_new(["1"], ["-1", "1"]).unwrap()
    n = 41
    mut xs = zeros([n])
    mut yf = zeros([n])
    mut yg = zeros([n])
    mut okf = zeros([n])
    mut okg = zeros([n])
    for i in 0 until n {
        xs.set(i, (i as Float) / 4.0 - 5.0)
        text = (i - 20).to_string() + "/4"
        match rational.rational_evaluate(f, text) {
            Ok(v) => {
                yf.set(i, value_of(v))
                okf.set(i, 1.0)
            },
            Err(message) => {
                yf.set(i, 0.0)
            },
        }
        match rational.rational_evaluate(g, text) {
            Ok(v) => {
                yg.set(i, value_of(v))
                okg.set(i, 1.0)
            },
            Err(message) => {
                yg.set(i, 0.0)
            },
        }
    }
    mut fig = viz.figure("f(x) = (x^2 - 1)/(x - 1) and g(x) = 1/(x - 1): no curve crosses an excluded point")
    fig = fig.labels("x", "value")
    mut start = 0
    mut inside = false
    mut first = true
    for i in 0 until n + 1 {
        mut defined = false
        if i < n {
            defined = okf[i] > 0.5
        }
        if defined and not inside {
            start = i
            inside = true
        }
        if inside and not defined {
            mut seg: List<Int> = []
            for j in start until i {
                seg.push(j)
            }
            mut label = ""
            if first {
                label = "f(x) = (x^2 - 1)/(x - 1), removable at x = 1"
                first = false
            }
            fig = fig.line(arrays.take(xs, seg).unwrap(), arrays.take(yf, seg).unwrap(), label: label)
            inside = false
        }
    }
    mut start_g = 0
    mut inside_g = false
    mut first_g = true
    for i in 0 until n + 1 {
        mut defined_g = false
        if i < n {
            defined_g = okg[i] > 0.5
        }
        if defined_g and not inside_g {
            start_g = i
            inside_g = true
        }
        if inside_g and not defined_g {
            mut seg_g: List<Int> = []
            for j in start_g until i {
                seg_g.push(j)
            }
            mut label_g = ""
            if first_g {
                label_g = "g(x) = 1/(x - 1), a pole at x = 1"
                first_g = false
            }
            fig = fig.line(arrays.take(xs, seg_g).unwrap(), arrays.take(yg, seg_g).unwrap(), label: label_g, dash: "6 4")
            inside_g = false
        }
    }
    mut marker_x = zeros([1])
    mut marker_y = zeros([1])
    marker_x.set(0, 1.0)
    fig = fig.scatter(marker_x, marker_y, label: "x = 1: excluded, no value is plotted")
    print(fig.svg())
}
`;

export const RATIONAL_INTERPRETATION = `# Interpretation

- The division is exact: F = q1 g1 + q2 g2 + r, and the program checks it. A zero remainder would not prove that F is in the
  ideal generated by g1 and g2 unless the divisors form a Gröbner basis; this notebook does not compute one.
- (x^2 - 1)/(x - 1) simplifies to x + 1, but x = 1 stays excluded. (x^2 - 1)/(x - 1) = 3 has the valid solution x = 2.
  (x^2 - 1)/(x - 1) = 2 has the candidate x = 1, which is excluded: it is rejected, not a solution.
- (x^2 - 2)/(x^2 - 2) = 1 is an identity only on the domain x^2 - 2 != 0. The points ±√2 are excluded. The list of excluded
  points is not exhaustive for requirements of degree above two, and the solver says so.
- x^3 + x + 1 = 0 has no rational root and the method has no exact algorithm for it: its status is partial, not solved.
- The figure never joins two defined values across an excluded point, and the excluded point is marked without a value.
`;
