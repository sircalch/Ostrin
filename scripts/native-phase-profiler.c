// A bounded, single-thread profiler appended to an emitted Ostrin translation unit.
// All callbacks/helpers are excluded from instrumentation to prevent recursion.
#include <time.h>
#include <stdint.h>
#include <inttypes.h>
#define PHASE_COUNT @PHASE_COUNT@
#define NO_PROFILE __attribute__((no_instrument_function))

typedef struct { void* address; const char* label; uint64_t calls, inclusive_ns, exclusive_ns; } phase_counter;
typedef struct { unsigned index; uint64_t start_ns, child_ns; } phase_frame;
static phase_counter phases[PHASE_COUNT] = { @PHASE_ENTRIES@ };
static phase_frame phase_stack[16];
static unsigned phase_depth, phase_error;
static uint64_t phase_unknown_calls;

static uint64_t NO_PROFILE phase_now(void) {
    struct timespec value;
    if (clock_gettime(CLOCK_MONOTONIC, &value) != 0) { phase_error = 1; return 0; }
    return (uint64_t)value.tv_sec * UINT64_C(1000000000) + (uint64_t)value.tv_nsec;
}
static int NO_PROFILE phase_index(void* function) {
    for (unsigned i = 0; i < PHASE_COUNT; ++i) if (phases[i].address == function) return (int)i;
    return -1;
}
void NO_PROFILE __cyg_profile_func_enter(void* function, void* caller) {
    (void)caller;
    int index = phase_index(function);
    if (index < 0) { ++phase_unknown_calls; return; }
    if (phase_depth >= 16) { phase_error = 1; return; }
    // Observable launch-to-entry latency also includes pipe/event-loop delivery.
    if (index == 0) fputs("ostrin phase entry\n", stderr);
    phase_stack[phase_depth++] = (phase_frame){ (unsigned)index, phase_now(), 0 };
    ++phases[index].calls;
}
void NO_PROFILE __cyg_profile_func_exit(void* function, void* caller) {
    (void)caller;
    int index = phase_index(function);
    if (index < 0) return;
    uint64_t end = phase_now();
    if (phase_depth == 0 || phase_stack[phase_depth - 1].index != (unsigned)index) { phase_error = 1; return; }
    phase_frame frame = phase_stack[--phase_depth];
    if (end < frame.start_ns || end - frame.start_ns < frame.child_ns) { phase_error = 1; return; }
    uint64_t inclusive = end - frame.start_ns;
    phases[index].inclusive_ns += inclusive;
    phases[index].exclusive_ns += inclusive - frame.child_ns;
    if (phase_depth) phase_stack[phase_depth - 1].child_ns += inclusive;
}
static void NO_PROFILE __attribute__((destructor)) phase_report(void) {
    fprintf(stderr, "ostrin phases: {\"schema\":1,\"unknownCalls\":%" PRIu64 ",\"error\":%u,\"depth\":%u,\"phases\":[", phase_unknown_calls, phase_error, phase_depth);
    for (unsigned i = 0; i < PHASE_COUNT; ++i) {
        fprintf(stderr, "%s{\"name\":\"%s\",\"calls\":%" PRIu64 ",\"inclusiveNs\":%" PRIu64 ",\"exclusiveNs\":%" PRIu64 "}", i ? "," : "", phases[i].label, phases[i].calls, phases[i].inclusive_ns, phases[i].exclusive_ns);
    }
    fputs("]}\n", stderr);
}
