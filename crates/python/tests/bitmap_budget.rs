// The extension's library test harness is disabled because Python resolves its
// symbols on import. Exercise the std-only admission code as an integration test.
#[path = "../src/bitmap_budget.rs"]
mod bitmap_budget;
