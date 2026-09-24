//! Regression, linear solves, histograms and the normal distribution on
//! `Array<Float>`: `linfit polyfit polyval solve histogram norm_pdf norm_cdf`.
//! Every loop has a fixed order and uses no fused operations, so the native
//! backend (`array_linalg.c`) reproduces each result bit for bit.

use super::{array, detmath, RuntimeError, Value};

type Res<T> = Result<T, RuntimeError>;

fn fail<T>(message: impl Into<String>) -> Res<T> {
    Err(RuntimeError::Error(message.into()))
}

pub fn is_regress(name: &str, arity: usize) -> bool {
    matches!(
        (name, arity),
        ("linfit", 2)
            | ("polyfit", 3)
            | ("polyval", 2)
            | ("solve", 2)
            | ("norm", 1)
            | ("eigvals", 1)
            | ("det", 1)
            | ("inv", 1)
            | ("trace", 1)
            | ("eye", 1)
            | ("histogram", 4)
            | ("norm_pdf", 3)
            | ("norm_cdf", 3)
    )
}

fn floats(value: &Value, what: &str) -> Res<(Vec<usize>, Vec<f64>)> {
    let Value::Array(a) = value else {
        return fail(format!("{what} expects an Array<Float>"));
    };
    let a = a.borrow();
    let mut data = Vec::with_capacity(a.data.len());
    for v in &a.data {
        match v {
            Value::Float(x) => data.push(*x),
            other => return fail(format!("{what} expects Float elements, got '{other}'")),
        }
    }
    Ok((a.shape.clone(), data))
}

fn float(value: &Value, what: &str) -> Res<f64> {
    match value {
        Value::Float(x) => Ok(*x),
        other => fail(format!("{what} expects a Float, got '{other}'")),
    }
}

fn vector(values: Vec<f64>) -> Value {
    let n = values.len();
    array::make(vec![n], values.into_iter().map(Value::Float).collect())
}

