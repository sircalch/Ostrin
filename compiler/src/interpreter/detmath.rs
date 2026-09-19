//! Deterministic elementary functions. Every result is computed with integer
//! operations, `floor`, `sqrt` and the correctly-rounded IEEE operations
//! `+ - * /` only — never with the platform's `libm` — so the same input gives
//! the same bits on every machine and in the native backend, whose
//! `detmath_runtime.c` mirrors this file operation for operation. Accuracy is
//! about 1e-15 relative (not correctly rounded). Do not "improve" a formula
//! here without changing the C file identically.

const LN2: f64 = 0.6931471805599453;
const LN2_HI: f64 = 6.93147180369123816490e-01;
const LN2_LO: f64 = 1.90821492927058770002e-10;
const INV_LN2: f64 = 1.4426950408889634;
const LN10: f64 = 2.302585092994046;
const PI: f64 = 3.141592653589793;
const PIO2: f64 = 1.5707963267948966;
const PIO4: f64 = 0.7853981633974483;
const TWO_OVER_PI: f64 = 0.6366197723675814;
const PIO2_1: f64 = 1.57079632673412561417e+00;
const PIO2_1T: f64 = 6.07710050650619224932e-11;

/// 2^k for k in [-1022, 1023].
fn pow2(k: i64) -> f64 {
    f64::from_bits(((k + 1023) as u64) << 52)
}

pub fn exp(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x > 709.782712893384 {
        return f64::INFINITY;
    }
    if x < -745.1332191019411 {
        return 0.0;
    }
    let k = (x * INV_LN2 + 0.5).floor();
    let r = (x - k * LN2_HI) - k * LN2_LO;
    let mut term = 1.0;
    let mut sum = 1.0;
    for n in 1..=26 {
        term = term * r / (n as f64);
        sum = sum + term;
    }
    let k = k as i64;
    let half = k / 2;
    sum * pow2(half) * pow2(k - half)
}

pub fn ln(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x.is_infinite() {
        return x;
    }
    let mut x = x;
    let mut adjust: i64 = 0;
    if x < 2.2250738585072014e-308 {
        x = x * 18014398509481984.0;
        adjust = -54;
    }
    let bits = x.to_bits();
    let mut e = ((bits >> 52) & 0x7FF) as i64 - 1023 + adjust;
    let mut m = f64::from_bits((bits & 0x000F_FFFF_FFFF_FFFF) | 0x3FF0_0000_0000_0000);
    if m > 1.4142135623730951 {
        m = m * 0.5;
        e += 1;
    }
    let z = (m - 1.0) / (m + 1.0);
    let z2 = z * z;
    let mut term = z;
    let mut sum = 0.0;
    for k in 0..16 {
        sum = sum + term / ((2 * k + 1) as f64);
        term = term * z2;
    }
    (e as f64) * LN2 + 2.0 * sum
}

pub fn log10(x: f64) -> f64 {
    ln(x) / LN10
}

fn sin_kernel(r: f64) -> f64 {
    let r2 = r * r;
    let mut term = r;
    let mut sum = r;
    for i in 1..=13 {
        term = -term * r2 / (((2 * i) * (2 * i + 1)) as f64);
        sum = sum + term;
    }
    sum
}

fn cos_kernel(r: f64) -> f64 {
    let r2 = r * r;
    let mut term = 1.0;
    let mut sum = 1.0;
    for i in 1..=13 {
        term = -term * r2 / (((2 * i - 1) * (2 * i)) as f64);
        sum = sum + term;
    }
    sum
}

/// Reduces `x` to `r` in about [-pi/4, pi/4] and the quadrant `n mod 4`.
fn reduce(x: f64) -> Option<(f64, i64)> {
    if !x.is_finite() || x.abs() > 1.0e6 {
        return None;
    }
    let n = (x * TWO_OVER_PI + 0.5).floor();
    let r = (x - n * PIO2_1) - n * PIO2_1T;
    let n = n as i64;
    Some((r, ((n % 4) + 4) % 4))
}

pub fn sin(x: f64) -> f64 {
    match reduce(x) {
        None => f64::NAN,
        Some((r, q)) => match q {
            0 => sin_kernel(r),
            1 => cos_kernel(r),
            2 => -sin_kernel(r),
            _ => -cos_kernel(r),
        },
    }
}

pub fn cos(x: f64) -> f64 {
    match reduce(x) {
        None => f64::NAN,
        Some((r, q)) => match q {
            0 => cos_kernel(r),
            1 => -sin_kernel(r),
            2 => -cos_kernel(r),
            _ => sin_kernel(r),
        },
    }
}

pub fn tan(x: f64) -> f64 {
    match reduce(x) {
        None => f64::NAN,
        Some((r, q)) => {
            let (s, c) = (sin_kernel(r), cos_kernel(r));
            match q {
                0 | 2 => s / c,
                _ => -c / s,
            }
        }
    }
}

