# Interactive 3D and keyboard drawing review

Status: under review. Author: @Ameyanagi. Targets [issue #48](https://github.com/Ameyanagi/ReShiki/issues/48).

Generate an RDKit conformer, relax around fixed or dragged atoms, and choose an editable projection before Apply. Depth fading can be frozen separately from geometry. F8 enables an explicit keyboard drawing hotspot.

## Reviewer workflow

1. Open [adamantane.rsk](../../tests/fixtures/geometry/adamantane.rsk), choose **3D optimize…**, and try MMFF94s, MMFF94 and UFF. After changing a field, Start calculates its energy and relaxes the retained coordinates. Missing parameters produce an error without substituting another field.
2. Start relaxation, select an atom, Pin selected, and drag another atom. Stop retains the preview; Start resumes. Tilt rotates the view while physical pins and energy remain unchanged.
3. Apply, Undo and Redo. Cancel another preview and confirm the saved drawing is unchanged. Unrelated components remain unchanged. Temporary calculation hydrogens do not become drawing atoms.
4. Open [adamantane-projection.rsk](../../tests/fixtures/geometry/adamantane-projection.rsk), Freeze depth, then Clear depth. The saved [frozen](../../tests/fixtures/geometry/adamantane-frozen.rsk) and [original-ink](../../tests/fixtures/geometry/adamantane-original-ink.rsk) fixtures have exactly equal atom records and bonds, including XYZ, while their appearance scopes differ.
5. On a blank canvas, press **F8, n, 1, 1, 1**. Left visits a bond; Left again visits the preceding atom. Try 0 for a branch, Enter for a label draft, and [ / ] to mark and connect distinct atoms. Follow the [complete key/context checklist](../3d-keyboard-drawing.md#reviewer-reproduction-keyboard-targets-and-history), including focus, modifiers and Undo/Redo.

## Real desktop and renderer evidence

Captured on macOS ARM64, Apple M4, with an optimized local build. These are new-feature examples. Base: `3f31b466ccafbf6eb648bfcbab199fef1e8ba93b`. The PR head supplies the source under review. The preview and keyboard captures use desktop binary SHA-256 `dfacd50de480edb2033e9d347a57a2d7e7a54b6d42518a1f36951b56f791e97c`. The frozen-depth capture and final save-state check use `a294cb035d16765f910bb61b0d4ebf69af78094a60206036fb739d2180863998`, rebuilt after correcting appearance dirty-state detection. Both are locally ad-hoc signed apps with the same rendering and controls. No user document appears in these captures.

![MMFF94s preview with force-field, rotation and depth controls](../images/3d-keyboard-drawing/geometry-preview.png)

The preview uses the public adamantane input, MMFF94s, no pins, then rotation buttons and two free Tilt drags to choose its view. The persisted projection is the reproduction input for the exact exported appearance; conformer sampling across compilers is not expected to produce identical pixels. Preview zoom: 224%; frozen desktop zoom: 250%. Both use the existing JACS/ACS publication style.

![Frozen depth remains editable after Apply](../images/3d-keyboard-drawing/frozen-depth.png)

The following are application-renderer PNG exports of the **same projected coordinates**, 659 × 643 pixels at 1200 dpi. They compare appearance settings, not different molecular geometries. The renderer's ordinary crossing gaps remain visible.

| Frozen rear fading                                                                       | Clear depth: original ink                                                                                   |
| ---------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| ![Rear bonds fade while geometry is fixed](../images/3d-keyboard-drawing/adamantane.png) | ![The same projection with original bond colors](../images/3d-keyboard-drawing/adamantane-original-ink.png) |

Keyboard captures: blank canvas → F8, n, 1, 1, 1, then Left; 100% zoom. The orange marker is the feature being demonstrated. The selected target and controls are intentionally visible.

| Atom hotspot                                                                                    | Bond hotspot after Left                                                                               |
| ----------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| ![Keyboard drawing with a carbon atom hotspot](../images/3d-keyboard-drawing/keyboard-atom.png) | ![Arrow navigation visits a bond without adding one](../images/3d-keyboard-drawing/keyboard-bond.png) |

## Verification

- Independent RDKit 2026.03.6 oracle: all 45 molecule/field energy and gradient cases passed at identical Cartesian coordinates; exact original and temporary-H pins, stereo, rigid invariance, scale, unsupported input and unavailable MMFF parameters also passed. This validates numerical parity and local optimization, not a global minimum.
- Native CTest and Rust bridge checks passed. The settled native code was compared using the optimized proof binary SHA-256 `3affa69d290bb5fd3c48e397ca9866b12a15e2626cd38c46aac2061a492ac7c3`; subsequent changes were UI/accessibility and appearance dirty-state bookkeeping only.
- Core checks passed: 369 library tests, 516 application tests and 9 depth tests in the recorded full run. Later focused checks passed all 18 optimization/canvas tests and 8 keyboard tests, including real renderer collection, native activation, Tab/Enter, repeated-key handling, disabled states and control bounds at 636/884 pixels. The actual Iced text-field focus check passed.
- Nine chemistry/reference targets passed, including projected preparation, MOL/CX/CDXML and native response regressions. Projected stereo checks cover tetrahedral identity, defined E/Z, unknown and unspecified stereo through view rotation and exports. Legacy projected-wedge CDXML rejection remains explicit.
- Saved depth Enable/Freeze/Clear and ring-fill dirty-state regressions passed, including Undo/Redo, native round-trip, autosave eligibility and the close prompt. The final desktop check confirmed Freeze and Clear show the unsaved title marker and Undo removes it.
- Dark-canvas automatic and frozen clipboard regressions passed for CDXML/CDX while native scopes retain their base colors.
- The final optimized executable passed all 8 distribution checks with 0 skips, including MMFF94/UFF from a relocated sole executable with empty PATH and no Python/RDKit runtime. SHA-256: `bcce66c7e9f2bceba1f1c3f9e8dae6d1eb2f465e9215b9c83f4a06707a74f162`. Native imports are system libraries only.
- Desktop input confirmed all three fields, live atom dragging with a pin, Start/Stop, view rotation, depth freeze/Clear, Apply, Undo/Redo, Cancel, keyboard growth/branching, label draft and marked ring closure.

Only macOS ARM was exercised locally. PR checks run the native crate and C++ proofs on six targets. Relocated application-worker acceptance and the independent RDKit oracle run in the release workflow; actual Windows, Linux and Intel macOS packaging remains to be verified by those jobs. This adds bundled, pinned RDKit/Boost C++ source and requires CMake and a C++20 compiler when building, while the installed app remains one executable. See [backend packaging](../geometry-backend.md) and [supported chemistry and limits](../3d-keyboard-drawing.md).

Reusable caption: **Optimize with MMFF or UFF, choose an editable 3D projection, freeze rear fading, and draw with a keyboard hotspot.**
