/* E1101's dynamic guard tracks moved records while they are in flight through
 * a channel. Receiving restores access to the transferred value; the static
 * ownership pass keeps the sender's binding invalid. State belongs to the
 * live allocation, so releasing it also prevents address-reuse false positives.
 * The heap mutex makes both transitions safe across native OS threads. */
static void ostrin_mark_moved(void* p) {
    if (!p) return;
    ostrin_heap_lock();
    OstrinAllocation** link = ostrin_table_link(p);
    if (link) (*link)->moved = true;
    ostrin_heap_unlock();
}

static void ostrin_unmark_moved(void* p) {
    if (!p) return;
    ostrin_heap_lock();
    OstrinAllocation** link = ostrin_table_link(p);
    if (link) (*link)->moved = false;
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
