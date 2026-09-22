// The audited native kernel uses this API only inside its isolated process.
#ifndef RESHIKI_INCHI_ARENA_H
#define RESHIKI_INCHI_ARENA_H
#include <stddef.h>
#ifdef __cplusplus
extern "C" {
#endif
// The failure handler must terminate the request without allocating.
typedef void (*rsh_heap_failure)(int reason, size_t budget, size_t used, size_t requested);
void rsh_heap_initialize(size_t budget, rsh_heap_failure failure);
void rsh_heap_destroy(void);
size_t rsh_heap_used(void);
size_t rsh_heap_peak(void);
void *rsh_kernel_malloc(size_t size);
void *rsh_kernel_calloc(size_t count, size_t size);
void *rsh_kernel_realloc(void *pointer, size_t size);
void rsh_kernel_free(void *pointer);
char *rsh_kernel_strdup(const char *value);
#ifdef __cplusplus
}
#endif
#endif