fn atan_series(z: f64) -> f64 {
    let z2 = z * z;
    let mut term = z;
    let mut sum = z;
    for i in 1..=27 {
        term = -term * z2;
        sum = sum + term / ((2 * i + 1) as f64);
    }
    sum
}

fn atan_core(a: f64) -> f64 {
    if a > 0.4142135623730951 {
        PIO4 + atan_series((a - 1.0) / (a + 1.0))
    } else {
        atan_series(a)
    }
}

pub fn atan(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    let ax = if x < 0.0 { -x } else { x };
    let r = if ax > 1.0 { PIO2 - atan_core(1.0 / ax) } else { atan_core(ax) };
    if x < 0.0 { -r } else { r }
}

pub fn atan2(y: f64, x: f64) -> f64 {
    if y.is_nan() || x.is_nan() {
        return f64::NAN;
    }
    if x > 0.0 {
        atan(y / x)
    } else if x < 0.0 {
        if y >= 0.0 { atan(y / x) + PI } else { atan(y / x) - PI }
    } else if y > 0.0 {
        PIO2
    } else if y < 0.0 {
        -PIO2
    } else {
        0.0
    }
}

pub fn asin(x: f64) -> f64 {
    if x.is_nan() || x > 1.0 || x < -1.0 {
        return f64::NAN;
    }
    atan2(x, (1.0 - x * x).sqrt())
}

pub fn acos(x: f64) -> f64 {
    if x.is_nan() || x > 1.0 || x < -1.0 {
        return f64::NAN;
    }
    atan2((1.0 - x * x).sqrt(), x)
}

pub fn sinh(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    let ax = if x < 0.0 { -x } else { x };
    if ax < 0.1 {
        let x2 = x * x;
        x * (1.0 + x2 / 6.0 * (1.0 + x2 / 20.0 * (1.0 + x2 / 42.0 * (1.0 + x2 / 72.0 * (1.0 + x2 / 110.0)))))
    } else {
        (exp(x) - exp(-x)) / 2.0
    }
}

pub fn cosh(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    (exp(x) + exp(-x)) / 2.0
}

pub fn tanh(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x > 20.0 {
        return 1.0;
    }
    if x < -20.0 {
        return -1.0;
    }
    sinh(x) / cosh(x)
}

pub fn pow(x: f64, y: f64) -> f64 {
    if y == 0.0 {
        return 1.0;
    }
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    let y_is_integer = y == y.floor();
    if y_is_integer && y.abs() <= 1024.0 {
        let mut result = 1.0;
        let mut base = x;
        let mut e = (if y < 0.0 { -y } else { y }) as u64;
        while e > 0 {
            if e & 1 == 1 {
                result = result * base;
            }
            base = base * base;
            e >>= 1;
        }
        return if y < 0.0 { 1.0 / result } else { result };
    }
    if x == 0.0 {
        return if y > 0.0 { 0.0 } else { f64::INFINITY };
    }
    if x < 0.0 {
        if !y_is_integer {
            return f64::NAN;
        }
        let odd = (y / 2.0).floor() * 2.0 != y;
        let magnitude = exp(y * ln(-x));
        return if odd { -magnitude } else { magnitude };
    }
    exp(y * ln(x))
}

const TWO_OVER_SQRTPI: f64 = 1.1283791670955126;
const SQRT_PI: f64 = 1.7724538509055159;
const SQRT2: f64 = 1.4142135623730951;
const SQRT_2PI: f64 = 2.5066282746310002;

/// Error function: Taylor series below 2, continued fraction for the
/// complementary function above (no cancellation), saturated beyond 6.
pub fn erf(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    let ax = if x < 0.0 { -x } else { x };
    let magnitude = if ax >= 6.0 {
        1.0
    } else if ax < 2.0 {
        let x2 = ax * ax;
        let mut term = ax;
        let mut sum = ax;
        for n in 1..=60 {
            term = -term * x2 / (n as f64);
            sum = sum + term / ((2 * n + 1) as f64);
        }
        TWO_OVER_SQRTPI * sum
    } else {
        let mut t = ax;
        for k in (1..=60).rev() {
            t = ax + (k as f64 / 2.0) / t;
        }
        1.0 - exp(-ax * ax) / SQRT_PI / t
    };
    if x < 0.0 { -magnitude } else { magnitude }
}

/// Density of `N(mu, sigma)` at `x`.
pub fn norm_pdf(x: f64, mu: f64, sigma: f64) -> f64 {
    let z = (x - mu) / sigma;
    exp(-0.5 * z * z) / (sigma * SQRT_2PI)
}

/// Cumulative distribution of `N(mu, sigma)` at `x`.
pub fn norm_cdf(x: f64, mu: f64, sigma: f64) -> f64 {
    0.5 * (1.0 + erf((x - mu) / (sigma * SQRT2)))
}
