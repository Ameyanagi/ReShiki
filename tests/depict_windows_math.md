# Windows CRT geometry profiles

Windows x64 uses two native transcendental paths. The UCRT chooses whether to
use FMA3 according to processor support. A fixture captured on an FMA3 machine
therefore cannot describe every Windows x64 runner, even when both programs use
the same original RDKit wheel and separate source-order arithmetic.

The Windows Server 2022 CI failure at `random-reflect/458` is reproduced by
calling the original `RDDepict::reflectPoint` on the physical capture host, with
`_set_FMA3_enable(0)` applied only inside that native observer process:

| Value                  | FMA3 enabled       | FMA3 disabled      |
| ---------------------- | ------------------ | ------------------ |
| Normalized dot product | `3fd954d5989f7ab5` | `3fd954d5989f7ab5` |
| `acos`                 | `3ff29f25c111e687` | `3ff29f25c111e686` |
| `sin(acos)`            | `3fed63011ebd161e` | `3fed63011ebd161d` |
| `cos(acos)`            | `3fd954d5989f7ab4` | `3fd954d5989f7ab7` |
| Reflected x            | `401134e00e409141` | `401134e00e409142` |
| Reflected y            | `c0135535b3bf8431` | `c0135535b3bf8430` |

These are binary64 bits. The disabled native x is exactly the value observed
from Rust on CI. The difference starts in the CRT `acos`, not in reflection's
source arithmetic. No application math code or global system setting changes.

Both native profiles were independently recaptured over the same 5,310 input
cases. The enabled capture reproduces every existing Windows fixture result.
The disabled profile changes 426 ring scalars and 21 reflection scalars;
canonical orientations, bisectors and boxes are identical. Both captures
contain the same 24 nonfinite one-atom rings. No projected f32 value differs
between the profiles in this corpus.

The Rust test identifies the CRT profile with the three primitive witnesses
above. `black_box` prevents compile-time folding of the runtime probe. An unknown
profile fails with its observed bits. The matching independently generated
fixture then requires exact bits for all 71,754 finite scalars. The other profile
remains an explicit cross-profile audit. Neither tolerance nor a fixed allowed
difference count is used.

## Capture and validation

Run from an x64 MSVC developer prompt, with the pinned wheel and Boost 1.85:

```bat
python tests\build_depict_windows_oracle.py --component geometry ^
  --rdkit-source F:\reference\rdkit --boost-include F:\reference\boost185 ^
  --geometry-fma3 0 ^
  --output tests\fixtures\depict-geometry-windows-no-fma3-native.json.gz
```

Use `--geometry-fma3 1` and a separate output path to audit the enabled fixture.
The observer rejects an unavailable requested profile. The fixture records the
requested setting, UCRT hash, original DLL/source hashes, compiler command and
observer hash. The control affects only the C++ observer process.

To exercise the disabled path on an FMA3-capable development host, compile
`tests/depict_windows_fma3_disabled.cpp` with `/c /O2 /MD /EHsc` and link the
resulting object directly into the geometry test executable using:

```bat
cargo rustc --locked --features rdkit-reference --test depict_geometry -- ^
  -C link-arg=F:\reference\depict_windows_fma3_disabled.obj
```

Run that executable with `RESHIKI_TEST_WINDOWS_MATH_PROFILE=no-fma3`; ordinary
validation on the capture host uses `fma3`. The required-profile check prevents
an ineffective initializer from passing unnoticed. This development-only object
is never linked into the application or included in release build commands.

Disabled fixture SHA-256:

- Compressed: `c43e59ac9bc81a79be649e1fc4ead6a58146db3caf0e5e332d8f36a747882cae`
- JSONL: `63353093f603d46938d48af90cc2d73bd9970e43664561d774ad1066fd0d29ca`
- Capture UCRT: `5c52e3a303baaac0e0af8bd9b96134993da34bc9d834a31ef37e1d2cdc7fe192`

The capture host is Windows 11 x64, AMD Ryzen 9 7940HS. This does not establish
Windows ARM64 parity or claim a successful hosted CI rerun.
