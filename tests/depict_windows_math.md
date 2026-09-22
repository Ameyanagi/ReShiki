# Windows CRT depiction profiles

The recorded fixtures cover two native CRT paths on the physical Windows
capture host. Windows CRT implementations also vary across OS versions and
processors. CI builds fresh observers against the pinned original wheel so each
runner supplies its own exact expectations.

## Fresh references on each runner

`setup_windows_depict_reference.py` checks out the pinned source with LF bytes,
verifies the Boost archive, and builds all eight x64 observers. It exports their
paths only after every build succeeds. Windows ARM uses the x64 reference wheel
under emulation; Rust continues to target native ARM64.

The Rust tests replay the original corpus through these native observers and
require exact f64/f32 bits, topology, ordering, and errors. Recorded fixtures
remain cross-runtime audits. No application arithmetic, tolerance, or expected
result is inferred from Rust output.

The setup records two independent CRT probes: one inside the reference Python
process and one compiled with MSVC `/MD`. Both record the loaded DLL path,
SHA-256, file version, and acos/sine/cosine witnesses. Build provenance and fresh
captures are retained by the targeted Windows depiction workflow, including
when a test fails. The full Checks gate still runs every reference target.

From an x64 MSVC developer environment:

```powershell
uv run --locked python scripts/setup_windows_depict_reference.py
. ./artifacts/depict-live-windows/environment.ps1
cargo test --locked --features rdkit-reference --test depict_geometry --test depict_rings --test depict_attachment --test depict_seeds --test depict_templates --test depict_collision --test depict_expansion --test depict_finalize --test depict_pipeline --no-fail-fast
```

Restore the native compiler environment before running ARM64 Rust tests.
Observer binaries are rebuilt on each host; only the hash-checked Boost download
is reusable.

## Why a single CRT profile witness was insufficient

The Windows Server 2022 run at `3aa2de5` reported the disabled-profile acos
witness, while every one of its 35,192 ring scalars matched the physical host's
enabled fixture. Four reflected scalars still differed from that fixture. The
three-witness classifier could therefore identify neither complete native
runtime. Adding more guessed profile labels would not establish native parity.
Fresh original observers resolve the ambiguity without choosing expectations
from Rust results. The optional static classifier now also checks independent
ring sine/cosine witnesses and rejects unknown combinations.

The targeted run `35709016632` independently confirmed the mixed witnesses in
both Python and a separately compiled native process on Server 2022, using UCRT
`10.0.20348.5622` (SHA-256
`5e5ae0f8e4325ceb3ee767065dda6e03e980d401f00a2bdb8ca81ed90813920f`).
The ARM runner's x64 reference process used UCRT `10.0.26100.9444` (SHA-256
`529e795875178b906ea8758abc8de5c28339f336b3e7a121d7778ae040908de9`)
and matched the recorded disabled witnesses. This identifies reference runtimes;
it does not establish Rust parity on those runners.

That run also exposed a checkout failure before native compilation. RDKit marks
C++ files as `text`, so `core.autocrlf=false` alone still permits native CRLF
endings on Windows. Fresh reference repositories now set **both**
`core.autocrlf=false` and `core.eol=lf` before their first checkout. The original
source hashes remain mandatory. A real Git regression forces a CRLF default and
checks that the new checkout exactly preserves the committed source blob.

## Recorded physical-host profiles

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

Without live observers, the tests ask the pinned **x64 reference interpreter**
to observe these UCRT and ring witnesses. An unknown profile fails with its observed bits. On Windows
ARM64, this is the x64 interpreter running under emulation, as used by the
original worker. The expected fixture is never chosen from Rust's result.

The matching independent geometry fixture requires exact bits for all 71,754
finite scalars. The other profile remains an explicit cross-profile audit.
Neither tolerance nor a fixed allowed difference count is used.

## Companion stages

All seven companion observers were also recaptured with FMA3 disabled. Each
received the same original input cases. Every changed value is an expected f64
bit string: no input, topology, order, outcome, or error changed.

| Stage        | Cases | Changed f64 values |
| ------------ | ----: | -----------------: |
| Rings        | 1,338 |              3,212 |
| Attachment   |   970 |              3,084 |
| Seeds        |   991 |                  0 |
| Templates    | 1,921 |              1,202 |
| Collision    | 1,634 |                161 |
| Expansion    | 1,877 |             20,210 |
| Finalization | 2,312 |                  0 |

Expansion additionally checks the extracted full wrapper against the original
exported `compute2DCoords` in both canonical settings for every request; 8,536
public-check scalars differ between profiles. Those checks remain exact within
each profile.

Seeds and finalization use their existing fixtures because the independently
recaptured data rows are byte-identical. Attachment needs a separate capture
even though its old-fixture replay passed: that stage restores the captured
initial ring before exercising attachment operations.

Capture hashes, native compiler commands, runtime initializer hashes, and the
invariant captures' provenance are recorded in
[`fixtures/depict-windows-profile-audit.json`](fixtures/depict-windows-profile-audit.json).
The five changed companion fixtures retain their full native provenance.

## Capture and validation

Run from an x64 MSVC developer prompt, with the pinned wheel and Boost 1.85:

```bat
python tests\build_depict_windows_oracle.py --component geometry ^
  --rdkit-source F:\reference\rdkit --boost-include F:\reference\boost185 ^
  --fma3 0 ^
  --output tests\fixtures\depict-geometry-windows-no-fma3-native.json.gz
```

Use `--fma3 1` and a separate output path to audit the enabled fixture.
The shared Windows builder accepts `--fma3` for geometry, rings, attachment,
seeds, and templates. The Windows expansion builder, finalization builder, and
collision reference capture accept the same option. Their diagnostic initializer
sets the CRT profile before any native requests execute; source adapters and
original DLL calls are unchanged.

The observer rejects an unavailable requested profile. The fixture records the
requested setting, UCRT hash, original DLL/source hashes, compiler command and
observer hash. The control affects only the C++ observer process.

To exercise the disabled path on an FMA3-capable development host, compile
`tests/depict_windows_fma3_disabled.cpp` with `/c /O2 /MD /EHsc` and link the
resulting object directly into a stage test executable, for example:

```bat
cargo rustc --locked --features rdkit-reference --test depict_geometry -- ^
  -C link-arg=F:\reference\depict_windows_fma3_disabled.obj
```

Run that executable with `RESHIKI_TEST_WINDOWS_MATH_PROFILE=no-fma3`; ordinary
validation on the capture host uses `fma3`. This diagnostic variable also sets
the independently probed reference process to the requested profile. The full
exact stage comparison detects an ineffective Rust test initializer. Normal CI
leaves this variable unset and observes the original reference runtime as-is.
This development-only object is never linked into the application or included in release build commands.

Disabled fixture SHA-256:

- Compressed: `c43e59ac9bc81a79be649e1fc4ead6a58146db3caf0e5e332d8f36a747882cae`
- JSONL: `63353093f603d46938d48af90cc2d73bd9970e43664561d774ad1066fd0d29ca`
- Capture UCRT: `5c52e3a303baaac0e0af8bd9b96134993da34bc9d834a31ef37e1d2cdc7fe192`

The capture host is Windows 11 x64, AMD Ryzen 9 7940HS. This does not establish
Windows ARM64 parity or claim a successful hosted CI rerun.
