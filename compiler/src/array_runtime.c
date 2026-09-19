/* Runtime for Array<T> (dense, row-major, N-dimensional), instantiated once per
 * element type by codegen.rs. Placeholders:
 *   @N@ struct name (Array_Float)   @T@ element C type
 *   @LT@ List_<T>   @LLT@ List_List_<T>   @LLLT@ List_List_List_<T>
 * and the macros OSTRIN_ADD/SUB/MUL/DIV(a, b), OSTRIN_ELEM_LT(a, b) supplied by the
 * instantiation. Mirrors interpreter/array.rs operation for operation (same
 * accumulation order, same error conditions). */
static @N@* @N@_alloc(int64_t rank, const int64_t* shape) {
    @N@* r = (@N@*)calloc(1, sizeof(@N@));
    if (!r) OSTRIN_OOM();
    int64_t size = 1;
    for (int64_t i = 0; i < rank; i++) size *= shape[i];
    r->rank = rank;
    r->size = size;
    r->shape = (int64_t*)malloc(sizeof(int64_t) * (size_t)rank);
    r->data = (@T@*)malloc(sizeof(@T@) * (size_t)size);
    if (!r->shape || !r->data) OSTRIN_OOM();
    memcpy(r->shape, shape, sizeof(int64_t) * (size_t)rank);
    return r;
}

static void @N@_check_shape(List_Int* s) {
    if (s->length < 1) OSTRIN_FAIL("an array needs at least one dimension");
    for (int64_t i = 0; i < s->length; i++) {
        if (s->items[i] < 1) OSTRIN_FAIL("array dimensions must be at least 1");
    }
}

static @N@* @N@_full(List_Int* s, @T@ value) {
    @N@_check_shape(s);
    @N@* r = @N@_alloc(s->length, s->items);
    for (int64_t i = 0; i < r->size; i++) r->data[i] = value;
    return r;
}

static @N@* @N@_from1(@LT@* l) {
    if (l->length < 1) OSTRIN_FAIL("an array can't be empty");
    int64_t shape[1] = { l->length };
    @N@* r = @N@_alloc(1, shape);
    for (int64_t i = 0; i < l->length; i++) r->data[i] = l->items[i];
    return r;
}

static @N@* @N@_from2(@LLT@* l) {
    if (l->length < 1 || l->items[0]->length < 1) OSTRIN_FAIL("an array can't be empty");
    int64_t shape[2] = { l->length, l->items[0]->length };
    @N@* r = @N@_alloc(2, shape);
    for (int64_t i = 0; i < shape[0]; i++) {
        if (l->items[i]->length != shape[1]) OSTRIN_FAIL("array(...) needs rectangular nested lists (all rows the same length)");
        for (int64_t j = 0; j < shape[1]; j++) r->data[i * shape[1] + j] = l->items[i]->items[j];
    }
    return r;
}

static @N@* @N@_from3(@LLLT@* l) {
    if (l->length < 1 || l->items[0]->length < 1 || l->items[0]->items[0]->length < 1) OSTRIN_FAIL("an array can't be empty");
    int64_t shape[3] = { l->length, l->items[0]->length, l->items[0]->items[0]->length };
    @N@* r = @N@_alloc(3, shape);
    for (int64_t i = 0; i < shape[0]; i++) {
        if (l->items[i]->length != shape[1]) OSTRIN_FAIL("array(...) needs rectangular nested lists (all rows the same length)");
        for (int64_t j = 0; j < shape[1]; j++) {
            if (l->items[i]->items[j]->length != shape[2]) OSTRIN_FAIL("array(...) needs rectangular nested lists (all rows the same length)");
            for (int64_t k = 0; k < shape[2]; k++) r->data[(i * shape[1] + j) * shape[2] + k] = l->items[i]->items[j]->items[k];
        }
    }
    return r;
}

static @T@ @N@_apply(int op, @T@ x, @T@ y) {
    switch (op) {
        case 0: return OSTRIN_ADD(x, y);
        case 1: return OSTRIN_SUB(x, y);
        case 2: return OSTRIN_MUL(x, y);
        default: return OSTRIN_DIV(x, y);
    }
}

