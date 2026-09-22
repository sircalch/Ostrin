/* Cancelable file I/O for native task execution.
 *
 * The libc operation itself is not forcibly interrupted. Native tasks wait on
 * a detached worker request instead, so cancellation releases the task at the
 * next timed checkpoint while the worker owns and cleans the request. The
 * cooperative/WASI path stays synchronous because it has no native worker ABI.
 */
typedef struct {
    bool ok;
    char* value;
    char* error;
} OstrinFileOutcome;

static char* ostrin_file_plain_dup(const char* value) {
    size_t length = strlen(value);
    char* copy = (char*)malloc(length + 1);
    if (!copy) OSTRIN_OOM();
    memcpy(copy, value, length + 1);
    return copy;
}

static char* ostrin_file_error_dup(int error_code) {
    const char* message = strerror(error_code ? error_code : EIO);
    return ostrin_file_plain_dup(message ? message : "file I/O error");
}

static void ostrin_file_outcome_dispose(OstrinFileOutcome* outcome) {
    if (!outcome) return;
    free(outcome->value);
    free(outcome->error);
    outcome->value = NULL;
    outcome->error = NULL;
}

static OstrinFileOutcome ostrin_file_read_blocking(const char* path) {
    OstrinFileOutcome outcome;
    memset(&outcome, 0, sizeof outcome);
    FILE* file = fopen(path, "rb");
    if (!file) {
        outcome.error = ostrin_file_error_dup(errno);
        return outcome;
    }
    if (fseek(file, 0, SEEK_END) != 0) {
        int error_code = errno;
        fclose(file);
        outcome.error = ostrin_file_error_dup(error_code);
        return outcome;
    }
    long size = ftell(file);
    if (size < 0) {
        int error_code = errno;
        fclose(file);
        outcome.error = ostrin_file_error_dup(error_code);
        return outcome;
    }
    if ((uintmax_t)size > (uintmax_t)SIZE_MAX - 1) {
        fclose(file);
        outcome.error = ostrin_file_plain_dup("file is too large to fit in a String");
        return outcome;
    }
    if (fseek(file, 0, SEEK_SET) != 0) {
        int error_code = errno;
        fclose(file);
        outcome.error = ostrin_file_error_dup(error_code);
        return outcome;
    }
    char* buffer = (char*)malloc((size_t)size + 1);
    if (!buffer) OSTRIN_OOM();
    errno = 0;
    size_t read = fread(buffer, 1, (size_t)size, file);
    int read_error = errno;
    if (read != (size_t)size && ferror(file)) {
        fclose(file);
        free(buffer);
        outcome.error = ostrin_file_error_dup(read_error);
        return outcome;
    }
    if (read != (size_t)size) {
        fclose(file);
        free(buffer);
        outcome.error = ostrin_file_plain_dup("short read while reading file");
        return outcome;
    }
    buffer[read] = 0;
    errno = 0;
    if (fclose(file) != 0) {
        int error_code = errno;
        free(buffer);
        outcome.error = ostrin_file_error_dup(error_code);
        return outcome;
    }
    outcome.ok = true;
    outcome.value = buffer;
    return outcome;
}

static OstrinFileOutcome ostrin_file_write_blocking(const char* path, const char* contents) {
    OstrinFileOutcome outcome;
    memset(&outcome, 0, sizeof outcome);
    FILE* file = fopen(path, "wb");
    if (!file) {
        outcome.error = ostrin_file_error_dup(errno);
        return outcome;
    }
    errno = 0;
    if (fputs(contents, file) == EOF) {
        int error_code = errno;
        fclose(file);
        outcome.error = ostrin_file_error_dup(error_code);
        return outcome;
    }
    errno = 0;
    if (fclose(file) != 0) {
        int error_code = errno;
        outcome.error = ostrin_file_error_dup(error_code);
        return outcome;
    }
    outcome.ok = true;
    return outcome;
}

