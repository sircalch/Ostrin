export const allocationScope = "Ostrin managed allocation records; excludes allocation bytes, libc allocations and process RSS";

export function parseAllocationCounters(stderr) {
  const lines = stderr.replaceAll("\r\n", "\n").split("\n")
    .filter((line) => line.startsWith("ostrin memory:"));
  if (lines.length !== 1) throw new Error("expected exactly one native allocation report");
  const match = lines[0].match(/^ostrin memory: live_allocations=(0|[1-9]\d*) peak_allocations=(0|[1-9]\d*) total_allocations=(0|[1-9]\d*)$/);
  if (!match) throw new Error("malformed native allocation report");
  const [live, peak, total] = match.slice(1).map(Number);
  if (![live, peak, total].every(Number.isSafeInteger) || live > peak || peak > total) {
    throw new Error("invalid native allocation counter range");
  }
  if (live !== 0) throw new Error(`native workload left ${live} managed allocations alive`);
  return { live, peak, total, raw: lines[0] };
}

export function validateAllocationProfile(profile, iterations, outputSha256) {
  const errors = [];
  if (!profile || typeof profile !== "object") return ["native allocation profile is missing"];
  if (profile.scope !== allocationScope || profile.timingIncluded !== false) {
    errors.push("allocation scope must exclude bytes/RSS and instrumented timings");
  }
  if (!/^[a-f0-9]{64}$/.test(profile.executableSha256 || "")) errors.push("allocation executable hash is missing");
  if (!Array.isArray(profile.samples) || profile.samples.length !== iterations) {
    errors.push("allocation samples do not match iterations");
  }
  for (const [index, sample] of (Array.isArray(profile.samples) ? profile.samples : []).entries()) {
    try {
      const parsed = parseAllocationCounters(sample?.raw || "");
      if (["live", "peak", "total"].some((key) => sample[key] !== parsed[key])) {
        errors.push(`allocation sample ${index}: counters differ from raw report`);
      }
      if (sample.outputSha256 !== outputSha256) errors.push(`allocation sample ${index}: output identity differs`);
    } catch (error) {
      errors.push(`allocation sample ${index}: ${error.message}`);
    }
  }
  return errors;
}
