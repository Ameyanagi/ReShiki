# Independent rear-opacity native and export verification

**PASS** for the retained C60 desktop saves and fresh signed CLI exports. Production source is `7ec621227bc6fd777d5918efb9b8b6443c207d8e`; the signed executable SHA256 is `4d35fe616bc9cb1388e4a6ec45312c65cc21aa6b67e42e17b077dd848fcc4043`. Independently verified 1,069 source inputs against Git and the worktree, all nine signed bundle files, codesign verification, and 15 own compiler artifacts marked `fresh: false`. This reviewer ran no Cargo or GUI.

All 11 native files retain 60 atoms, 90 bonds and all 180 finite XYZ values. Against the typed parent native19 save, every atom/bond field and every nonappearance field is exact. At 100%, only version19→22 changes. At 25% and 0%, there is one automatic alpha scope owning all 60 atoms, with RGB strength0. Stored normalized weights match the expected f32 values exactly: 30 atoms are front and 30 rear. Between 25% and 0%, only `rear_opacity` changes.

Raw bytes match in these groups:

- 100% = 25% Undo.
- 25% = 25% Redo = 0% Undo = fresh25% = repeated25% no-op.
- 0% = 0% Redo = fresh0%.

Fresh signed CLI analysis is identical across all three settings: C60, mass720.66, exact mass720, 32 reported rings and identical canonical SMILES, InChI and InChIKey. Typed-before SVG, PNG and PDF exports are byte-identical to 100%. The 25% SVG has 16 faded primitives, each with effective alpha0.25 and exactly one wrapper; its 15 opaque foreground primitives exactly match 0%. The 0% SVG contains no faded primitives. PDF retains `/ca 0.25`.

Four recorded solid bond-interior samples, classified by endpoint Z, retain black foreground. Rear samples become RGB191 at 25% and white at 0%. These are representative samples selected after inspecting output, not an exhaustive or preregistered pixel test. The complete SVG opacity and foreground-primitive comparisons are separate structural checks. These CLI PNGs use opaque white paper, so the measurement is composited color, not transparent alpha64. All 17 bounded signature and CLI commands passed; logs and export artifacts are retained.

Root's actual CUA RAWJPEGs were inspected; no simulated screenshot was produced. They show faint rear ink at 25%, hidden rear ink at 0%, retained foreground and unchanged whole-drawing C60/60/90 properties. The six public JPEGs and the manual receipt have recorded hashes. Root reports disabled Undo/Redo after fresh opens and the repeated25% no-op; saved native bytes independently confirm those states. Fresh capture tab counts and chrome differ and are not claimed matched. Private initial recovery-banner and keyboard-mode images remain retained outside the public set. The 0% CLI export bounds shrink slightly because omitted rear paint no longer contributes to the bounds; XYZ is unchanged and root used the same209% desktop zoom.

The portable checker passed native/export replay, Ruff check and format, ty and Python syntax checks. Negative controls rejected changed XYZ, a wrong Redo and an extra opacity wrapper. Copy it unchanged to `scripts/verify_rear_opacity_evidence.py`: its default input is `tests/fixtures/rear-opacity`, retaining all 11 original filenames. Native/history replay needs only standard-library Python3.9+. Optional `--export-dir` verification needs Pillow and the retained CLI outputs. Those exports need not be published for portable native replay; omit that option when only native fixtures are copied.

The manual scope is the whole unselected C60 in light theme. This does not claim manual checks of dark theme, selected components, highlights, C70 or Windows/Office. Scene clipping and omitted rear ink are expected paint changes; chemical fields, XYZ and bond projection flags remain exact. Exact hashes, native groups, capture conditions, runtime commands and limits are in `independent-desktop-verification.json`.
