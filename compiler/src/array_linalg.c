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

static double @N@_norm(@N@* a) {
    double acc = 0.0;
    for (int64_t i = 0; i < a->size; i++) acc = acc + a->data[i] * a->data[i];
    return sqrt(acc);
}

static @N@* @N@_eigvals(@N@* a) {
    if (a->rank != 2 || a->shape[0] != a->shape[1]) OSTRIN_FAIL("eigvals needs a square (n, n) matrix");
    int64_t n = a->shape[0];
    for (int64_t i = 0; i < n; i++)
        for (int64_t j = i + 1; j < n; j++)
            if (fabs(a->data[i * n + j] - a->data[j * n + i]) > 1e-9 * (1.0 + fabs(a->data[i * n + j]))) OSTRIN_FAIL("eigvals needs a symmetric matrix");
    double* m = (double*)ostrin_alloc(sizeof(double) * (size_t)(n * n));
    if (!m) OSTRIN_OOM();
    memcpy(m, a->data, sizeof(double) * (size_t)(n * n));
    for (int sweep = 0; sweep < 100; sweep++) {
        bool rotated = false;
        for (int64_t p = 0; p < n; p++) {
            for (int64_t q = p + 1; q < n; q++) {
                double apq = m[p * n + q];
                if (fabs(apq) <= 1e-15 * (fabs(m[p * n + p]) + fabs(m[q * n + q]))) continue;
                rotated = true;
                double theta = (m[q * n + q] - m[p * n + p]) / (2.0 * apq);
                double sign = theta < 0.0 ? -1.0 : 1.0;
                double t = sign / (fabs(theta) + sqrt(theta * theta + 1.0));
                double c = 1.0 / sqrt(t * t + 1.0);
                double s = t * c;
                for (int64_t k = 0; k < n; k++) {
                    double akp = m[k * n + p], akq = m[k * n + q];
                    m[k * n + p] = c * akp - s * akq;
                    m[k * n + q] = s * akp + c * akq;
                }
                for (int64_t k = 0; k < n; k++) {
                    double apk = m[p * n + k], aqk = m[q * n + k];
                    m[p * n + k] = c * apk - s * aqk;
                    m[q * n + k] = s * apk + c * aqk;
                }
            }
        }
        if (!rotated) break;
    }
    double* values = (double*)ostrin_alloc(sizeof(double) * (size_t)n);
    if (!values) OSTRIN_OOM();
    for (int64_t i = 0; i < n; i++) values[i] = m[i * n + i];
    for (int64_t i = 1; i < n; i++) {
        int64_t j = i;
        while (j > 0 && values[j - 1] > values[j]) { double t = values[j - 1]; values[j - 1] = values[j]; values[j] = t; j--; }
    }
    @N@* r = @N@_vector(n, values);
    ostrin_free(m); ostrin_free(values);
    return r;
}

static double @N@_det_raw(double* m, int64_t n) {
    double det = 1.0;
    for (int64_t col = 0; col < n; col++) {
        int64_t piv = col;
        double best = fabs(m[col * n + col]);
        for (int64_t r = col + 1; r < n; r++) {
            double candidate = fabs(m[r * n + col]);
            if (candidate > best) { best = candidate; piv = r; }
        }
        if (best == 0.0) return 0.0;
        if (piv != col) {
            for (int64_t c = 0; c < n; c++) { double t = m[piv * n + c]; m[piv * n + c] = m[col * n + c]; m[col * n + c] = t; }
            det = -det;
        }
        det = det * m[col * n + col];
        for (int64_t r = col + 1; r < n; r++) {
            double f = m[r * n + col] / m[col * n + col];
            for (int64_t c = col; c < n; c++) m[r * n + c] = m[r * n + c] - f * m[col * n + c];
        }
    }
    return det;
}

static void @N@_square_check(@N@* a, const char* what) {
    if (a->rank != 2 || a->shape[0] != a->shape[1]) {
        char msg[96];
        snprintf(msg, sizeof msg, "%s needs a square (n, n) matrix", what);
        OSTRIN_FAIL(msg);
    }
}

static double @N@_det(@N@* a) {
    @N@_square_check(a, "det");
    int64_t n = a->shape[0];
    double* m = (double*)ostrin_alloc(sizeof(double) * (size_t)(n * n));
    if (!m) OSTRIN_OOM();
    memcpy(m, a->data, sizeof(double) * (size_t)(n * n));
    double d = @N@_det_raw(m, n);
    ostrin_free(m);
    return d;
}

static double @N@_trace(@N@* a) {
    @N@_square_check(a, "trace");
    int64_t n = a->shape[0];
    double acc = 0.0;
    for (int64_t i = 0; i < n; i++) acc = acc + a->data[i * n + i];
    return acc;
}

static @N@* @N@_inv(@N@* a) {
    @N@_square_check(a, "inv");
    int64_t n = a->shape[0];
    int64_t shape[2] = { n, n };
    @N@* r = @N@_alloc(2, shape);
    double* m = (double*)ostrin_alloc(sizeof(double) * (size_t)(n * n));
    double* v = (double*)ostrin_alloc(sizeof(double) * (size_t)n);
    double* x = (double*)ostrin_alloc(sizeof(double) * (size_t)n);
    if (!m || !v || !x) OSTRIN_OOM();
    for (int64_t j = 0; j < n; j++) {
        memcpy(m, a->data, sizeof(double) * (size_t)(n * n));
        for (int64_t i = 0; i < n; i++) v[i] = 0.0;
        v[j] = 1.0;
        @N@_solve_raw(m, v, n, x);
        for (int64_t i = 0; i < n; i++) r->data[i * n + j] = x[i];
    }
    ostrin_free(m); ostrin_free(v); ostrin_free(x);
    return r;
}

static @N@* @N@_eye(int64_t n) {
    if (n < 1) OSTRIN_FAIL("eye needs n >= 1");
    int64_t shape[2] = { n, n };
    @N@* r = @N@_alloc(2, shape);
    for (int64_t i = 0; i < n * n; i++) r->data[i] = 0.0;
    for (int64_t i = 0; i < n; i++) r->data[i * n + i] = 1.0;
    return r;
}

static @N@* @N@_solve(@N@* a, @N@* b) {
    if (a->rank != 2 || a->shape[0] != a->shape[1] || b->rank != 1 || b->shape[0] != a->shape[0]) OSTRIN_FAIL("solve needs an (n, n) matrix and a vector of length n");
    int64_t n = a->shape[0];
    double* m = (double*)ostrin_alloc(sizeof(double) * (size_t)(n * n));
    double* v = (double*)ostrin_alloc(sizeof(double) * (size_t)n);
    double* x = (double*)ostrin_alloc(sizeof(double) * (size_t)n);
    if (!m || !v || !x) OSTRIN_OOM();
    memcpy(m, a->data, sizeof(double) * (size_t)(n * n));
    memcpy(v, b->data, sizeof(double) * (size_t)n);
    @N@_solve_raw(m, v, n, x);
    @N@* r = @N@_vector(n, x);
    ostrin_free(m); ostrin_free(v); ostrin_free(x);
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
    double* powers = (double*)ostrin_alloc(sizeof(double) * (size_t)(n * (2 * d + 1)));
    double* a = (double*)ostrin_alloc(sizeof(double) * (size_t)(k * k));
    double* b = (double*)ostrin_alloc(sizeof(double) * (size_t)k);
    double* sol = (double*)ostrin_alloc(sizeof(double) * (size_t)k);
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
    ostrin_free(powers); ostrin_free(a); ostrin_free(b); ostrin_free(sol);
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
