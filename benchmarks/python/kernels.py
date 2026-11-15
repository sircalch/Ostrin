"""Python counterparts of benchmarks/ostrin/*.ostrin, with the same algorithms and the same order of operations.

Usage: python kernels.py NAME      (NAME is one of simpson, bisect, matmul, regression, horner)
Prints one line "RESULT <name> <values>" like the Ostrin programs do.
"""
import math
import sys
import time


def simpson():
    a, b, n = 0.0, 3.141592653589793, 4000000
    panels = n if n % 2 == 0 else n + 1
    h = (b - a) / float(panels)
    total = math.sin(a) + math.sin(b)
    for i in range(1, panels):
        x = a + float(i) * h
        total = total + (4.0 if i % 2 == 1 else 2.0) * math.sin(x)
    return total * h / 3.0


def bisect():
    root = 0.0
    for _ in range(200000):
        lo, hi = 2.0, 3.0
        f = lambda x: x * x * x - 2.0 * x - 5.0
        flo = f(lo)
        fhi = f(hi)
        result = None
        if flo == 0.0:
            result = lo
        elif fhi == 0.0:
            result = hi
        else:
            i = 0
            tol = 0.0000000000001
            while i < 200 and hi - lo > tol:
                mid = (lo + hi) / 2.0
                fmid = f(mid)
                if fmid == 0.0:
                    result = mid
                    break
                if (fmid < 0.0) == (flo < 0.0):
                    lo = mid
                    flo = fmid
                else:
                    hi = mid
                i += 1
            if result is None:
                result = (lo + hi) / 2.0
        root = result
    return root


def matmul():
    n = 300
    a = [0.0] * (n * n)
    b = [0.0] * (n * n)
    for i in range(n):
        for j in range(n):
            a[i * n + j] = float((i * j + i + 2 * j) % 7)
            b[i * n + j] = float((3 * i + j * j) % 5)
    c = [0.0] * (n * n)
    for i in range(n):
        for j in range(n):
            acc = 0.0
            for k in range(n):
                acc = acc + a[i * n + k] * b[k * n + j]
            c[i * n + j] = acc
    trace = 0.0
    total = 0.0
    for i in range(n):
        trace = trace + c[i * n + i]
        for j in range(n):
            total = total + c[i * n + j]
    return "trace %s total %s" % (repr(trace), repr(total))


def regression():
    n = 2000000
    s0 = s1 = s2 = s3 = s4 = t0 = t1 = t2 = 0.0
    for i in range(n):
        x = float(i) / float(n) * 2.0 - 1.0
        y = 1.0 + 2.0 * x - 3.0 * x * x
        x2 = x * x
        s0 = s0 + 1.0
        s1 = s1 + x
        s2 = s2 + x2
        s3 = s3 + x2 * x
        s4 = s4 + x2 * x2
        t0 = t0 + y
        t1 = t1 + x * y
        t2 = t2 + x2 * y
    m10 = s1 / s0
    a11 = s2 - m10 * s1
    a12 = s3 - m10 * s2
    b1 = t1 - m10 * t0
    m20 = s2 / s0
    a21 = s3 - m20 * s1
    a22 = s4 - m20 * s2
    b2 = t2 - m20 * t0
    m21 = a21 / a11
    a22b = a22 - m21 * a12
    b2b = b2 - m21 * b1
    c2 = b2b / a22b
    c1 = (b1 - a12 * c2) / a11
    c0 = (t0 - s1 * c1 - s2 * c2) / s0
    return "%s %s %s" % (repr(c0), repr(c1), repr(c2))


def horner():
    degree = 50
    coefficients = [float((k % 5) - 2) for k in range(degree + 1)]
    total = 0.0
    for i in range(2000000):
        x = float(i % 1000) / 1000.0
        value = 0.0
        for k in range(degree + 1):
            value = value * x + coefficients[degree - k]
        total = total + value
    return repr(total)


def sieve():
    """Number of primes below 300000 (Sieve of Eratosthenes)."""
    n = 300000
    composite = [False] * n
    count = 0
    for i in range(2, n):
        if not composite[i]:
            count += 1
            for j in range(i * i, n, i):
                composite[j] = True
    return count


def collatz():
    """Total number of Collatz steps for the starts 1 to 19999."""
    total = 0
    for n in range(1, 20000):
        x = n
        while x != 1:
            x = x // 2 if x % 2 == 0 else 3 * x + 1
            total += 1
    return total


KERNELS = {"simpson": simpson, "bisect": bisect, "matmul": matmul, "regression": regression, "horner": horner, "sieve": sieve, "collatz": collatz}

if __name__ == "__main__":
    name = sys.argv[1]
    timed = "--timed" in sys.argv[2:]
    started = time.perf_counter()
    value = KERNELS[name]()
    cold_ms = (time.perf_counter() - started) * 1000.0
    if isinstance(value, float):
        value = repr(value)
    print("RESULT %s %s" % (name, value))
    if timed:
        # A second call in the same process, after the first has run: the warm time, without process start.
        started = time.perf_counter()
        KERNELS[name]()
        warm_ms = (time.perf_counter() - started) * 1000.0
        print("TIMING cold_ms %.3f warm_ms %.3f" % (cold_ms, warm_ms))
