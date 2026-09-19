/* String methods, mirroring interpreter/strings.rs: ASCII whitespace and case mapping only,
 * `length` counts code points. Every result is a fresh malloc'd string (never freed, like the
 * rest of this backend's heap values). */
static char* ostrin_s_dup(const char* s, size_t n) {
    char* r = (char*)ostrin_alloc(n + 1);
    if (!r) OSTRIN_OOM();
    memcpy(r, s, n);
    r[n] = 0;
    return r;
}

static bool ostrin_s_space(char c) {
    return c == ' ' || c == '\t' || c == '\n' || c == '\f' || c == '\r';
}

static int64_t ostrin_s_length(const char* s) {
    int64_t n = 0;
    for (; *s; s++) if (((unsigned char)*s & 0xC0) != 0x80) n++;
    return n;
}

static const char* ostrin_s_trim(const char* s) {
    while (*s && ostrin_s_space(*s)) s++;
    size_t n = strlen(s);
    while (n > 0 && ostrin_s_space(s[n - 1])) n--;
    return ostrin_s_dup(s, n);
}

static const char* ostrin_s_upper(const char* s) {
    size_t n = strlen(s);
    char* r = ostrin_s_dup(s, n);
    for (size_t i = 0; i < n; i++) if (r[i] >= 'a' && r[i] <= 'z') r[i] = (char)(r[i] - 32);
    return r;
}

static const char* ostrin_s_lower(const char* s) {
    size_t n = strlen(s);
    char* r = ostrin_s_dup(s, n);
    for (size_t i = 0; i < n; i++) if (r[i] >= 'A' && r[i] <= 'Z') r[i] = (char)(r[i] + 32);
    return r;
}

static bool ostrin_s_starts_with(const char* s, const char* p) {
    return strncmp(s, p, strlen(p)) == 0;
}

static bool ostrin_s_ends_with(const char* s, const char* p) {
    size_t n = strlen(s), m = strlen(p);
    return m <= n && memcmp(s + n - m, p, m) == 0;
}

static const char* ostrin_s_replace(const char* s, const char* from, const char* to) {
    size_t fl = strlen(from), tl = strlen(to);
    if (fl == 0) OSTRIN_FAIL("'replace' needs a non-empty pattern");
    size_t count = 0;
    for (const char* p = s; (p = strstr(p, from)); p += fl) count++;
    size_t n = strlen(s);
    char* r = (char*)ostrin_alloc(n + count * (tl > fl ? tl - fl : 0) + 1);
    if (!r) OSTRIN_OOM();
    char* w = r;
    const char* p = s;
    const char* q;
    while ((q = strstr(p, from))) {
        memcpy(w, p, (size_t)(q - p)); w += q - p;
        memcpy(w, to, tl); w += tl;
        p = q + fl;
    }
    strcpy(w, p);
    return r;
}

static const char* ostrin_s_path_join(const char* left, const char* right) {
    size_t l = strlen(left), r = strlen(right);
    while (l > 0 && (left[l - 1] == '/' || left[l - 1] == '\\')) l--;
    size_t start = 0;
    while (start < r && (right[start] == '/' || right[start] == '\\')) start++;
    size_t n = l + (l > 0 && start < r ? 1 : 0) + (r - start);
    char* out = (char*)ostrin_alloc(n + 1);
    if (!out) OSTRIN_OOM();
    size_t p = 0;
    if (l > 0) { memcpy(out + p, left, l); p += l; }
    if (l > 0 && start < r) out[p++] = '/';
    if (start < r) { memcpy(out + p, right + start, r - start); p += r - start; }
    out[p] = 0;
    return out;
}

static const char** ostrin_s_split(const char* s, const char* sep, int64_t* out_n) {
    size_t sl = strlen(sep);
    if (sl == 0) OSTRIN_FAIL("'split' needs a non-empty separator");
    int64_t count = 1;
    for (const char* p = s; (p = strstr(p, sep)); p += sl) count++;
    const char** items = (const char**)ostrin_alloc(sizeof(char*) * (size_t)count);
    if (!items) OSTRIN_OOM();
    int64_t i = 0;
    const char* p = s;
    const char* q;
    while ((q = strstr(p, sep))) {
        items[i++] = ostrin_s_dup(p, (size_t)(q - p));
        p = q + sl;
    }
    items[i++] = ostrin_s_dup(p, strlen(p));
    *out_n = count;
    return items;
}

static const char** ostrin_s_lines(const char* s, int64_t* out_n) {
    int64_t count = 0;
    for (const char* p = s; *p; ) {
        const char* q = strchr(p, '\n');
        count++;
        if (!q) break;
        p = q + 1;
    }
    const char** items = (const char**)ostrin_alloc(sizeof(char*) * (size_t)(count > 0 ? count : 1));
    if (!items) OSTRIN_OOM();
    int64_t i = 0;
    for (const char* p = s; *p; ) {
        const char* q = strchr(p, '\n');
        size_t n = q ? (size_t)(q - p) : strlen(p);
        if (q && n > 0 && p[n - 1] == '\r') n--;
        items[i++] = ostrin_s_dup(p, n);
        if (!q) break;
        p = q + 1;
    }
    *out_n = count;
    return items;
}

