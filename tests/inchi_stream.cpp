// Exercise the original stream functions at allocation boundaries. The helper
// links these same patched kernel objects; source repair is independent of Rust.
#include "../tools/inchi-helper/arena.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <initializer_list>
#include <limits>
#include <string>
#define TARGET_API_LIB
#include "ichi_io.h"

static void check(bool condition) { if (!condition) std::exit(9); }
static void failure(int, size_t, size_t, size_t) { std::exit(10); }

int main() {
    constexpr size_t capacity = 16 * 1024 * 1024;
    rsh_heap_initialize(capacity, failure);
    using Printer = int (*)(INCHI_IOSTREAM *, const char *, ...);
    for (Printer print : {inchi_ios_print, inchi_ios_print_nodisplay, inchi_ios_eprint}) {
        for (size_t size : {0u, 32767u, 32768u, 32769u, 65536u, 2u * 1024u * 1024u}) {
            INCHI_IOSTREAM stream{};
            inchi_ios_init(&stream, INCHI_IOS_TYPE_STRING, nullptr);
            const std::string text(size, 'C');
            check(print(&stream, "%s", text.c_str()) == static_cast<int>(size));
            check(stream.s.nAllocatedLength > stream.s.nUsedLength);
            check(stream.s.nUsedLength == static_cast<int>(size));
            check(std::memcmp(stream.s.pStr, text.c_str(), size + 1) == 0);
            check(print(&stream, "%s", " literal 100% text") == 18);
            const auto expected = text + " literal 100% text";
            check(stream.s.nAllocatedLength > stream.s.nUsedLength);
            check(expected == stream.s.pStr);
            inchi_ios_close(&stream);
            check(rsh_heap_used() == 0);
        }
        INCHI_IOSTREAM overflow{};
        inchi_ios_init(&overflow, INCHI_IOS_TYPE_STRING, nullptr);
        overflow.s.nAllocatedLength = std::numeric_limits<int>::max();
        overflow.s.nUsedLength = std::numeric_limits<int>::max();
        check(print(&overflow, "%s", "C") == -1);
        check(overflow.s.pStr == nullptr && rsh_heap_used() == 0);
    }
    rsh_heap_destroy();
    std::puts("stream allocation boundaries passed");
    return 0;
}
