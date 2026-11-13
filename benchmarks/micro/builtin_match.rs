// Cost of the built-in name check that every named call performs before it looks up a user function.
//
// The 24 names and their order are those of the match in compiler/src/interpreter/mod.rs (eval_call). The
// function reproduces that match as a function of the name, and a user function name (7 bytes, like "advance")
// is tested against it, which is the path a user call takes. The result is kept with black_box so that the
// comparisons are performed. Build with: rustc -O builtin_match.rs
use std::hint::black_box;

#[inline(never)]
fn builtin_kind(name: &str) -> u8 {
    match name {
        "print" => 1,
        "args" => 2,
        "yield" => 3,
        "env" => 4,
        "path_join" => 5,
        "cwd" => 6,
        "file_exists" => 7,
        "char_from_codepoint" => 8,
        "hash" => 9,
        "format" => 10,
        "format_float_value" => 11,
        "temperature_point" => 12,
        "temperature_point_to" => 13,
        "select" => 14,
        "clone" => 15,
        "drop" => 16,
        "sum" => 17,
        "read_file" => 18,
        "write_file" => 19,
        "parse_csv" => 20,
        "parse_int" => 21,
        "panic" => 22,
        "assert" => 23,
        "assert_eq" => 24,
        _ => 0,
    }
}

fn main() {
    let names = ["advance", "step_one", "fact_rec"];
    let iterations: u64 = 10_000_000;
    let started = std::time::Instant::now();
    let mut total: u64 = 0;
    for i in 0..iterations {
        let name = names[(i % 3) as usize];
        total += builtin_kind(black_box(name)) as u64;
    }
    let elapsed = started.elapsed().as_nanos() as f64;
    println!(
        "RESULT builtin_match {} iterations, {:.1} ns per check (sum {})",
        iterations,
        elapsed / iterations as f64,
        black_box(total)
    );
}
