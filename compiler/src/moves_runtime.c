/* "Moved after send" tracking (E1101), mirroring the interpreter: a record sent
 * through a channel is remembered by address, and reading a variable that holds it
 * afterwards is a runtime error. Records are never freed, so an address is never
 * reused. Open-addressing hash set of pointers. */
static void** ostrin_moved_table;
static uint64_t ostrin_moved_capacity;
static uint64_t ostrin_moved_count;

static uint64_t ostrin_moved_hash(void* p) {
    return ((uint64_t)(uintptr_t)p >> 4) * 0x9E3779B97F4A7C15ULL;
}

static void ostrin_moved_insert_raw(void* p) {
    uint64_t i = ostrin_moved_hash(p) & (ostrin_moved_capacity - 1);
    while (ostrin_moved_table[i] && ostrin_moved_table[i] != p) i = (i + 1) & (ostrin_moved_capacity - 1);
    if (!ostrin_moved_table[i]) ostrin_moved_count++;
    ostrin_moved_table[i] = p;
}

static void ostrin_mark_moved(void* p) {
    if (ostrin_moved_count * 2 >= ostrin_moved_capacity) {
        void** old = ostrin_moved_table;
        uint64_t old_capacity = ostrin_moved_capacity;
        ostrin_moved_capacity = old_capacity ? old_capacity * 2 : 64;
        ostrin_moved_table = (void**)ostrin_calloc(ostrin_moved_capacity, sizeof(void*));
        if (!ostrin_moved_table) OSTRIN_OOM();
        ostrin_moved_count = 0;
        for (uint64_t i = 0; i < old_capacity; i++) if (old[i]) ostrin_moved_insert_raw(old[i]);
        ostrin_free(old);
    }
    ostrin_moved_insert_raw(p);
}

static void* ostrin_use_record(void* p, const char* name) {
    if (ostrin_moved_capacity) {
        uint64_t i = ostrin_moved_hash(p) & (ostrin_moved_capacity - 1);
        while (ostrin_moved_table[i]) {
            if (ostrin_moved_table[i] == p) {
                fprintf(stderr, "runtime error: '%s' was moved into a channel send earlier and cannot be used afterwards.\n", name);
                exit(1);
            }
            i = (i + 1) & (ostrin_moved_capacity - 1);
        }
    }
    return p;
}
