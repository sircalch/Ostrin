# In-process repetitions of a kernel of benchmarks/julia/kernels.jl.
#
# Usage: julia --threads=1 run_v2.jl NAME K
# Runs the kernel K times in one process. Each repetition is timed with time_ns, so the times exclude process start.
# The first repetition includes the first-call compilation of the kernel. Prints one "RESULT <name> <value>" line (from
# the last repetition) and one "REP_MS t1 ... tK" line.
const KERNELS_LIBRARY_ONLY = true
include(joinpath(@__DIR__, "..", "..", "julia", "kernels.jl"))

name = ARGS[1]
repeats = length(ARGS) >= 2 ? parse(Int, ARGS[2]) : 1
fn = KERNELS[name]
times = Float64[]
value = nothing
for _ in 1:repeats
    t0 = time_ns()
    global value = fn()
    push!(times, (time_ns() - t0) / 1.0e6)
end
println("RESULT ", name, " ", value isa Float64 ? repr(value) : value)
println("REP_MS ", join(repr.(times), " "))
