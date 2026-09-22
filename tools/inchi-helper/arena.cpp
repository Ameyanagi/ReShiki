// Fixed arena for the pinned kernel's direct C heap allocations. Payload,
// alignment padding, and block metadata all live inside the requested budget.
// This is not a limit on process RSS, libc internals, stack, or bridge buffers.
#include "arena.h"
#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <limits>

namespace {
constexpr size_t alignment = alignof(std::max_align_t);
constexpr size_t none = std::numeric_limits<size_t>::max();
constexpr size_t tag = size_t(0x52485348454150ULL);
struct alignas(std::max_align_t) Block {
    size_t marker, length, previous, next, requested;
    bool available;
};
static_assert(sizeof(Block) % alignment == 0);
unsigned char *arena = nullptr;
size_t capacity = 0, budget = 0, used = 0, peak = 0;
rsh_heap_failure on_failure = nullptr;

[[noreturn]] void fail(int reason, size_t requested) {
    if (on_failure) on_failure(reason, budget, used, requested);
    std::_Exit(4); // Fail closed if a handler unexpectedly returns.
}
size_t rounded(size_t value) {
    if (value > none - (alignment - 1)) fail(1, value);
    return (value + alignment - 1) / alignment * alignment;
}
Block read(size_t offset) {
    if (!arena || offset % alignment || offset > capacity || sizeof(Block) > capacity - offset)
        fail(3, offset);
    Block block{};
    std::memcpy(&block, arena + offset, sizeof(block));
    if (block.marker != (tag ^ offset) || block.length > capacity - offset - sizeof(Block))
        fail(3, offset);
    return block;
}
void write(size_t offset, Block block) {
    if (!arena || offset > capacity || sizeof(Block) > capacity - offset)
        fail(3, offset);
    block.marker = tag ^ offset;
    std::memcpy(arena + offset, &block, sizeof(block));
}
size_t locate(void *pointer) {
    const auto base = reinterpret_cast<std::uintptr_t>(arena);
    const auto address = reinterpret_cast<std::uintptr_t>(pointer);
    if (!arena || address < base || address - base < sizeof(Block) || address - base >= capacity)
        fail(3, 0);
    const size_t offset = address - base - sizeof(Block);
    if (read(offset).available) fail(3, 0);
    return offset;
}
void merge_next(size_t offset, Block &block) {
    if (block.next == none) return;
    auto next = read(block.next);
    if (!next.available) return;
    if (block.next != offset + sizeof(Block) + block.length || next.previous != offset)
        fail(3, 0);
    block.length += sizeof(Block) + next.length;
    block.next = next.next;
    if (block.next != none) {
        auto following = read(block.next);
        following.previous = offset;
        write(block.next, following);
    }
    write(offset, block);
}
void split(size_t offset, Block &block, size_t wanted) {
    if (wanted > block.length) fail(3, wanted);
    if (block.length - wanted < sizeof(Block) + alignment) return;
    const size_t next_offset = offset + sizeof(Block) + wanted;
    Block remainder{0, block.length - wanted - sizeof(Block), offset, block.next, 0, true};
    if (block.next != none) {
        auto following = read(block.next);
        following.previous = next_offset;
        write(block.next, following);
    }
    block.length = wanted;
    block.next = next_offset;
    write(next_offset, remainder);
    merge_next(next_offset, remainder);
    write(offset, block);
}
}

extern "C" void rsh_heap_initialize(size_t bytes, rsh_heap_failure failure) {
    on_failure = failure;
    if (arena) fail(3, bytes);
    budget = bytes;
    capacity = bytes / alignment * alignment;
    used = peak = 0;
    if (capacity < sizeof(Block) + alignment) fail(1, sizeof(Block) + alignment);
    arena = static_cast<unsigned char *>(std::malloc(capacity));
    if (!arena) fail(2, capacity);
    write(0, Block{0, capacity - sizeof(Block), none, none, 0, true});
}
extern "C" void rsh_heap_destroy(void) {
    std::free(arena);
    arena = nullptr;
    capacity = budget = used = peak = 0;
    on_failure = nullptr;
}
extern "C" size_t rsh_heap_used(void) { return used; }
extern "C" size_t rsh_heap_peak(void) { return peak; }

extern "C" void *rsh_kernel_malloc(size_t size) {
    const size_t wanted = rounded(std::max(size, size_t(1)));
    size_t offset = 0;
    while (offset != none) {
        auto block = read(offset);
        if (block.available && block.length >= wanted) {
            split(offset, block, wanted);
            block.available = false;
            block.requested = size;
            write(offset, block);
            used += sizeof(Block) + block.length;
            peak = std::max(peak, used);
            if (used > capacity) fail(3, size);
            return arena + offset + sizeof(Block);
        }
        if (block.next != none && block.next <= offset) fail(3, size);
        offset = block.next;
    }
    fail(1, size);
}
extern "C" void *rsh_kernel_calloc(size_t count, size_t size) {
    if (count != 0 && size > none / count) fail(1, none);
    const size_t bytes = count * size;
    void *pointer = rsh_kernel_malloc(bytes);
    std::memset(pointer, 0, bytes);
    return pointer;
}
extern "C" char *rsh_kernel_strdup(const char *value) {
    // MSVC's _strdup and the kernel's Unix inchi__strdup both accept null.
    // Default option parsing uses this path before any allocation is needed.
    if (!value) return nullptr;
    const size_t length = std::strlen(value);
    if (length == none) fail(1, length);
    auto *copy = static_cast<char *>(rsh_kernel_malloc(length + 1));
    std::memcpy(copy, value, length + 1);
    return copy;
}
extern "C" void rsh_kernel_free(void *pointer) {
    if (!pointer) return;
    size_t offset = locate(pointer);
    auto block = read(offset);
    if (used < sizeof(Block) + block.length) fail(3, 0);
    used -= sizeof(Block) + block.length;
    block.available = true;
    block.requested = 0;
    write(offset, block);
    merge_next(offset, block);
    if (block.previous != none) {
        auto previous = read(block.previous);
        if (previous.available) merge_next(block.previous, previous);
    }
}
extern "C" void *rsh_kernel_realloc(void *pointer, size_t size) {
    if (!pointer) return rsh_kernel_malloc(size);
    if (size == 0) { rsh_kernel_free(pointer); return nullptr; }
    const size_t offset = locate(pointer);
    auto block = read(offset);
    const size_t old_length = block.length;
    const size_t wanted = rounded(size);
    if (wanted > block.length && block.next != none) {
        const auto next = read(block.next);
        if (next.available && sizeof(Block) + next.length >= wanted - block.length)
            merge_next(offset, block);
    }
    if (wanted <= block.length) {
        split(offset, block, wanted);
        used = used - old_length + block.length;
        peak = std::max(peak, used);
        block.requested = size;
        write(offset, block);
        return pointer;
    }
    void *replacement = rsh_kernel_malloc(size);
    std::memcpy(replacement, pointer, std::min(block.requested, size));
    rsh_kernel_free(pointer);
    return replacement;
}
