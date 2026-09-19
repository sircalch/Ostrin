/* Deterministic elementary functions: a line-by-line mirror of
 * interpreter/detmath.rs. Only integer operations, floor, sqrt and the
 * correctly-rounded IEEE + - * / are used (never libm's sin/exp/pow), and the
 * program is compiled with -ffp-contract=off, so the results are bit-for-bit
 * the interpreter's on every platform. */
#define OSTRIN_DM_LN2 0.6931471805599453
#define OSTRIN_DM_LN2_HI 6.93147180369123816490e-01
#define OSTRIN_DM_LN2_LO 1.90821492927058770002e-10
#define OSTRIN_DM_INV_LN2 1.4426950408889634
#define OSTRIN_DM_LN10 2.302585092994046
#define OSTRIN_DM_PI 3.141592653589793
#define OSTRIN_DM_PIO2 1.5707963267948966
#define OSTRIN_DM_PIO4 0.7853981633974483
#define OSTRIN_DM_TWO_OVER_PI 0.6366197723675814
#define OSTRIN_DM_PIO2_1 1.57079632673412561417e+00
#define OSTRIN_DM_PIO2_1T 6.07710050650619224932e-11

static double ostrin_dm_pow2(int64_t k) {
    uint64_t bits = (uint64_t)(k + 1023) << 52;
    double d;
    memcpy(&d, &bits, sizeof d);
    return d;
}

static double ostrin_dm_exp(double x) {
    if (x != x) return x;
    if (x > 709.782712893384) return INFINITY;
    if (x < -745.1332191019411) return 0.0;
    double k = floor(x * OSTRIN_DM_INV_LN2 + 0.5);
    double r = (x - k * OSTRIN_DM_LN2_HI) - k * OSTRIN_DM_LN2_LO;
    double term = 1.0;
    double sum = 1.0;
    for (int n = 1; n <= 26; n++) {
        term = term * r / (double)n;
        sum = sum + term;
    }
    int64_t ki = (int64_t)k;
    int64_t half = ki / 2;
    return sum * ostrin_dm_pow2(half) * ostrin_dm_pow2(ki - half);
}

static double ostrin_dm_ln(double x) {
    if (x != x || x < 0.0) return NAN;
    if (x == 0.0) return -INFINITY;
    if (isinf(x)) return x;
    int64_t adjust = 0;
    if (x < 2.2250738585072014e-308) {
        x = x * 18014398509481984.0;
        adjust = -54;
    }
    uint64_t bits;
    memcpy(&bits, &x, sizeof bits);
    int64_t e = (int64_t)((bits >> 52) & 0x7FF) - 1023 + adjust;
    bits = (bits & 0x000FFFFFFFFFFFFFULL) | 0x3FF0000000000000ULL;
    double m;
    memcpy(&m, &bits, sizeof m);
    if (m > 1.4142135623730951) { m = m * 0.5; e += 1; }
    double z = (m - 1.0) / (m + 1.0);
    double z2 = z * z;
    double term = z;
    double sum = 0.0;
    for (int k = 0; k < 16; k++) {
        sum = sum + term / (double)(2 * k + 1);
        term = term * z2;
    }
    return (double)e * OSTRIN_DM_LN2 + 2.0 * sum;
}

static double ostrin_dm_log10(double x) { return ostrin_dm_ln(x) / OSTRIN_DM_LN10; }

static double ostrin_dm_sin_kernel(double r) {
    double r2 = r * r;
    double term = r;
    double sum = r;
    for (int i = 1; i <= 13; i++) {
        term = -term * r2 / (double)((2 * i) * (2 * i + 1));
        sum = sum + term;
    }
    return sum;
}

static double ostrin_dm_cos_kernel(double r) {
    double r2 = r * r;
    double term = 1.0;
    double sum = 1.0;
    for (int i = 1; i <= 13; i++) {
        term = -term * r2 / (double)((2 * i - 1) * (2 * i));
        sum = sum + term;
    }
    return sum;
}

/* Returns 0 (and leaves the outputs alone) when x is not finite or |x| > 1e6. */
static int ostrin_dm_reduce(double x, double* r, int64_t* q) {
    if (!isfinite(x) || fabs(x) > 1.0e6) return 0;
    double n = floor(x * OSTRIN_DM_TWO_OVER_PI + 0.5);
    *r = (x - n * OSTRIN_DM_PIO2_1) - n * OSTRIN_DM_PIO2_1T;
    int64_t ni = (int64_t)n;
    *q = ((ni % 4) + 4) % 4;
    return 1;
}

static double ostrin_dm_sin(double x) {
    double r; int64_t q;
    if (!ostrin_dm_reduce(x, &r, &q)) return NAN;
    switch (q) {
        case 0: return ostrin_dm_sin_kernel(r);
        case 1: return ostrin_dm_cos_kernel(r);
        case 2: return -ostrin_dm_sin_kernel(r);
        default: return -ostrin_dm_cos_kernel(r);
    }
}

static double ostrin_dm_cos(double x) {
    double r; int64_t q;
    if (!ostrin_dm_reduce(x, &r, &q)) return NAN;
    switch (q) {
        case 0: return ostrin_dm_cos_kernel(r);
        case 1: return -ostrin_dm_sin_kernel(r);
        case 2: return -ostrin_dm_cos_kernel(r);
        default: return ostrin_dm_sin_kernel(r);
    }
}

