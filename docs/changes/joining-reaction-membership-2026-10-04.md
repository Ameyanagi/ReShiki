# Reaction membership when joining existing fragments

Joining an existing reaction fragment can lose participant roles or coefficients,
accept a join between separate reaction participants, or drop the reaction record
when its arrow and captions move too. This correctness fix restores the original
reaction metadata after drawable IDs are restored, maps merged source atoms to
their surviving destination IDs, and runs the existing reconciliation and
validation on the candidate document.

The branch is `fix/joining-reaction-membership`; the comparison baseline is
`81ca82101061ecc201545a3cae8d8257f03254d3`. Production adds 12 lines in
`src/joining.rs`; `tests/joining.rs` adds 240 lines. This is an intentional behavior
fix, separately scoped from the document/geometry refactor.

## Before and after

The native fixture has host atoms 1–2 at `(0, 0)` and `(42, 0)`, moving atoms 3–4
at `(180, 0)` and `(222, 0)`, and reaction arrow 5. Both pairs have plain single
bonds. The moving pair is a reactant with coefficient 7. A caption, when present,
has ID 6 and belongs to the reaction. These are document-property comparisons;
the test fixture does not require a renderer, zoom, or export format.

| Action                                                                             | Baseline result                                                                   | Fixed result                                                                                    |
| ---------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Select atom 3, leave arrow 5 behind, and Connect to atom 1                         | The moving reactant participant is lost.                                          | The participant retains coefficient 7 and contains joined atoms `[1, 2, 3, 4]`.                 |
| Assign host and moving molecules to separate participants under arrow 5, then join | The join succeeds after one participant was pruned, bypassing the conflict check. | The existing separate-participant error is returned and the source document is unchanged.       |
| Select atom 3, arrow 5, and caption 6, then Connect to atom 1                      | The reaction record is absent after restoring drawable IDs.                       | Arrow ID 5 and caption ID 6 remain referenced, with coefficient 7 and joined participant atoms. |

The first two regressions exercise Reactant, Product, and Agent roles across
Connect, ShareAtom, and FuseBond. Shared/fused atoms are remapped before pruning
so even a participant whose source atoms disappear entirely retains its role and
coefficient on the surviving destination atoms.

## Scope and atomicity

The existing geometry and valence path runs first. Metadata restoration and the
existing reaction reconciliation operate on the local candidate, so a rejected
join never mutates the source document or prepared original. Arrow and caption
references use their original IDs, which the existing drawable-restoration code
has already reinstated. Reaction order, role assignment, participant coefficients,
and caption-reference order are retained; reconciliation keeps its existing
sorted atom membership.

Conflicts are checked within each reaction. The same resulting molecule may
legitimately participate under different reaction arrows. The fix retains that
policy and does not change the document schema or reaction validator. No geometry,
stereo, or resource-budget guard is weakened.

## Recorded verification

Before the production fix, root ran the three new regressions against the
baseline implementation. All three failed: participant count was zero, a
conflicting join returned success, and the moved reaction vector was empty.
The command returned 101:

```sh
cargo test --locked --test joining joining_ -- --nocapture --test-threads=1
```

After the initial fix, **8 joining tests and 3 reaction tests passed**, with no
failures or ignored tests. The command returned zero:

```sh
cargo test --locked --test joining --test reactions -- --test-threads=1
```

The expanded suite passed **10 joining tests and 3 reaction tests**, with no
failures or ignored tests and a zero command result. It ran on the combined
candidate using the mandatory candidate-built native InChI helper, selected by
`RESHIKI_INCHI_HELPER`. Five new regression functions cover **30 parameterized
role/mode/topology cases**. The extensions check moved arrows and
captions in all three modes; a participant containing a stationary sodium
counterion, including conflict rejection; and legal cross-arrow overlap with
reaction order, coefficients, nonsorted caption-reference order, and moved/fixed
caption identities. The same command above runs the expanded joining and reaction
suites; for the same helper configuration, point `RESHIKI_INCHI_HELPER` to the
native helper built from the candidate checkout. Rustfmt and whitespace checks
passed.

Raw command receipts and logs are retained locally as `joining-red.{json,log}`,
`joining-green.{json,log}`, and `joining-expanded-green.{json,log}`. The checked-in
fixtures, commands, counts, and before/after property assertions provide the
review evidence without depending on those local artifacts. These results
establish the reproduced native document cases, not a full repository suite,
cross-platform coverage, desktop interaction, or an interchange-export comparison.
A matching picture would not establish correct reaction metadata; the property
assertions are the relevant evidence.

Reusable release caption: Keep reaction roles and coefficients when joining
existing fragments, and reject joins that merge separate participants in one
reaction.