static @N@* @N@_binop(@N@* a, @N@* b, int op) {
    int64_t rank = a->rank > b->rank ? a->rank : b->rank;
    int64_t* shape = (int64_t*)malloc(sizeof(int64_t) * (size_t)rank);
    int64_t* coords = (int64_t*)malloc(sizeof(int64_t) * (size_t)rank);
    if (!shape || !coords) OSTRIN_OOM();
    for (int64_t i = 0; i < rank; i++) {
        int64_t da = i < rank - a->rank ? 1 : a->shape[i - (rank - a->rank)];
        int64_t db = i < rank - b->rank ? 1 : b->shape[i - (rank - b->rank)];
        if (da == db) shape[i] = da;
        else if (da == 1) shape[i] = db;
        else if (db == 1) shape[i] = da;
        else OSTRIN_FAIL("shape mismatch: the arrays can't be broadcast together");
    }
    @N@* r = @N@_alloc(rank, shape);
    for (int64_t lin = 0; lin < r->size; lin++) {
        int64_t rem = lin;
        for (int64_t d = rank - 1; d >= 0; d--) { coords[d] = rem % r->shape[d]; rem /= r->shape[d]; }
        int64_t ia = 0, ib = 0;
        for (int64_t d = 0; d < a->rank; d++) {
            int64_t dim = a->shape[d];
            ia = ia * dim + (dim == 1 ? 0 : coords[d + (rank - a->rank)]);
        }
        for (int64_t d = 0; d < b->rank; d++) {
            int64_t dim = b->shape[d];
            ib = ib * dim + (dim == 1 ? 0 : coords[d + (rank - b->rank)]);
        }
        r->data[lin] = @N@_apply(op, a->data[ia], b->data[ib]);
    }
    free(shape);
    free(coords);
    return r;
}

static @N@* @N@_scalar(@N@* a, @T@ s, int op, int scalar_left) {
    @N@* r = @N@_alloc(a->rank, a->shape);
    for (int64_t i = 0; i < a->size; i++) {
        r->data[i] = scalar_left ? @N@_apply(op, s, a->data[i]) : @N@_apply(op, a->data[i], s);
    }
    return r;
}

static @N@* @N@_neg(@N@* a) {
    @N@* r = @N@_alloc(a->rank, a->shape);
    for (int64_t i = 0; i < a->size; i++) r->data[i] = (@T@)(-a->data[i]);
    return r;
}

static @N@* @N@_map(@N@* a, @T@ (*f)(@T@)) {
    @N@* r = @N@_alloc(a->rank, a->shape);
    for (int64_t i = 0; i < a->size; i++) r->data[i] = f(a->data[i]);
    return r;
}

@ELEM_EXTRAS@
static @T@ @N@_sum(@N@* a) {
    @T@ acc = a->data[0];
    for (int64_t i = 1; i < a->size; i++) acc = OSTRIN_ADD(acc, a->data[i]);
    return acc;
}

static @T@ @N@_min(@N@* a) {
    @T@ best = a->data[0];
    for (int64_t i = 1; i < a->size; i++) if (OSTRIN_ELEM_LT(a->data[i], best)) best = a->data[i];
    return best;
}

static @T@ @N@_max(@N@* a) {
    @T@ best = a->data[0];
    for (int64_t i = 1; i < a->size; i++) if (OSTRIN_ELEM_LT(best, a->data[i])) best = a->data[i];
    return best;
}

static int64_t @N@_rank(@N@* a) { return a->rank; }
static int64_t @N@_size(@N@* a) { return a->size; }
static List_Int* @N@_shape(@N@* a) { return List_Int_new_from_array(a->shape, a->rank); }
static @LT@* @N@_to_list(@N@* a) { return @LT@_new_from_array(a->data, a->size); }

static int64_t @N@_offset(@N@* a, const int64_t* idx, int64_t n) {
    if (n != a->rank) OSTRIN_FAIL("the number of indices doesn't match the array's dimensions");
    int64_t linear = 0;
    for (int64_t d = 0; d < n; d++) {
        if (idx[d] < 0 || idx[d] >= a->shape[d]) { fprintf(stderr, "runtime error: index out of bounds: %lld\n", (long long)idx[d]); exit(1); }
        linear = linear * a->shape[d] + idx[d];
    }
    return linear;
}
static @T@ @N@_get(@N@* a, const int64_t* idx, int64_t n) { return a->data[@N@_offset(a, idx, n)]; }
static void @N@_set(@N@* a, const int64_t* idx, int64_t n, @T@ v) { a->data[@N@_offset(a, idx, n)] = v; }
static @T@ @N@_index1(@N@* a, int64_t i) {
    if (a->rank != 1) OSTRIN_FAIL("a[i] needs a one-dimensional array");
    if (i < 0 || i >= a->shape[0]) { fprintf(stderr, "runtime error: index out of bounds: %lld\n", (long long)i); exit(1); }
    return a->data[i];
}

