/* String methods, mirroring interpreter/strings.rs: ASCII whitespace and case mapping only,
 * `length` counts code points. Every result is a fresh malloc'd string (never freed, like the
 * rest of this backend's heap values). */
static char* ostrin_s_dup(const char* s, size_t n) {
    char* r = (char*)malloc(n + 1);
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
    char* r = (char*)malloc(n + count * (tl > fl ? tl - fl : 0) + 1);
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

static const char** ostrin_s_split(const char* s, const char* sep, int64_t* out_n) {
    size_t sl = strlen(sep);
    if (sl == 0) OSTRIN_FAIL("'split' needs a non-empty separator");
    int64_t count = 1;
    for (const char* p = s; (p = strstr(p, sep)); p += sl) count++;
    const char** items = (const char**)malloc(sizeof(char*) * (size_t)count);
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
    const char** items = (const char**)malloc(sizeof(char*) * (size_t)(count > 0 ? count : 1));
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
    char* r = (char*)malloc(total);
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