#if defined(OSTRIN_NATIVE_THREADS)
typedef struct OstrinFileRequest {
    atomic_int refs;
    bool write;
    bool done;
    char* path;
    char* contents;
    OstrinFileOutcome outcome;
    OstrinMutex mutex;
    OstrinCond ready;
} OstrinFileRequest;

static void ostrin_file_request_worker(void* raw);

static void ostrin_file_request_release(OstrinFileRequest* request) {
    if (!request || atomic_fetch_sub(&request->refs, 1) != 1) return;
    ostrin_file_outcome_dispose(&request->outcome);
    ostrin_mutex_destroy(&request->mutex);
    ostrin_cond_destroy(&request->ready);
    free(request->path);
    free(request->contents);
    free(request);
}

static OstrinFileRequest* ostrin_file_request_start(const char* path, const char* contents, bool write) {
    OstrinFileRequest* request = (OstrinFileRequest*)calloc(1, sizeof *request);
    if (!request) OSTRIN_OOM();
    atomic_init(&request->refs, 2);
    request->write = write;
    request->path = ostrin_file_plain_dup(path);
    request->contents = contents ? ostrin_file_plain_dup(contents) : NULL;
    ostrin_mutex_init(&request->mutex);
    ostrin_cond_init(&request->ready);
    ostrin_thread_start_detached(ostrin_file_request_worker, request);
    return request;
}

static void ostrin_file_request_worker(void* raw) {
    OstrinFileRequest* request = (OstrinFileRequest*)raw;
    OstrinFileOutcome outcome = request->write
        ? ostrin_file_write_blocking(request->path, request->contents)
        : ostrin_file_read_blocking(request->path);
    free(request->path);
    request->path = NULL;
    free(request->contents);
    request->contents = NULL;
    ostrin_mutex_lock(&request->mutex);
    request->outcome = outcome;
    request->done = true;
    ostrin_cond_broadcast(&request->ready);
    ostrin_mutex_unlock(&request->mutex);
    ostrin_file_request_release(request);
}

static OstrinFileOutcome ostrin_file_request_wait(OstrinFileRequest* request) {
    OstrinFileOutcome outcome;
    memset(&outcome, 0, sizeof outcome);
    ostrin_mutex_lock(&request->mutex);
    while (!request->done) {
        ostrin_cond_wait_timeout(&request->ready, &request->mutex);
        if (ostrin_cancellation_requested()) {
            ostrin_mutex_unlock(&request->mutex);
            ostrin_file_request_release(request);
            ostrin_task_checkpoint();
            return outcome;
        }
    }
    outcome = request->outcome;
    request->outcome.value = NULL;
    request->outcome.error = NULL;
    bool cancelled = ostrin_cancellation_requested();
    ostrin_mutex_unlock(&request->mutex);
    ostrin_file_request_release(request);
    if (cancelled) {
        ostrin_file_outcome_dispose(&outcome);
        ostrin_task_checkpoint();
    }
    return outcome;
}
#endif

static OstrinFileOutcome ostrin_file_read_cancelable(const char* path) {
    ostrin_task_checkpoint();
#if defined(OSTRIN_NATIVE_THREADS)
    return ostrin_file_request_wait(ostrin_file_request_start(path, NULL, false));
#else
    OstrinFileOutcome outcome = ostrin_file_read_blocking(path);
    if (ostrin_cancellation_requested()) {
        ostrin_file_outcome_dispose(&outcome);
        ostrin_task_checkpoint();
    }
    return outcome;
#endif
}

static OstrinFileOutcome ostrin_file_write_cancelable(const char* path, const char* contents) {
    ostrin_task_checkpoint();
#if defined(OSTRIN_NATIVE_THREADS)
    return ostrin_file_request_wait(ostrin_file_request_start(path, contents, true));
#else
    OstrinFileOutcome outcome = ostrin_file_write_blocking(path, contents);
    if (ostrin_cancellation_requested()) {
        ostrin_file_outcome_dispose(&outcome);
        ostrin_task_checkpoint();
    }
    return outcome;
#endif
}
