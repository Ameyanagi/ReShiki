//! Development-only executable for independent transport and fault tests.
#![forbid(unsafe_code)]

#[global_allocator]
static ALLOCATOR: reshiki_process_heap::BoundedHeap = reshiki_process_heap::BoundedHeap;

fn main() {
    reshiki::chemistry::inchi::worker::run();
}
