# Linear-algebra kernels for the Benchmark V2 linear-algebra study.
#
# Usage: julia linalg_kernels.jl KERNEL IMPL N K
#
# KERNEL is matmul, lu_solve or norms. IMPL is pure (loops over Matrix{Float64}, no library call) or lib (the library
# operations: `*`, `\` and `norm`, which call OpenBLAS / LAPACK). The inputs use the same formulas as the Ostrin programs
# and benchmarks/v2/linalg/reference.mjs. The kernel runs K times in this process; each repetition is timed with
# time_ns(), so compilation of the first call is part of repetition 1 and is reported separately by the runner.
#
# Prints one "RESULT <kernel> <numbers>" line (from the last repetition) and one "REP_MS t1 ... tK" line.

using LinearAlgebra
BLAS.set_num_threads(1)  # single-threaded library calls, as in the Python and Ostrin runs

function matmul_inputs(n)
    a = [((7i + 3j) % 11 - 5) / 4.0 for i in 0:n-1, j in 0:n-1]
    b = [((5i + 13j) % 9 - 4) / 8.0 for i in 0:n-1, j in 0:n-1]
    return a, b
end

function solve_inputs(n)
    d = [i == j ? 2.0 * n : ((3i + 5j) % 7 - 3) / 4.0 for i in 0:n-1, j in 0:n-1]
    xstar = [Float64(j % 9 - 4) for j in 0:n-1]
    b = d * xstar
    return d, b
end

norms_inputs(n) = [((7i + 3j) % 11 - 5) / 4.0 for i in 0:n-1, j in 0:n-1]

# ------------------------------------------------------------------ pure Julia (loops, no library)

function matmul_pure(a, b, n)
    total = 0.0
    for i in 1:n
        for j in 1:n
            acc = 0.0
            for k in 1:n
                acc += a[i, k] * b[k, j]
            end
            total += acc
        end
    end
    return total
end

function lu_solve_pure(d, b, n)
    m = copy(d)
    rhs = copy(b)
    for k in 1:n
        p = k
        best = abs(m[k, k])
        for i in k+1:n
            if abs(m[i, k]) > best
                best = abs(m[i, k])
                p = i
            end
        end
        if p != k
            for j in 1:n
                m[k, j], m[p, j] = m[p, j], m[k, j]
            end
            rhs[k], rhs[p] = rhs[p], rhs[k]
        end
        pivot = m[k, k]
        for i in k+1:n
            factor = m[i, k] / pivot
            m[i, k] = factor
            for j in k+1:n
                m[i, j] -= factor * m[k, j]
            end
            rhs[i] -= factor * rhs[k]
        end
    end
    x = zeros(Float64, n)
    for i in n:-1:1
        acc = rhs[i]
        for j in i+1:n
            acc -= m[i, j] * x[j]
        end
        x[i] = acc / m[i, i]
    end
    return sum(x)
end

function norms_pure(a, n)
    col = 0.0
    for j in 1:n
        s = 0.0
        for i in 1:n
            s += abs(a[i, j])
        end
        col = max(col, s)
    end
    row = 0.0
    for i in 1:n
        s = 0.0
        for j in 1:n
            s += abs(a[i, j])
        end
        row = max(row, s)
    end
    sq = 0.0
    for i in 1:n, j in 1:n
        sq += a[i, j] * a[i, j]
    end
    return (col + row, sqrt(sq))
end

# ------------------------------------------------------------------ Julia with the library (OpenBLAS / LAPACK)

matmul_lib(a, b) = sum(a * b)
lu_solve_lib(d, b) = sum(d \ b)
norms_lib(a) = (opnorm(a, 1) + opnorm(a, Inf), norm(a))

function run_kernel(kernel, impl, n, repeats)
    if kernel == "matmul"
        a, b = matmul_inputs(n)
        f = impl == "pure" ? () -> matmul_pure(a, b, n) : () -> matmul_lib(a, b)
    elseif kernel == "lu_solve"
        d, b = solve_inputs(n)
        f = impl == "pure" ? () -> lu_solve_pure(d, b, n) : () -> lu_solve_lib(d, b)
    elseif kernel == "norms"
        a = norms_inputs(n)
        f = impl == "pure" ? () -> norms_pure(a, n) : () -> norms_lib(a)
    else
        error("unknown kernel $kernel")
    end
    times = Float64[]
    value = nothing
    for _ in 1:repeats
        started = time_ns()
        value = f()
        push!(times, (time_ns() - started) / 1.0e6)
    end
    return value, times
end

function main(args)
    kernel, impl, n, repeats = args[1], args[2], parse(Int, args[3]), parse(Int, args[4])
    value, times = run_kernel(kernel, impl, n, repeats)
    numbers = value isa Tuple ? value : (value,)
    println("RESULT ", kernel, " ", join(string.(Float64.(collect(numbers))), " "))
    println("REP_MS ", join(string.(times), " "))
end

main(ARGS)
