---
section: Changes
issues: []
---

- Reduced repeated allocation and undo-memory accounting work while editing
  drawings, preserving chemistry-change detection and undo behavior.
- Equipment lookups load definitions as needed and reuse a catalogue interface
  index when finding compatible adapters, keeping the same matching choices.
- Depiction repair reuses bounded topology distances and detached fragments
  across stages while preserving coordinates and work-limit accounting.
- NMR prediction reuses molecular adjacency and site sphere distances during
  fallback without changing environment identifiers or deadline checks.
