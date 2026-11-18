"""In-process repetitions of a kernel of benchmarks/python/kernels.py.

Usage: python run_v2.py NAME K
Runs the kernel K times in one process. Each repetition is timed with time.perf_counter, so the times exclude process
start. Prints one "RESULT <name> <value>" line (from the last repetition) and one "REP_MS t1 t2 ... tK" line.
"""
import os
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "python"))
import kernels  # noqa: E402


def main():
    name = sys.argv[1]
    repeats = int(sys.argv[2]) if len(sys.argv) > 2 else 1
    fn = kernels.KERNELS[name]
    times = []
    value = None
    for _ in range(repeats):
        started = time.perf_counter()
        value = fn()
        times.append((time.perf_counter() - started) * 1000.0)
    if isinstance(value, float):
        value = repr(value)
    print("RESULT %s %s" % (name, value))
    print("REP_MS " + " ".join(repr(t) for t in times))


if __name__ == "__main__":
    main()
