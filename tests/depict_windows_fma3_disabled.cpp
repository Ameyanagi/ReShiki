// Development-only CRT profile validation. Link this object directly into a
// geometry test executable, never into the application or a shipped library.
// The C runtime invokes this initializer before Rust's test harness starts.
#include <math.h>
namespace {
const int native_fma3_disabled = _set_FMA3_enable(0);
}
