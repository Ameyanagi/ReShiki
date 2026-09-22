# Cleanup rotation arithmetic

Windows ARM cleanup follows the pinned x64 CPython worker running under emulation. Its non-FMA UCRT sine can differ from the native ARM CRT sine. Cleanup keeps CPython's compensated sums and expression order, then uses the existing bounded AMD win-libm adaptation for sine and cosine on Windows ARM. The other target ABIs retain their original host `cos` and `sin` calls. `atan2` is unchanged.

The original worker case `F[C@](Cl)(Br)I/0.81/selected_atoms/[2, 3, 4, 5]/True` isolates the difference. On a physical Windows x64 host, changing only the process-local `_set_FMA3_enable` setting gives identical input positions, dot/cross sums, angle, and cosine:

| Scalar             | Non-FMA CRT           | FMA CRT               |
| ------------------ | --------------------- | --------------------- |
| `atan2` angle bits | `bfe806a29303c80d`    | `bfe806a29303c80d`    |
| Sine bits          | `bfe5d4d6729eadb0`    | `bfe5d4d6729eadaf`    |
| Cosine bits        | `3fe76578751e067b`    | `3fe76578751e067b`    |
| Analysis atom 2, Y | `-1.9335777144834954` | `-1.9335777144834956` |

The second Y value is exactly the mismatch reported by Windows ARM CI run 35695458411. This identifies the output difference without assuming how the ARM CRT implements sine.

`cleanup_windows_reference.py` captures both profiles from the unchanged original worker. It computes the complete response before installing observation hooks, verifies the observed response is identical, and verifies source immutability. `fixtures/cleanup-windows-trigonometry.json` retains the source and UCRT hashes, all orientation inputs/outputs, intermediate scalars, and final full-precision analysis positions. The regression replays the real Rust orientation with the non-FMA policy and compares every coordinate bit.

The shared implementation in `src/chemistry/windows_trigonometry.rs` retains the previously audited abbreviation arithmetic unchanged. Its existing 42,567-pair native corpus and finite `[-2π, 2π]` domain remain in force. Cleanup's finite `atan2` result lies inside that domain; rejection does not fall back to host trigonometry. Attribution and the pinned AMD source are in `licenses/amd-win-libm/NOTICE`.

Recreate the capture using the pinned Windows x64 Python environment:

```sh
python tests/cleanup_windows_reference.py tests/fixtures/cleanup-windows-trigonometry.json
```

The capture requires a CPU that supports both CRT profiles. The portable fixture regression and original CPython numeric comparison run with `cargo test --features rdkit-reference --lib cleanup::numeric`; the unchanged native pair corpus runs with `cargo test --lib windows_trigonometry`. The full Windows ARM cleanup suite remains the hardware confirmation of the production target branch.
