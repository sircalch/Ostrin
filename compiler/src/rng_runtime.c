/* Runtime for Rng: xoshiro256** seeded through splitmix64. Mirrors
 * interpreter/rng.rs line by line so both backends produce the same stream.
 * Only integer operations and correctly-rounded IEEE arithmetic are used
 * (ostrin_dm_ln, from detmath_runtime.c, is built from + - * / alone); compile with -ffp-contract=off. */
typedef struct { uint64_t s[4]; } OstrinRng;

static uint64_t ostrin_splitmix64(uint64_t* x) {
    uint64_t z = (*x += 0x9E3779B97F4A7C15ULL);
    z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ULL;
    z = (z ^ (z >> 27)) * 0x94D049BB133111EBULL;
    return z ^ (z >> 31);
}

static OstrinRng* ostrin_rng_new(int64_t seed) {
    OstrinRng* r = (OstrinRng*)ostrin_alloc(sizeof(OstrinRng));
    if (!r) OSTRIN_OOM();
    uint64_t x = (uint64_t)seed;
    for (int i = 0; i < 4; i++) r->s[i] = ostrin_splitmix64(&x);
    return r;
}

static uint64_t ostrin_rotl(uint64_t x, int k) { return (x << k) | (x >> (64 - k)); }

static uint64_t ostrin_rng_u64(OstrinRng* r) {
    uint64_t* s = r->s;
    uint64_t result = ostrin_rotl(s[1] * 5, 7) * 9;
    uint64_t t = s[1] << 17;
    s[2] ^= s[0];
    s[3] ^= s[1];
    s[1] ^= s[2];
    s[0] ^= s[3];
    s[2] ^= t;
    s[3] = ostrin_rotl(s[3], 45);
    return result;
}

static double ostrin_rng_float(OstrinRng* r) {
    return (double)(ostrin_rng_u64(r) >> 11) * (1.0 / 9007199254740992.0);
}

static int64_t ostrin_rng_int(OstrinRng* r, int64_t lo, int64_t hi) {
    if (hi <= lo) OSTRIN_FAIL("next_int needs lo < hi");
    uint64_t range = (uint64_t)hi - (uint64_t)lo;
    uint64_t threshold = (0 - range) % range;
    uint64_t x;
    do { x = ostrin_rng_u64(r); } while (x < threshold);
    return (int64_t)((uint64_t)lo + x % range);
}

static double ostrin_rng_normal(OstrinRng* r) {
    for (;;) {
        double u = 2.0 * ostrin_rng_float(r) - 1.0;
        double v = 2.0 * ostrin_rng_float(r) - 1.0;
        double s = u * u + v * v;
        if (s > 0.0 && s < 1.0) return u * sqrt((-2.0 * ostrin_dm_ln(s)) / s);
    }
}
