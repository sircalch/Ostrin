"""Linear-algebra kernels for the Benchmark V2 linear-algebra study.

Usage: python linalg_kernels.py KERNEL IMPL N K

KERNEL is matmul, lu_solve or norms. IMPL is pure (plain Python lists and loops, no library) or numpy (NumPy, which calls
the BLAS and LAPACK libraries it was built with). The inputs are generated once, outside the timed repetitions, with the
same formulas as the Ostrin programs and benchmarks/v2/linalg/reference.mjs. The kernel runs K times in this process; each
repetition is timed with time.perf_counter, so the times exclude the interpreter start-up and the input generation.

Prints one "RESULT <kernel> <numbers>" line (from the last repetition, the same numbers the Ostrin program prints) and one
"REP_MS t1 ... tK" line. The NumPy import time is not inside any repetition.
"""
import sys
import time


def matmul_inputs(n):
    a = [[((7 * i + 3 * j) % 11 - 5) / 4.0 for j in range(n)] for i in range(n)]
    b = [[((5 * i + 13 * j) % 9 - 4) / 8.0 for j in range(n)] for i in range(n)]
    return a, b


def solve_inputs(n):
    d = [[(2.0 * n if i == j else ((3 * i + 5 * j) % 7 - 3) / 4.0) for j in range(n)] for i in range(n)]
    x_star = [float(j % 9 - 4) for j in range(n)]
    b = [sum(d[i][j] * x_star[j] for j in range(n)) for i in range(n)]
    return d, b


def norms_inputs(n):
    return [[((7 * i + 3 * j) % 11 - 5) / 4.0 for j in range(n)] for i in range(n)]


# ---------------------------------------------------------------- pure Python (no library)

def matmul_pure(a, b, n):
    total = 0.0
    for i in range(n):
        row = a[i]
        for j in range(n):
            acc = 0.0
            for k in range(n):
                acc += row[k] * b[k][j]
            total += acc
    return total


def lu_solve_pure(d, b, n):
    m = [row[:] for row in d]
    rhs = b[:]
    for k in range(n):
        p = k
        best = abs(m[k][k])
        for i in range(k + 1, n):
            if abs(m[i][k]) > best:
                best = abs(m[i][k])
                p = i
        if p != k:
            m[k], m[p] = m[p], m[k]
            rhs[k], rhs[p] = rhs[p], rhs[k]
        pivot = m[k][k]
        for i in range(k + 1, n):
            factor = m[i][k] / pivot
            m[i][k] = factor
            for j in range(k + 1, n):
                m[i][j] -= factor * m[k][j]
            rhs[i] -= factor * rhs[k]
    x = [0.0] * n
    for i in range(n - 1, -1, -1):
        acc = rhs[i]
        for j in range(i + 1, n):
            acc -= m[i][j] * x[j]
        x[i] = acc / m[i][i]
    return sum(x)


def norms_pure(a, n):
    col = 0.0
    for j in range(n):
        s = 0.0
        for i in range(n):
            s += abs(a[i][j])
        col = max(col, s)
    row = 0.0
    for i in range(n):
        s = 0.0
        for j in range(n):
            s += abs(a[i][j])
        row = max(row, s)
    sq = 0.0
    for i in range(n):
        for j in range(n):
            sq += a[i][j] * a[i][j]
    return (col + row), sq ** 0.5


# ---------------------------------------------------------------- NumPy (library)

def matmul_numpy(a, b):
    import numpy as np
    c = np.asarray(a) @ np.asarray(b)
    return float(c.sum())


def lu_solve_numpy(d, b):
    import numpy as np
    x = np.linalg.solve(np.asarray(d), np.asarray(b))
    return float(x.sum())


def norms_numpy(a):
    import numpy as np
    arr = np.asarray(a)
    return float(np.linalg.norm(arr, 1) + np.linalg.norm(arr, np.inf)), float(np.linalg.norm(arr, "fro"))


def main():
    kernel, impl, n, repeats = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
    if kernel == "matmul":
        a, b = matmul_inputs(n)
        run = (lambda: matmul_pure(a, b, n)) if impl == "pure" else (lambda: matmul_numpy(a, b))
    elif kernel == "lu_solve":
        d, b = solve_inputs(n)
        run = (lambda: lu_solve_pure(d, b, n)) if impl == "pure" else (lambda: lu_solve_numpy(d, b))
    elif kernel == "norms":
        a = norms_inputs(n)
        run = (lambda: norms_pure(a, n)) if impl == "pure" else (lambda: norms_numpy(a))
    else:
        raise SystemExit(f"unknown kernel {kernel}")
    times = []
    value = None
    for _ in range(repeats):
        started = time.perf_counter()
        value = run()
        times.append((time.perf_counter() - started) * 1000.0)
    numbers = value if isinstance(value, tuple) else (value,)
    print(f"RESULT {kernel} " + " ".join(repr(float(v)) for v in numbers))
    print("REP_MS " + " ".join(repr(t) for t in times))


if __name__ == "__main__":
    main()