/// Eigenvalues of a symmetric matrix by cyclic Jacobi rotations, sorted ascending. Only `+ - * /`
/// and `sqrt`, in a fixed order (mirrored by `array_linalg.c`).
fn jacobi_eigenvalues(mut a: Vec<f64>, n: usize) -> Vec<f64> {
    for _sweep in 0..100 {
        let mut rotated = false;
        for p in 0..n {
            for q in p + 1..n {
                let apq = a[p * n + q];
                if apq.abs() <= 1e-15 * (a[p * n + p].abs() + a[q * n + q].abs()) {
                    continue;
                }
                rotated = true;
                let theta = (a[q * n + q] - a[p * n + p]) / (2.0 * apq);
                let sign = if theta < 0.0 { -1.0 } else { 1.0 };
                let t = sign / (theta.abs() + (theta * theta + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..n {
                    let (akp, akq) = (a[k * n + p], a[k * n + q]);
                    a[k * n + p] = c * akp - s * akq;
                    a[k * n + q] = s * akp + c * akq;
                }
                for k in 0..n {
                    let (apk, aqk) = (a[p * n + k], a[q * n + k]);
                    a[p * n + k] = c * apk - s * aqk;
                    a[q * n + k] = s * apk + c * aqk;
                }
            }
        }
        if !rotated {
            break;
        }
    }
    let mut values: Vec<f64> = (0..n).map(|i| a[i * n + i]).collect();
    for i in 1..n {
        let mut j = i;
        while j > 0 && values[j - 1] > values[j] {
            values.swap(j - 1, j);
            j -= 1;
        }
    }
    values
}

/// Determinant by the same elimination as `solve_system`; a singular matrix gives exactly 0.
fn determinant(mut m: Vec<f64>, n: usize) -> f64 {
    let mut det = 1.0;
    for col in 0..n {
        let mut piv = col;
        let mut best = m[col * n + col].abs();
        for r in col + 1..n {
            let candidate = m[r * n + col].abs();
            if candidate > best {
                best = candidate;
                piv = r;
            }
        }
        if best == 0.0 {
            return 0.0;
        }
        if piv != col {
            for c in 0..n {
                m.swap(piv * n + c, col * n + c);
            }
            det = -det;
        }
        det = det * m[col * n + col];
        for r in col + 1..n {
            let f = m[r * n + col] / m[col * n + col];
            for c in col..n {
                m[r * n + c] = m[r * n + c] - f * m[col * n + c];
            }
        }
    }
    det
}

/// Gaussian elimination with partial pivoting (first maximum wins ties).
fn solve_system(mut m: Vec<f64>, mut v: Vec<f64>, n: usize) -> Res<Vec<f64>> {
    for col in 0..n {
        let mut piv = col;
        let mut best = m[col * n + col].abs();
        for r in col + 1..n {
            let candidate = m[r * n + col].abs();
            if candidate > best {
                best = candidate;
                piv = r;
            }
        }
        if best == 0.0 {
            return fail("singular matrix");
        }
        if piv != col {
            for c in 0..n {
                m.swap(piv * n + c, col * n + c);
            }
            v.swap(piv, col);
        }
        for r in col + 1..n {
            let f = m[r * n + col] / m[col * n + col];
            for c in col..n {
                m[r * n + c] = m[r * n + c] - f * m[col * n + c];
            }
            v[r] = v[r] - f * v[col];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = v[i];
        for j in i + 1..n {
            s = s - m[i * n + j] * x[j];
        }
        x[i] = s / m[i * n + i];
    }
    Ok(x)
}

pub fn call(name: &str, args: &[Value]) -> Res<Value> {
    match name {
        "linfit" => {
            let ((sx, x), (sy, y)) = (floats(&args[0], "linfit")?, floats(&args[1], "linfit")?);
            if sx.len() != 1 || sy.len() != 1 || x.len() != y.len() || x.len() < 2 {
                return fail(
                    "linfit needs two one-dimensional arrays of the same length (at least 2)",
                );
            }
            let n = x.len() as f64;
            let mut sum_x = x[0];
            let mut sum_y = y[0];
            for i in 1..x.len() {
                sum_x = sum_x + x[i];
                sum_y = sum_y + y[i];
            }
            let (mx, my) = (sum_x / n, sum_y / n);
            let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
            for i in 0..x.len() {
                let (dx, dy) = (x[i] - mx, y[i] - my);
                sxx = sxx + dx * dx;
                sxy = sxy + dx * dy;
                syy = syy + dy * dy;
            }
            if sxx == 0.0 {
                return fail("linfit needs x values that are not all equal");
            }
            let slope = sxy / sxx;
            let intercept = my - slope * mx;
            let r2 = if syy == 0.0 {
                1.0
            } else {
                sxy * sxy / (sxx * syy)
            };
            Ok(vector(vec![slope, intercept, r2]))
        }
        "polyfit" => {
            let ((sx, x), (sy, y)) = (floats(&args[0], "polyfit")?, floats(&args[1], "polyfit")?);
            let Value::Int(degree) = &args[2] else {
                return fail("polyfit expects an Int degree");
            };
            if sx.len() != 1 || sy.len() != 1 || x.len() != y.len() {
                return fail("polyfit needs two one-dimensional arrays of the same length");
            }
            if *degree < 0 || (*degree as usize) + 1 > x.len() {
                return fail("polyfit needs 0 <= degree < number of points");
            }
            let d = *degree as usize;
            let k = d + 1;
            // powers[i][p] = x_i^p, built by repeated multiplication.
            let mut powers = vec![vec![1.0; 2 * d + 1]; x.len()];
            for i in 0..x.len() {
                for p in 1..=2 * d {
                    powers[i][p] = powers[i][p - 1] * x[i];
                }
            }
            let mut a = vec![0.0; k * k];
            let mut b = vec![0.0; k];
            for r in 0..k {
                for c in 0..k {
                    let mut acc = 0.0;
                    for i in 0..x.len() {
                        acc = acc + powers[i][r + c];
                    }
                    a[r * k + c] = acc;
                }
                let mut acc = 0.0;
                for i in 0..x.len() {
                    acc = acc + y[i] * powers[i][r];
                }
                b[r] = acc;
            }
            Ok(vector(solve_system(a, b, k)?))
        }
        "polyval" => {
            let (_, coefficients) = floats(&args[0], "polyval")?;
            let horner = |x: f64| {
                let mut r = coefficients[coefficients.len() - 1];
                for i in (0..coefficients.len() - 1).rev() {
                    r = r * x + coefficients[i];
                }
                r
            };
            match &args[1] {
                Value::Float(x) => Ok(Value::Float(horner(*x))),
                other => {
                    let (shape, xs) = floats(other, "polyval")?;
                    Ok(array::make(
                        shape,
                        xs.into_iter().map(|x| Value::Float(horner(x))).collect(),
                    ))
                }
            }
        }
        "det" | "inv" | "trace" => {
            let (shape, a) = floats(&args[0], name)?;
            if shape.len() != 2 || shape[0] != shape[1] {
                return fail(format!("{name} needs a square (n, n) matrix"));
            }
            let n = shape[0];
            match name {
                "trace" => {
                    let mut acc = 0.0;
                    for i in 0..n {
                        acc = acc + a[i * n + i];
                    }
                    Ok(Value::Float(acc))
                }
                "det" => Ok(Value::Float(determinant(a, n))),
                _ => {
                    let mut out = vec![0.0; n * n];
                    for j in 0..n {
                        let mut unit = vec![0.0; n];
                        unit[j] = 1.0;
                        let column = solve_system(a.clone(), unit, n)?;
                        for i in 0..n {
                            out[i * n + j] = column[i];
                        }
                    }
                    Ok(array::make(
                        vec![n, n],
                        out.into_iter().map(Value::Float).collect(),
                    ))
                }
            }
        }
        "norm" => {
            let (_, a) = floats(&args[0], "norm")?;
            let mut acc = 0.0;
            for x in a {
                acc = acc + x * x;
            }
            Ok(Value::Float(acc.sqrt()))
        }
        "eigvals" => {
            let (shape, a) = floats(&args[0], "eigvals")?;
            if shape.len() != 2 || shape[0] != shape[1] {
                return fail("eigvals needs a square (n, n) matrix");
            }
            let n = shape[0];
            for i in 0..n {
                for j in i + 1..n {
                    if (a[i * n + j] - a[j * n + i]).abs() > 1e-9 * (1.0 + a[i * n + j].abs()) {
                        return fail("eigvals needs a symmetric matrix");
                    }
                }
            }
            Ok(vector(jacobi_eigenvalues(a, n)))
        }
        "eye" => {
            let Value::Int(n) = &args[0] else {
                return fail("eye expects an Int size");
            };
            if *n < 1 {
                return fail("eye needs n >= 1");
            }
            let n = *n as usize;
            let mut out = vec![0.0; n * n];
            for i in 0..n {
                out[i * n + i] = 1.0;
            }
            Ok(array::make(
                vec![n, n],
                out.into_iter().map(Value::Float).collect(),
            ))
        }
        "solve" => {
            let ((sa, a), (sb, b)) = (floats(&args[0], "solve")?, floats(&args[1], "solve")?);
            if sa.len() != 2 || sa[0] != sa[1] || sb.len() != 1 || sb[0] != sa[0] {
                return fail("solve needs an (n, n) matrix and a vector of length n");
            }
            Ok(vector(solve_system(a, b, sa[0])?))
        }
        "histogram" => {
            let (_, data) = floats(&args[0], "histogram")?;
            let Value::Int(bins) = &args[1] else {
                return fail("histogram expects an Int bin count");
            };
            let (lo, hi) = (float(&args[2], "histogram")?, float(&args[3], "histogram")?);
            if *bins < 1 || !(hi > lo) {
                return fail("histogram needs bins >= 1 and lo < hi");
            }
            let bins = *bins as usize;
            let width = (hi - lo) / bins as f64;
            let mut counts = vec![0i64; bins];
            for v in data {
                if !(v >= lo && v <= hi) {
                    continue;
                }
                let mut index = ((v - lo) / width).floor() as usize;
                if index >= bins {
                    index = bins - 1;
                }
                counts[index] += 1;
            }
            let n = counts.len();
            Ok(array::make(
                vec![n],
                counts.into_iter().map(Value::Int).collect(),
            ))
        }
        "norm_pdf" | "norm_cdf" => {
            let (mu, sigma) = (float(&args[1], name)?, float(&args[2], name)?);
            if !(sigma > 0.0) {
                return fail(format!("{name} needs sigma > 0"));
            }
            let f = |x: f64| {
                if name == "norm_pdf" {
                    detmath::norm_pdf(x, mu, sigma)
                } else {
                    detmath::norm_cdf(x, mu, sigma)
                }
            };
            match &args[0] {
                Value::Float(x) => Ok(Value::Float(f(*x))),
                other => {
                    let (shape, xs) = floats(other, name)?;
                    Ok(array::make(
                        shape,
                        xs.into_iter().map(|x| Value::Float(f(x))).collect(),
                    ))
                }
            }
        }
        _ => fail(format!("'{name}' isn't a regression function")),
    }
}