static const char* ostrin_s_join(const char** items, int64_t n, const char* sep) {
    size_t sl = strlen(sep), total = 1;
    for (int64_t i = 0; i < n; i++) total += strlen(items[i]) + (i ? sl : 0);
    char* r = (char*)ostrin_alloc(total);
    if (!r) OSTRIN_OOM();
    char* w = r;
    for (int64_t i = 0; i < n; i++) {
        if (i) { memcpy(w, sep, sl); w += sl; }
        size_t l = strlen(items[i]);
        memcpy(w, items[i], l); w += l;
    }
    *w = 0;
    return r;
}

static bool ostrin_s_ieq(const char* s, const char* lower) {
    for (; *lower; s++, lower++) {
        char c = *s;
        if (c >= 'A' && c <= 'Z') c = (char)(c + 32);
        if (c != *lower) return false;
    }
    return *s == 0;
}

/* 0 = valid float literal (Rust's grammar), 1 = empty, 2 = invalid. */
static int ostrin_s_float_check(const char* s) {
    if (!*s) return 1;
    const char* p = s;
    if (*p == '+' || *p == '-') p++;
    if (ostrin_s_ieq(p, "inf") || ostrin_s_ieq(p, "infinity") || ostrin_s_ieq(p, "nan")) return 0;
    int digits = 0;
    while (*p >= '0' && *p <= '9') { p++; digits++; }
    if (*p == '.') {
        p++;
        while (*p >= '0' && *p <= '9') { p++; digits++; }
    }
    if (digits == 0) return 2;
    if (*p == 'e' || *p == 'E') {
        p++;
        if (*p == '+' || *p == '-') p++;
        int exp_digits = 0;
        while (*p >= '0' && *p <= '9') { p++; exp_digits++; }
        if (exp_digits == 0) return 2;
    }
    return *p == 0 ? 0 : 2;
}

/* `parse_csv`, mirroring `parse_csv` in interpreter/strings.rs. Returns the rows (each an array
 * of fields) with their counts. */
typedef struct { char* buf; size_t len, cap; } OstrinSBuf;
static void ostrin_sbuf_push(OstrinSBuf* b, char c) {
    if (b->len + 1 >= b->cap) {
        b->cap = b->cap ? b->cap * 2 : 16;
        b->buf = (char*)ostrin_realloc(b->buf, b->cap);
        if (!b->buf) OSTRIN_OOM();
    }
    b->buf[b->len++] = c;
}
static const char* ostrin_sbuf_take(OstrinSBuf* b) {
    char* r = ostrin_s_dup(b->buf ? b->buf : "", b->len);
    b->len = 0;
    return r;
}
typedef struct { const char** items; int64_t n, cap; } OstrinSRow;
static void ostrin_srow_push(OstrinSRow* r, const char* s) {
    if (r->n >= r->cap) {
        r->cap = r->cap ? r->cap * 2 : 4;
        r->items = (const char**)ostrin_realloc((void*)r->items, sizeof(char*) * (size_t)r->cap);
        if (!r->items) OSTRIN_OOM();
    }
    r->items[r->n++] = s;
}
static const char*** ostrin_s_csv(const char* s, int64_t* out_rows, int64_t** out_cols) {
    const char*** rows = NULL; int64_t* cols = NULL; int64_t nrows = 0, cap = 0;
    OstrinSRow row = {0}; OstrinSBuf field = {0};
    bool quoted = false, touched = false;
    for (const char* p = s; *p; p++) {
        char c = *p;
        if (quoted) {
            if (c == '"') {
                if (p[1] == '"') { ostrin_sbuf_push(&field, '"'); p++; } else quoted = false;
            } else ostrin_sbuf_push(&field, c);
            continue;
        }
        if (c == '"') { quoted = true; touched = true; }
        else if (c == ',') { ostrin_srow_push(&row, ostrin_sbuf_take(&field)); touched = true; }
        else if (c == '\r' && p[1] == '\n') { }
        else if (c == '\n') {
            if (touched) {
                ostrin_srow_push(&row, ostrin_sbuf_take(&field));
                if (nrows >= cap) {
                    cap = cap ? cap * 2 : 8;
                    rows = (const char***)ostrin_realloc((void*)rows, sizeof(void*) * (size_t)cap);
                    cols = (int64_t*)ostrin_realloc(cols, sizeof(int64_t) * (size_t)cap);
                    if (!rows || !cols) OSTRIN_OOM();
                }
                rows[nrows] = row.items; cols[nrows] = row.n; nrows++;
                row.items = NULL; row.n = 0; row.cap = 0;
            }
            touched = false;
        } else { ostrin_sbuf_push(&field, c); touched = true; }
    }
    if (touched) {
        ostrin_srow_push(&row, ostrin_sbuf_take(&field));
        if (nrows >= cap) {
            cap = cap ? cap * 2 : 8;
            rows = (const char***)ostrin_realloc((void*)rows, sizeof(void*) * (size_t)cap);
            cols = (int64_t*)ostrin_realloc(cols, sizeof(int64_t) * (size_t)cap);
            if (!rows || !cols) OSTRIN_OOM();
        }
        rows[nrows] = row.items; cols[nrows] = row.n; nrows++;
    }
    ostrin_free(field.buf);
    *out_rows = nrows; *out_cols = cols;
    return rows;
}
