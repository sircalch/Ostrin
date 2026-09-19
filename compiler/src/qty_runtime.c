/* Runtime support for Quantity values, mirroring the interpreter's own
 * unit handling (unit_factor / resolve_unit_factor / convert in
 * interpreter/mod.rs): the dimension of a quantity is a compile-time fact,
 * but its unit is a runtime string, exactly like Value::Quantity. */
typedef struct { double v; const char* u; } Qty;

static double ostrin_unit_factor(const char* a) {
    static const struct { const char* s; double f; } table[] = {
        {"m", 1.0}, {"s", 1.0}, {"kg", 1.0}, {"K", 1.0}, {"A", 1.0}, {"mol", 1.0},
        {"cd", 1.0}, {"USD", 1.0}, {"bit", 1.0}, {"C", 1.0}, {"atm", 1.0}, {"Pa", 1.0},
        {"nm", 1e-9}, {"km", 1000.0}, {"cm", 0.01}, {"mm", 0.001}, {"ms", 0.001},
        {"min", 60.0}, {"h", 3600.0}, {"g", 0.001}, {"mg", 1e-6}, {"mmol", 0.001},
        {"L", 0.001}, {"EUR", 1.0}, {"byte", 8.0}
    };
    for (size_t i = 0; i < sizeof table / sizeof table[0]; i++) {
        if (strcmp(table[i].s, a) == 0) return table[i].f;
    }
    fprintf(stderr, "runtime error: unknown unit '%s'\n", a);
    exit(1);
}

static double ostrin_unit_expr_factor(const char* expr) {
    double result = 1.0;
    char op = '*';
    const char* p = expr;
    for (;;) {
        char atom[64];
        size_t n = 0;
        while (*p && *p != '*' && *p != '/' && *p != '^') { if (n < 63) atom[n++] = *p; p++; }
        atom[n] = 0;
        if (n == 0) { fprintf(stderr, "runtime error: malformed unit expression '%s'\n", expr); exit(1); }
        double factor = ostrin_unit_factor(atom);
        if (*p == '^') {
            p++;
            char digits[16];
            size_t k = 0;
            while ((*p >= '0' && *p <= '9') || *p == '-') { if (k < 15) digits[k++] = *p; p++; }
            digits[k] = 0;
            int e = atoi(digits);
            double r = 1.0;
            for (int i = 0; i < (e < 0 ? -e : e); i++) r *= factor;
            factor = e < 0 ? 1.0 / r : r;
        }
        result = op == '*' ? result * factor : result / factor;
        if (*p == 0) break;
        if (*p == '*' || *p == '/') { op = *p; p++; }
        else { fprintf(stderr, "runtime error: malformed unit expression '%s'\n", expr); exit(1); }
    }
    return result;
}

static double ostrin_convert(double v, const char* from, const char* to) {
    if (strcmp(from, to) == 0) return v;
    return v * ostrin_unit_expr_factor(from) / ostrin_unit_expr_factor(to);
}

static const char* ostrin_unit_cat(const char* a, const char* op, const char* b) {
    size_t n = strlen(a) + strlen(op) + strlen(b) + 1;
    char* out = (char*)ostrin_alloc(n);
    if (!out) { fprintf(stderr, "ostrin: out of memory\n"); exit(1); }
    snprintf(out, n, "%s%s%s", a, op, b);
    return out;
}

static Qty ostrin_qty_add(Qty a, Qty b) { Qty r = { a.v + ostrin_convert(b.v, b.u, a.u), a.u }; return r; }
static Qty ostrin_qty_sub(Qty a, Qty b) { Qty r = { a.v - ostrin_convert(b.v, b.u, a.u), a.u }; return r; }
static Qty ostrin_qty_mul(Qty a, Qty b) { Qty r = { a.v * b.v, ostrin_unit_cat(a.u, "*", b.u) }; return r; }
static Qty ostrin_qty_div(Qty a, Qty b) { Qty r = { a.v / b.v, ostrin_unit_cat(a.u, "/", b.u) }; return r; }
static double ostrin_qty_ratio(Qty a, Qty b) { return a.v / ostrin_convert(b.v, b.u, a.u); }
static Qty ostrin_qty_scale_mul(Qty a, double s) { Qty r = { a.v * s, a.u }; return r; }
static Qty ostrin_qty_scale_div(Qty a, double s) { Qty r = { a.v / s, a.u }; return r; }
static Qty ostrin_scalar_div_qty(double s, Qty a) { Qty r = { s / a.v, ostrin_unit_cat("1/", "", a.u) }; return r; }
static int ostrin_qty_cmp(Qty a, Qty b) {
    double c = ostrin_convert(b.v, b.u, a.u);
    return a.v < c ? -1 : (a.v > c ? 1 : 0);
}

static void ostrin_print_qty(Qty q) {
    char buf[64];
    ostrin_fmt_double(q.v, buf, sizeof buf);
    printf("%s %s\n", buf, q.u);
}

static const char* ostrin_qty_to_string(Qty q) {
    char buf[64];
    ostrin_fmt_double(q.v, buf, sizeof buf);
    return ostrin_unit_cat(ostrin_unit_cat(buf, " ", ""), q.u, "");
}

