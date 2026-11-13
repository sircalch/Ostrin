# Julia counterparts of benchmarks/ostrin/*.ostrin, with the same algorithms and the same order of operations.
#
# Usage: julia kernels.jl NAME   (NAME is one of simpson, bisect, matmul, regression, horner)
# Prints one line "RESULT <name> <values>" like the Ostrin programs do.

function simpson()
    a, b, n = 0.0, 3.141592653589793, 4000000
    panels = n % 2 == 0 ? n : n + 1
    h = (b - a) / Float64(panels)
    total = sin(a) + sin(b)
    for i in 1:(panels - 1)
        x = a + Float64(i) * h
        total = total + (i % 2 == 1 ? 4.0 : 2.0) * sin(x)
    end
    return total * h / 3.0
end

function bisect_once(f, lo, hi, tol, max_iter)
    flo = f(lo)
    fhi = f(hi)
    if flo == 0.0
        return lo
    elseif fhi == 0.0
        return hi
    end
    i = 0
    while i < max_iter && hi - lo > tol
        mid = (lo + hi) / 2.0
        fmid = f(mid)
        if fmid == 0.0
            return mid
        end
        if (fmid < 0.0) == (flo < 0.0)
            lo = mid
            flo = fmid
        else
            hi = mid
        end
        i += 1
    end
    return (lo + hi) / 2.0
end

function bisect()
    f(x) = x * x * x - 2.0 * x - 5.0
    root = 0.0
    for _ in 1:200000
        root = bisect_once(f, 2.0, 3.0, 0.0000000000001, 200)
    end
    return root
end

function matmul()
    n = 300
    a = zeros(Float64, n * n)
    b = zeros(Float64, n * n)
    for i in 0:(n - 1), j in 0:(n - 1)
        a[i * n + j + 1] = Float64((i * j + i + 2 * j) % 7)
        b[i * n + j + 1] = Float64((3 * i + j * j) % 5)
    end
    c = zeros(Float64, n * n)
    for i in 0:(n - 1), j in 0:(n - 1)
        acc = 0.0
        for k in 0:(n - 1)
            acc = acc + a[i * n + k + 1] * b[k * n + j + 1]
        end
        c[i * n + j + 1] = acc
    end
    trace = 0.0
    total = 0.0
    for i in 0:(n - 1)
        trace = trace + c[i * n + i + 1]
        for j in 0:(n - 1)
            total = total + c[i * n + j + 1]
        end
    end
    return "trace " * repr(trace) * " total " * repr(total)
end

function regression()
    n = 2000000
    s0 = s1 = s2 = s3 = s4 = t0 = t1 = t2 = 0.0
    for i in 0:(n - 1)
        x = Float64(i) / Float64(n) * 2.0 - 1.0
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
    end
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
    return repr(c0) * " " * repr(c1) * " " * repr(c2)
end

function horner()
    degree = 50
    coefficients = [Float64((k % 5) - 2) for k in 0:degree]
    total = 0.0
    for i in 0:1999999
        x = Float64(i % 1000) / 1000.0
        value = 0.0
        for k in 0:degree
            value = value * x + coefficients[degree - k + 1]
        end
        total = total + value
    end
    return repr(total)
end

const KERNELS = Dict("simpson" => simpson, "bisect" => bisect, "matmul" => matmul, "regression" => regression, "horner" => horner)

if !@isdefined(KERNELS_LIBRARY_ONLY)
name = ARGS[1]
timed = "--timed" in ARGS
t0 = time_ns()
value = KERNELS[name]()
cold_ms = (time_ns() - t0) / 1.0e6
println("RESULT ", name, " ", value isa Float64 ? repr(value) : value)
if timed
    # A second call in the same process: the warm time, without process start or first-call compilation.
    t1 = time_ns()
    KERNELS[name]()
    warm_ms = (time_ns() - t1) / 1.0e6
    println("TIMING cold_ms ", round(cold_ms; digits=3), " warm_ms ", round(warm_ms; digits=3))
end
end
