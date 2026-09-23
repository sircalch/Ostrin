/* E1101 is attached to each live allocation, not to its address globally.
 * Releasing a record removes the flag with its allocation-table entry, so a
 * later object at the same address starts unmoved. The heap mutex also makes
 * the flag safe when native tasks access it from different OS threads. */
static void ostrin_mark_moved(void* p) {
    if (!p) return;
    ostrin_heap_lock();
    OstrinAllocation** link = ostrin_table_link(p);
    if (link) (*link)->moved = true;
    ostrin_heap_unlock();
}

static void* ostrin_use_record(void* p, const char* name) {
    if (!p) return p;
    ostrin_heap_lock();
    OstrinAllocation** link = ostrin_table_link(p);
    bool moved = link && (*link)->moved;
    ostrin_heap_unlock();
    if (moved) {
        fprintf(stderr, "runtime error: '%s' was moved into a channel send earlier and cannot be used afterwards.\n", name);
        exit(1);
    }
    return p;
}
