/* Statistics for Array<Float> / Array<Float32>, appended to the array runtime
 * (same placeholders and OSTRIN_* macros, plus OSTRIN_SQRT). Mirrors the `stats`
 * functions of interpreter/array.rs operation for operation: two-pass variance,
 * stable merge sort, linear-interpolation percentiles. */
static @T@ @N@_mean_value(@N@* a) { return OSTRIN_DIV(@N@_sum(a), (@T@)a->size); }

static @T@ @N@_var_ddof(@N@* a, int64_t ddof) {
    int64_t n = a->size;
    if (n - ddof < 1) OSTRIN_FAIL("not enough elements for this variance");
    @T@ m = @N@_mean_value(a);
    @T@ acc = 0;
    for (int64_t i = 0; i < n; i++) {
        @T@ d = OSTRIN_SUB(a->data[i], m);
        acc = OSTRIN_ADD(acc, OSTRIN_MUL(d, d));
    }
    return OSTRIN_DIV(acc, (@T@)(n - ddof));
}
static @T@ @N@_var(@N@* a) { return @N@_var_ddof(a, 0); }
static @T@ @N@_std(@N@* a) { return OSTRIN_SQRT(@N@_var_ddof(a, 0)); }
static @T@ @N@_sample_var(@N@* a) { return @N@_var_ddof(a, 1); }
static @T@ @N@_sample_std(@N@* a) { return OSTRIN_SQRT(@N@_var_ddof(a, 1)); }

static @T@ @N@_median(@N@* a) {
    @T@* s = @N@_sorted_flat(a);
    int64_t n = a->size;
    @T@ r = (n % 2 == 1) ? s[n / 2] : OSTRIN_DIV(OSTRIN_ADD(s[n / 2 - 1], s[n / 2]), (@T@)2);
    free(s);
    return r;
}

static @T@ @N@_percentile(@N@* a, double p) {
    if (!(p >= 0.0 && p <= 100.0)) OSTRIN_FAIL("percentile needs 0 <= p <= 100");
    @T@* s = @N@_sorted_flat(a);
    int64_t n = a->size;
    double pos = p / 100.0 * (double)(n - 1);
    int64_t lo = (int64_t)pos;
    double frac = pos - (double)lo;
    int64_t hi = lo + 1 < n ? lo + 1 : n - 1;
    @T@ r = OSTRIN_ADD(s[lo], OSTRIN_MUL(OSTRIN_SUB(s[hi], s[lo]), (@T@)frac));
    free(s);
    return r;
}

static @T@ @N@_cov(@N@* a, @N@* b) {
    if (a->rank != 1 || b->rank != 1 || a->size != b->size) OSTRIN_FAIL("cov needs two one-dimensional arrays of the same length");
    @T@ ma = @N@_mean_value(a);
    @T@ mb = @N@_mean_value(b);
    @T@ acc = 0;
    for (int64_t i = 0; i < a->size; i++) acc = OSTRIN_ADD(acc, OSTRIN_MUL(OSTRIN_SUB(a->data[i], ma), OSTRIN_SUB(b->data[i], mb)));
    return OSTRIN_DIV(acc, (@T@)a->size);
}

static @T@ @N@_corr(@N@* a, @N@* b) {
    @T@ c = @N@_cov(a, b);
    return OSTRIN_DIV(c, OSTRIN_MUL(OSTRIN_SQRT(@N@_var_ddof(a, 0)), OSTRIN_SQRT(@N@_var_ddof(b, 0))));
}