static @N@* @N@_reshape(@N@* a, List_Int* s) {
    @N@_check_shape(s);
    int64_t size = 1;
    for (int64_t i = 0; i < s->length; i++) size *= s->items[i];
    if (size != a->size) OSTRIN_FAIL("cannot reshape the array to that shape");
    @N@* r = @N@_alloc(s->length, s->items);
    memcpy(r->data, a->data, sizeof(@T@) * (size_t)a->size);
    return r;
}

static @N@* @N@_transpose(@N@* a) {
    if (a->rank != 2) OSTRIN_FAIL("transpose needs a two-dimensional array");
    int64_t shape[2] = { a->shape[1], a->shape[0] };
    @N@* r = @N@_alloc(2, shape);
    for (int64_t i = 0; i < a->shape[0]; i++)
        for (int64_t j = 0; j < a->shape[1]; j++)
            r->data[j * a->shape[0] + i] = a->data[i * a->shape[1] + j];
    return r;
}

static @N@* @N@_sum_axis(@N@* a, int64_t axis) {
    if (a->rank < 2) OSTRIN_FAIL("sum_axis needs at least two dimensions");
    if (axis < 0 || axis >= a->rank) OSTRIN_FAIL("the axis is out of range");
    int64_t orank = a->rank - 1;
    int64_t* oshape = (int64_t*)malloc(sizeof(int64_t) * (size_t)orank);
    int64_t* coords = (int64_t*)malloc(sizeof(int64_t) * (size_t)a->rank);
    if (!oshape || !coords) OSTRIN_OOM();
    for (int64_t d = 0, o = 0; d < a->rank; d++) if (d != axis) oshape[o++] = a->shape[d];
    @N@* r = @N@_alloc(orank, oshape);
    for (int64_t lin = 0; lin < r->size; lin++) {
        int64_t rem = lin;
        for (int64_t d = orank - 1; d >= 0; d--) { coords[d < axis ? d : d + 1] = rem % oshape[d]; rem /= oshape[d]; }
        @T@ acc = 0;
        for (int64_t k = 0; k < a->shape[axis]; k++) {
            coords[axis] = k;
            int64_t at = 0;
            for (int64_t d = 0; d < a->rank; d++) at = at * a->shape[d] + coords[d];
            acc = k == 0 ? a->data[at] : OSTRIN_ADD(acc, a->data[at]);
        }
        r->data[lin] = acc;
    }
    free(oshape);
    free(coords);
    return r;
}

static @T@ @N@_dot(@N@* a, @N@* b) {
    if (a->rank != 1 || b->rank != 1 || a->shape[0] != b->shape[0]) OSTRIN_FAIL("dot needs two one-dimensional arrays of the same length");
    @T@ acc = OSTRIN_MUL(a->data[0], b->data[0]);
    for (int64_t i = 1; i < a->size; i++) acc = OSTRIN_ADD(acc, OSTRIN_MUL(a->data[i], b->data[i]));
    return acc;
}

static @N@* @N@_matmul(@N@* a, @N@* b) {
    if (a->rank != 2 || b->rank != 2 || a->shape[1] != b->shape[0]) OSTRIN_FAIL("matmul needs (m, k) x (k, n) matrices");
    int64_t m = a->shape[0], k = a->shape[1], n = b->shape[1];
    int64_t shape[2] = { m, n };
    @N@* r = @N@_alloc(2, shape);
    for (int64_t i = 0; i < m; i++) {
        for (int64_t j = 0; j < n; j++) {
            @T@ acc = OSTRIN_MUL(a->data[i * k], b->data[j]);
            for (int64_t t = 1; t < k; t++) acc = OSTRIN_ADD(acc, OSTRIN_MUL(a->data[i * k + t], b->data[t * n + j]));
            r->data[i * n + j] = acc;
        }
    }
    return r;
}

static const char* @N@_show_rec(@N@* a, int64_t dim, int64_t off) {
    if (dim == a->rank) return @SHOW_ELEM@;
    int64_t stride = 1;
    for (int64_t k = dim + 1; k < a->rank; k++) stride *= a->shape[k];
    const char* s = "[";
    for (int64_t i = 0; i < a->shape[dim]; i++) {
        if (i > 0) s = ostrin_str_concat(s, ", ");
        s = ostrin_str_concat(s, @N@_show_rec(a, dim + 1, off + i * stride));
    }
    return ostrin_str_concat(s, "]");
}
