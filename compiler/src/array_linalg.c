/* Regression, linear solves, histograms and normal distribution for
 * Array<Float>, appended to its runtime (@N@ = Array_Float). Mirrors
 * interpreter/regress.rs operation for operation. */
static Array_Int* Array_Int_alloc(int64_t rank, const int64_t* shape);

static @N@* @N@_vector(int64_t n, const double* values) {
    int64_t shape[1] = { n };
    @N@* r = @N@_alloc(1, shape);
    memcpy(r->data, values, sizeof(double) * (size_t)n);
    return r;
}

static void @N@_solve_raw(double* m, double* v, int64_t n, double* x) {
    for (int64_t col = 0; col < n; col++) {
        int64_t piv = col;
        double best = fabs(m[col * n + col]);
        for (int64_t r = col + 1; r < n; r++) {
            double candidate = fabs(m[r * n + col]);
            if (candidate > best) { best = candidate; piv = r; }
        }
        if (best == 0.0) OSTRIN_FAIL("singular matrix");
        if (piv != col) {
            for (int64_t c = 0; c < n; c++) { double t = m[piv * n + c]; m[piv * n + c] = m[col * n + c]; m[col * n + c] = t; }
            double t = v[piv]; v[piv] = v[col]; v[col] = t;
        }
        for (int64_t r = col + 1; r < n; r++) {
            double f = m[r * n + col] / m[col * n + col];
            for (int64_t c = col; c < n; c++) m[r * n + c] = m[r * n + c] - f * m[col * n + c];
            v[r] = v[r] - f * v[col];
        }
    }
    for (int64_t i = n - 1; i >= 0; i--) {
        double s = v[i];
        for (int64_t j = i + 1; j < n; j++) s = s - m[i * n + j] * x[j];
        x[i] = s / m[i * n + i];
    }
}

static @N@* @N@_solve(@N@* a, @N@* b) {
    if (a->rank != 2 || a->shape[0] != a->shape[1] || b->rank != 1 || b->shape[0] != a->shape[0]) OSTRIN_FAIL("solve needs an (n, n) matrix and a vector of length n");
    int64_t n = a->shape[0];
    double* m = (double*)malloc(sizeof(double) * (size_t)(n * n));
    double* v = (double*)malloc(sizeof(double) * (size_t)n);
    double* x = (double*)malloc(sizeof(double) * (size_t)n);
    if (!m || !v || !x) OSTRIN_OOM();
    memcpy(m, a->data, sizeof(double) * (size_t)(n * n));
    memcpy(v, b->data, sizeof(double) * (size_t)n);
    @N@_solve_raw(m, v, n, x);
    @N@* r = @N@_vector(n, x);
    free(m); free(v); free(x);
    return r;
}

static @N@* @N@_linfit(@N@* x, @N@* y) {
    if (x->rank != 1 || y->rank != 1 || x->size != y->size || x->size < 2) OSTRIN_FAIL("linfit needs two one-dimensional arrays of the same length (at least 2)");
    double n = (double)x->size;
    double sum_x = x->data[0];
    double sum_y = y->data[0];
    for (int64_t i = 1; i < x->size; i++) { sum_x = sum_x + x->data[i]; sum_y = sum_y + y->data[i]; }
    double mx = sum_x / n, my = sum_y / n;
    double sxx = 0.0, sxy = 0.0, syy = 0.0;
    for (int64_t i = 0; i < x->size; i++) {
        double dx = x->data[i] - mx, dy = y->data[i] - my;
        sxx = sxx + dx * dx;
        sxy = sxy + dx * dy;
        syy = syy + dy * dy;
    }
    if (sxx == 0.0) OSTRIN_FAIL("linfit needs x values that are not all equal");
    double slope = sxy / sxx;
    double intercept = my - slope * mx;
    double r2 = syy == 0.0 ? 1.0 : sxy * sxy / (sxx * syy);
    double out[3] = { slope, intercept, r2 };
    return @N@_vector(3, out);
}

static @N@* @N@_polyfit(@N@* x, @N@* y, int64_t degree) {
    if (x->rank != 1 || y->rank != 1 || x->size != y->size) OSTRIN_FAIL("polyfit needs two one-dimensional arrays of the same length");
    if (degree < 0 || degree + 1 > x->size) OSTRIN_FAIL("polyfit needs 0 <= degree < number of points");
    int64_t d = degree, k = d + 1, n = x->size;
    double* powers = (double*)malloc(sizeof(double) * (size_t)(n * (2 * d + 1)));
    double* a = (double*)malloc(sizeof(double) * (size_t)(k * k));
    double* b = (double*)malloc(sizeof(double) * (size_t)k);
    double* sol = (double*)malloc(sizeof(double) * (size_t)k);
    if (!powers || !a || !b || !sol) OSTRIN_OOM();
    int64_t stride = 2 * d + 1;
    for (int64_t i = 0; i < n; i++) {
        powers[i * stride] = 1.0;
        for (int64_t p = 1; p <= 2 * d; p++) powers[i * stride + p] = powers[i * stride + p - 1] * x->data[i];
    }
    for (int64_t r = 0; r < k; r++) {
        for (int64_t c = 0; c < k; c++) {
            double acc = 0.0;
            for (int64_t i = 0; i < n; i++) acc = acc + powers[i * stride + r + c];
            a[r * k + c] = acc;
        }
        double acc = 0.0;
        for (int64_t i = 0; i < n; i++) acc = acc + y->data[i] * powers[i * stride + r];
        b[r] = acc;
    }
    @N@_solve_raw(a, b, k, sol);
    @N@* r = @N@_vector(k, sol);
    free(powers); free(a); free(b); free(sol);
    return r;
}

static double @N@_horner(@N@* c, double x) {
    double r = c->data[c->size - 1];
    for (int64_t i = c->size - 2; i >= 0; i--) r = r * x + c->data[i];
    return r;
}
static double @N@_polyval(@N@* c, double x) { return @N@_horner(c, x); }
static @N@* @N@_polyval_array(@N@* c, @N@* xs) {
    @N@* r = @N@_alloc(xs->rank, xs->shape);
    for (int64_t i = 0; i < xs->size; i++) r->data[i] = @N@_horner(c, xs->data[i]);
    return r;
}

static Array_Int* @N@_histogram(@N@* a, int64_t bins, double lo, double hi) {
    if (bins < 1 || !(hi > lo)) OSTRIN_FAIL("histogram needs bins >= 1 and lo < hi");
    int64_t shape[1] = { bins };
    Array_Int* r = Array_Int_alloc(1, shape);
    for (int64_t i = 0; i < bins; i++) r->data[i] = 0;
    double width = (hi - lo) / (double)bins;
    for (int64_t i = 0; i < a->size; i++) {
        double v = a->data[i];
        if (!(v >= lo && v <= hi)) continue;
        int64_t index = (int64_t)floor((v - lo) / width);
        if (index >= bins) index = bins - 1;
        r->data[index] += 1;
    }
    return r;
}

static @N@* @N@_norm_map(@N@* a, double mu, double sigma, int cdf) {
    if (!(sigma > 0.0)) OSTRIN_FAIL("the normal distribution needs sigma > 0");
    @N@* r = @N@_alloc(a->rank, a->shape);
    for (int64_t i = 0; i < a->size; i++) r->data[i] = cdf ? ostrin_dm_norm_cdf(a->data[i], mu, sigma) : ostrin_dm_norm_pdf(a->data[i], mu, sigma);
    return r;
}
