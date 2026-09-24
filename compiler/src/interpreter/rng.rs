//! `Rng`: a reproducible pseudo-random generator (xoshiro256** seeded through
//! splitmix64). Every step uses only integer operations and correctly-rounded
//! IEEE arithmetic (`+ - * /` and `sqrt`), plus the deterministic natural log of `detmath.rs`, built
//! from those same operations, so the stream is bit-for-bit identical on every
//! platform and in the native backend (`RNG_RUNTIME` in `codegen.rs` mirrors this
//! file line by line). Never replace `detmath::ln` with `f64::ln`: the platform's libm
//! is allowed to differ in the last digit.

use super::{array, RuntimeError, Value};

pub struct RngState {
    s: [u64; 4],
}

fn fail<T>(message: impl Into<String>) -> Result<T, RuntimeError> {
    Err(RuntimeError::Error(message.into()))
}

fn splitmix64(x: &mut u64) -> u64 {
    *x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl RngState {
    pub fn new(seed: i64) -> RngState {
        let mut x = seed as u64;
        let mut s = [0u64; 4];
        for slot in s.iter_mut() {
            *slot = splitmix64(&mut x);
        }
        RngState { s }
    }

    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.s;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    /// Uniform in `[0, 1)` with 53 random bits.
    pub fn next_float(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / 9007199254740992.0)
    }

    /// Uniform in `[lo, hi)`, without modulo bias.
    pub fn next_int(&mut self, lo: i64, hi: i64) -> Result<i64, RuntimeError> {
        if hi <= lo {
            return fail("next_int needs lo < hi");
        }
        let range = (hi as u64).wrapping_sub(lo as u64);
        let threshold = range.wrapping_neg() % range;
        loop {
            let x = self.next_u64();
            if x >= threshold {
                return Ok((lo as u64).wrapping_add(x % range) as i64);
            }
        }
    }

    /// Standard normal (Marsaglia polar method; the second value is discarded).
    pub fn normal(&mut self) -> f64 {
        loop {
            let u = 2.0 * self.next_float() - 1.0;
            let v = 2.0 * self.next_float() - 1.0;
            let s = u * u + v * v;
            if s > 0.0 && s < 1.0 {
                return u * ((-2.0 * super::detmath::ln(s)) / s).sqrt();
            }
        }
    }
}

fn shape_of(value: &Value) -> Result<Vec<usize>, RuntimeError> {
    let Value::List(items) = value else {
        return fail("an array shape must be a List<Int>");
    };
    let mut shape = Vec::new();
    for item in items.borrow().iter() {
        match item {
            Value::Int(n) if *n >= 1 => shape.push(*n as usize),
            _ => return fail("array dimensions must be Int values of at least 1"),
        }
    }
    if shape.is_empty() {
        return fail("an array needs at least one dimension");
    }
    Ok(shape)
}

fn as_int(value: &Value) -> Result<i64, RuntimeError> {
    match value {
        Value::Int(n) => Ok(*n),
        other => fail(format!("expected an Int, got '{other}'")),
    }
}

/// `rng.method(args)`.
pub fn call_method(
    rng: &mut RngState,
    method: &str,
    args: &[Value],
) -> Result<Value, RuntimeError> {
    match (method, args) {
        ("next_float", []) => Ok(Value::Float(rng.next_float())),
        ("normal", []) => Ok(Value::Float(rng.normal())),
        ("next_int", [lo, hi]) => Ok(Value::Int(rng.next_int(as_int(lo)?, as_int(hi)?)?)),
        ("rand", [shape]) | ("randn", [shape]) => {
            let shape = shape_of(shape)?;
            let count: usize = shape.iter().product();
            let data = (0..count)
                .map(|_| {
                    Value::Float(if method == "rand" {
                        rng.next_float()
                    } else {
                        rng.normal()
                    })
                })
                .collect();
            Ok(array::make(shape, data))
        }
        ("randint", [lo, hi, shape]) => {
            let (lo, hi) = (as_int(lo)?, as_int(hi)?);
            let shape = shape_of(shape)?;
            let count: usize = shape.iter().product();
            let mut data = Vec::with_capacity(count);
            for _ in 0..count {
                data.push(Value::Int(rng.next_int(lo, hi)?));
            }
            Ok(array::make(shape, data))
        }
        ("permutation", [n]) => {
            let n = as_int(n)?;
            if n < 1 {
                return fail("permutation needs n >= 1");
            }
            let mut items: Vec<i64> = (0..n).collect();
            for i in (1..n as usize).rev() {
                let j = rng.next_int(0, i as i64 + 1)? as usize;
                items.swap(i, j);
            }
            Ok(array::make(
                vec![n as usize],
                items.into_iter().map(Value::Int).collect(),
            ))
        }
        _ => fail(format!("Rng has no method '{method}' with these arguments")),
    }
}
