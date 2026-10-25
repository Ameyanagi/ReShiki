# Unreleased changes

## Under review

[@Ameyanagi](https://github.com/Ameyanagi) in
[PR #578](https://github.com/Ameyanagi/ReShiki/pull/578):

- Keep a pointer-owned keyboard hotspot under the pointer after wheel pan or
  zoom. Arrow navigation and edits retain their chosen target until actual
  pointer motion or a click takes over.
- Speed up hover queries on large drawings while preserving label, atom and
  bond hit results. On the exact 7,728-atom gallery, optimized M1 CPU work from
  real widget pointer input through app update and retained hover drawing falls
  from 213.3 ms to 20.0 ms. Three matched native phases keep the hotspot within
  one bond of the physical pointer; the QA note records host, camera and
  viewport limits.

| After wheel zoom                                                                                                | During wheel pan                                                                                          |
| --------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| ![A hover-owned hotspot resolves under the pointer after zoom](images/pointer-hover/after/after-wheel-zoom.png) | ![A hover-owned hotspot stays at the pointer during pan](images/pointer-hover/after/during-wheel-pan.png) |

The [source-pinned QA note](changes/qa-pointer-hover.md) includes matched
before/after captures, exact fixture and source hashes, quiet benchmark scope,
and the distinction between pointer and keyboard ownership.

See [ReShiki 0.13.0](changes-0.13.md) for the latest release notes.
