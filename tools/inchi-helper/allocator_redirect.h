// Force-included ONLY into the pinned C translation units. Include system
// declarations before redirecting both raw calls and inchi_* macro expansions.
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#include <malloc.h>
#endif
#include "arena.h"
#define malloc rsh_kernel_malloc
#define calloc rsh_kernel_calloc
#define realloc rsh_kernel_realloc
#define free rsh_kernel_free
#define strdup rsh_kernel_strdup
#define _strdup rsh_kernel_strdup