static double ostrin_dm_tan(double x) {
    double r; int64_t q;
    if (!ostrin_dm_reduce(x, &r, &q)) return NAN;
    double s = ostrin_dm_sin_kernel(r);
    double c = ostrin_dm_cos_kernel(r);
    return (q == 0 || q == 2) ? s / c : -c / s;
}

static double ostrin_dm_atan_series(double z) {
    double z2 = z * z;
    double term = z;
    double sum = z;
    for (int i = 1; i <= 27; i++) {
        term = -term * z2;
        sum = sum + term / (double)(2 * i + 1);
    }
    return sum;
}

static double ostrin_dm_atan_core(double a) {
    if (a > 0.4142135623730951) return OSTRIN_DM_PIO4 + ostrin_dm_atan_series((a - 1.0) / (a + 1.0));
    return ostrin_dm_atan_series(a);
}

static double ostrin_dm_atan(double x) {
    if (x != x) return x;
    double ax = x < 0.0 ? -x : x;
    double r = ax > 1.0 ? OSTRIN_DM_PIO2 - ostrin_dm_atan_core(1.0 / ax) : ostrin_dm_atan_core(ax);
    return x < 0.0 ? -r : r;
}

static double ostrin_dm_atan2(double y, double x) {
    if (y != y || x != x) return NAN;
    if (x > 0.0) return ostrin_dm_atan(y / x);
    if (x < 0.0) return y >= 0.0 ? ostrin_dm_atan(y / x) + OSTRIN_DM_PI : ostrin_dm_atan(y / x) - OSTRIN_DM_PI;
    if (y > 0.0) return OSTRIN_DM_PIO2;
    if (y < 0.0) return -OSTRIN_DM_PIO2;
    return 0.0;
}

static double ostrin_dm_asin(double x) {
    if (x != x || x > 1.0 || x < -1.0) return NAN;
    return ostrin_dm_atan2(x, sqrt(1.0 - x * x));
}

static double ostrin_dm_acos(double x) {
    if (x != x || x > 1.0 || x < -1.0) return NAN;
    return ostrin_dm_atan2(sqrt(1.0 - x * x), x);
}

static double ostrin_dm_sinh(double x) {
    if (x != x) return x;
    double ax = x < 0.0 ? -x : x;
    if (ax < 0.1) {
        double x2 = x * x;
        return x * (1.0 + x2 / 6.0 * (1.0 + x2 / 20.0 * (1.0 + x2 / 42.0 * (1.0 + x2 / 72.0 * (1.0 + x2 / 110.0)))));
    }
    return (ostrin_dm_exp(x) - ostrin_dm_exp(-x)) / 2.0;
}

static double ostrin_dm_cosh(double x) {
    if (x != x) return x;
    return (ostrin_dm_exp(x) + ostrin_dm_exp(-x)) / 2.0;
}

static double ostrin_dm_tanh(double x) {
    if (x != x) return x;
    if (x > 20.0) return 1.0;
    if (x < -20.0) return -1.0;
    return ostrin_dm_sinh(x) / ostrin_dm_cosh(x);
}

static double ostrin_dm_pow(double x, double y) {
    if (y == 0.0) return 1.0;
    if (x != x || y != y) return NAN;
    int y_is_integer = y == floor(y);
    if (y_is_integer && fabs(y) <= 1024.0) {
        double result = 1.0;
        double base = x;
        uint64_t e = (uint64_t)(y < 0.0 ? -y : y);
        while (e > 0) {
            if (e & 1) result = result * base;
            base = base * base;
            e >>= 1;
        }
        return y < 0.0 ? 1.0 / result : result;
    }
    if (x == 0.0) return y > 0.0 ? 0.0 : INFINITY;
    if (x < 0.0) {
        if (!y_is_integer) return NAN;
        int odd = floor(y / 2.0) * 2.0 != y;
        double magnitude = ostrin_dm_exp(y * ostrin_dm_ln(-x));
        return odd ? -magnitude : magnitude;
    }
    return ostrin_dm_exp(y * ostrin_dm_ln(x));
}

/* Single-precision wrappers: compute in double, round once. */
#define OSTRIN_DM_F32(name) static float ostrin_dm_##name##f(float x) { return (float)ostrin_dm_##name((double)x); }
OSTRIN_DM_F32(sin) OSTRIN_DM_F32(cos) OSTRIN_DM_F32(tan) OSTRIN_DM_F32(asin) OSTRIN_DM_F32(acos) OSTRIN_DM_F32(atan)
OSTRIN_DM_F32(sinh) OSTRIN_DM_F32(cosh) OSTRIN_DM_F32(tanh) OSTRIN_DM_F32(exp) OSTRIN_DM_F32(ln) OSTRIN_DM_F32(log10)
static float ostrin_dm_powf(float a, float b) { return (float)ostrin_dm_pow((double)a, (double)b); }
static float ostrin_dm_atan2f(float a, float b) { return (float)ostrin_dm_atan2((double)a, (double)b); }
