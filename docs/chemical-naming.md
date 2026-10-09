# Local chemical naming

Open **Import → Chemical names…**. Both directions are deterministic local
operations: names and graphs are not sent to a service, and there is no network
fallback. The earlier HTTP prototype in draft [PR #275](https://github.com/Ameyanagi/ReShiki/pull/275)
is superseded; its screenshots remain [historical evidence](chemical-naming-visual-review.md).
Local rule/parser/worker checks, native CI on macOS/Linux/Windows and the actual
[current-source desktop smoke](changes/local-chemical-naming.md) pass. The
broader original [desktop review](chemical-naming-local-visual-review.md) retains
its earlier exact source and app provenance. The feature remains under review.

## Name → editable structure

Enter a supported systematic or retained name and choose **Parse name locally**.
The bundled pinned [OPSIN 2.9.0 engine](https://github.com/dan2097/opsin/tree/2.9.0)
runs in a supervised local Java process. It uses grammar rules, rather than a
name lookup table. A compatible Java 11+ HotSpot JRE/JDK must already be installed.
If Java is missing, ReShiki explains the prerequisite; it downloads nothing.
`JAVA_HOME`, an installed runtime on PATH, or an absolute `RESHIKI_JAVA` path can
select Java. macOS runtime discovery avoids the Apple download launcher.

OPSIN supports broad organic systematic nomenclature, including substituted
multifunctional esters, bicyclo/spiro systems, heterocycles, retained names,
absolute R/S and ordinary E/Z, and supported isotope/charge names. Forward
coverage is larger than the reverse profile. It is not an arbitrary common-name
database: an unrecognized name such as `aspirin` returns an explanation, never a
guessed structure or remote lookup.

Only an unambiguous complete result is accepted. Typed ambiguity/ignored-stereo
warnings, optical-rotation-only descriptions such as `(+)-lactic acid`, relative
or racemic stereo, polymers, radicals, query atoms, unsupported semantic CXSMILES
layers, disconnected salts and outputs above 512 atoms are rejected. Specified
absolute stereo, isotopes and formal charges must survive native graph import.
Unspecified stereochemistry stays unspecified. Permissive acid shorthand and
uninterpretable stereo are disabled.

The preview is an editable native drawing. Dragging atoms changes its layout;
editing SMILES and choosing **Update preview** changes its chemistry. Restore
parsed structure returns to the exact parsed graph. If the preview identity
changes, the input name is no longer asserted to describe it. Review native
import warnings when present, then **Insert editable structure**. Insertion is
one Undo step. Scrolled preview hit testing, stale-input tickets, per-tab state
and graph verification protect the insertion.

Wheel input inside the preview pans its camera; Command/Control-wheel zooms.
To scroll the surrounding panel, use its header or margin. **Restore parsed
structure** rebuilds and recenters a preview moved out of view.

## Structure → local systematic name

Select a complete connected molecule, then **Generate name locally**. Abbreviations
expand to their complete underlying graph. A selection cutting a bond, disconnected
mixture or unknown/wavy stereo is rejected. A bounded same-executable worker runs
original Rust compositional rules, profile **ReShiki organic rules 1**. It is neither
a synonym database nor a claim to cover every IUPAC name or preferred IUPAC name
(PIN). No unaudited third-party name generator is used.

The declared reverse domain is neutral organic graphs of at most 64 heavy atoms:

- Acyclic carbon parents and saturated alkyl branches, with parent/substituent
  stems of 1–20 carbons and up to three nested branch levels. Parent endpoint
  paths are exhaustive. Complete locant vectors and alphabetic ties are compared;
  a work-limit result never silently chooses a partial candidate set.
- Parent-chain C=C/C≡C locants and mixed F/Cl/Br/I prefixes. Carbon branches must
  be saturated; unsupported unsaturation or functional groups in a branch reject.
- Carboxylic acid, one simple ester, one primary amide, one nitrile, one aldehyde,
  ketones, alcohols and primary amines. Seniority is explicit. Hydroxy/amino/oxo
  and supported alkoxy groups are composed as subordinate prefixes. Terminal
  suffix classes require one group at carbon-chain locant 1.
- One saturated carbocycle of 3–12 atoms, or exact aromatic retained parents:
  benzene/phenol/aniline, pyridine, pyridazine, pyrimidine, pyrazine, 1H-pyrrole,
  1H-pyrazole, 1H-imidazole, furan, thiophene, 1,3-thiazole and 1,3-oxazole.
  Topology and heteroatom hydrogen state distinguish these parents. Retained
  aromatic suffix parents currently allow one suffix group; ring-attached
  acyclic suffixes and carbon branches larger than the ring reject.
- Specified tetrahedral R/S and ordinary parent-chain alkene E/Z use ReShiki's
  full CIP assignments on the complete graph, mapped to the naming AST locants.
  Unsupported substituent, pseudoasymmetric, axial, relative or unresolved stereo
  contexts reject. Descriptors use native full CIP priorities, rather than a
  guess from wedge direction or SMILES spelling.

Isotope and formal-charge descriptors are not implemented in the reverse profile;
those graphs explicitly reject rather than receive unlabeled or neutral names.
Fused, spiro, bridged, multiple rings, nonaromatic unsaturated rings, unsupported
heteroatom/functional groups and competing parent contexts outside the profile
also reject. Forward parsing may still support them.

A candidate name becomes visible only after strict pinned local OPSIN reconstructs
it and the native canonical isomeric graph equals the selected graph, including
connectivity, element, charge, isotope, tautomer and specified stereo. This exact
roundtrip prevents structural loss, but does not prove IUPAC parent preference:
independent expected-name tests separately exercise the rules. Names can be copied
or inserted as a caption; a changed drawing/selection requires regeneration
before caption insertion. Caption insertion is one Undo step.

The rule implementation follows the declared subset of the IUPAC
[seniority and parent principles](https://iupac.qmul.ac.uk/BlueBook/P4.html) and
[first-difference locant principles](https://iupac.qmul.ac.uk/BlueBook/P1.html#P1435).
Alkoxy composition uses the [ether prefix rules](https://iupac.qmul.ac.uk/BlueBook/P6.html#P6322)
for retained methoxy/ethoxy/propoxy/butoxy and concatenated alkyl-oxy prefixes.
These are references to nomenclature principles, not certification of PIN coverage.

## Runtime limits and provenance

The Rust executable embeds the unmodified pinned OPSIN core jar and original
Java adapter. Runtime checksum verification, an empty private working directory,
pinned options and removal of JVM injection variables prevent accidental resource
or configuration substitution. Semantic CXSMILES includes enhanced stereo,
polymers and atom labels, so unsupported semantic layers can be rejected before
native import. Naming adds no Python runtime, JNA or OPSIN InChI module.
Editable previews use the editor's native graph/import chemistry.

Each spawned worker has bounded input/output and a 15-second wall deadline.
The deadline starts after the platform spawn call returns; the synchronous macOS
observer registration/acknowledgment has no separate startup deadline. Its
20-second observer timer starts after descriptor cleanup and registration.
Cancellation,
timeout, oversized output, resource failure and dropped futures kill the dedicated
Unix process group or Windows job and reap the main child. All three pipes use
nonblocking polling; no reader/writer thread or blocking completion join can
retain a pipe after cancellation. An exited worker that leaves a pipe open is
rejected after one second.
The parser has a 192 MiB Java heap, 128 MiB metaspace, 48 MiB code cache and bounded
thread stack. A 16 GiB Linux virtual-address cap permits JVM reservations; it is
not a physical-memory allowance. The validated macOS host rejects a finite
`RLIMIT_AS` request with EINVAL, so macOS uses no address-space cap. Unix leader RSS
is sampled every 25 ms against 768 MiB for Java / 384 MiB for the native
generator, so transient sampling overshoot is possible. A disappearing process
record or Linux snapshot without a memory context during exec/exit has at most
four retries at a nominal 25 ms poll interval;
scheduling and per-poll work can add delay.
Other measurement failures and measured excess fail immediately, even when a
worker exits successfully just afterward.
Unix supports trusted direct HotSpot/ReShiki executables; launchers
that fork, daemonize or delegate parsing are unsupported. Process-group cleanup
and leader RSS accounting are not a sandbox for arbitrary hostile descendants.
Linux registers a direct-child parent-death SIGKILL before exec and rechecks the
expected parent. macOS creates an independent kernel-only observer before exec;
the worker waits for editor/worker exit-notification registration and rechecks
its parent. The observer enumerates live inherited descriptors into a fixed
stack buffer, closes them before acknowledging registration, and terminates the group
on either exit or its independent 20-second bound. Windows 10+ creates the
worker atomically in a prelimited, noninherited kill-on-close job using
`PROC_THREAD_ATTRIBUTE_JOB_LIST`; committed-memory/CPU and single-process limits
apply from its first instruction, including JVM startup. Native rules also have a 96 MiB allocation ceiling, bounded
64-atom input and explicit search/CIP work budgets. CPU caps are 20 seconds;
core dumps and output-file growth are bounded on Unix. Limits are local naming
policy and do not change global worker settings. Platform contracts are based on
[Linux parent-death signaling](https://www.man7.org/linux/man-pages/man2/PR_SET_PDEATHSIG.2const.html),
[Apple process-exit notifications](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/kqueue.2.html)
and [Microsoft creation-time job membership](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute).

On Windows, direct synchronous pipe reads distinguish an idle connected pipe
from real EOF, including peer zero-byte writes; no-data polls preserve the
response stream. Linux absent-`VmRSS` snapshots follow the bounded transient
recheck, while malformed readings and measured excess remain immediate failures.
See the [source corrections and actual platform regressions](changes/local-chemical-naming.md#corrected-process-and-admission-behavior).

Only one Java operation and one native rule operation run at a time. A semaphore
permit stays with the supervised blocking worker until it has killed/reaped and
cleaned up, even when the UI future is cancelled. Missing runtime or any failure
leaves the drawing unchanged and publishes no name. These limits are implemented
for macOS/Linux/Windows. Matching-host process tests pass for ARM64 and x64 on
all three platforms; real local-Java naming passes on macOS ARM64, Linux x64
and Windows x64. Other naming/runtime versions remain separate coverage.

Payload pins/rebuild instructions are in [tools/opsin](../tools/opsin/README.md).
[OPSIN and bundled dependency notices](../licenses/opsin/NOTICE) retain upstream
MIT/BSD/Apache terms. Original rules, adapters and tests are MIT OR Apache-2.0.

## Reproducible validation

The retained old before test `tests/chemical_naming_baseline.rs` proves that an
unchanged pre-feature Import rejects `ethanol` as SMILES. Actual local Java
baseline cases and structured warnings were recorded before this revision.
The independent Rust rule suite passes all 66 reference names, including
atom-order permutations and explicit unsupported cases. Each of those 66 names
also passes actual native-worker → local OPSIN → exact native graph verification
on macOS with network access denied. Fourteen forward reference graphs and
strict semantic rejection, missing-Java, cancellation/permit recovery and
bounded output/pipe tests pass in the same local suite. Do not attribute the old
HTTP desktop evidence to this local implementation.

The original `aaa6f9b7` locked native workspace suite passes 2,016 tests across 125 suites, with 99
explicit opt-in/live/reference tests ignored. Nine naming app state/history
tests and two separately selected actual-renderer accessibility/scrolled Insert
tests pass. The separate actual macOS desktop review verifies local ethanol and
R-lactic parsing/generation, scrolled insertion, caption history, explicit
optical-rotation rejection and fresh-process reopening. All 12 raw JPEGs and
11 native files are preserved with a byte/hash/history verifier. An
[independent native QA receipt](../tests/fixtures/chemical-naming/local/independent-desktop-qa.md)
confirms signed-app network-denied CLI formulas/graphs and independent RDKit
stereo reconstruction. GUI Cancel was not clicked; cancellation is tested in
the compiled suite. RDKit and the evidence verifier are maintainer validation
tools, not application dependencies. Those original desktop/build results
retain their recorded source. The later `d9df6491` production source passes
full required native CI and a separate fresh signed-app ethanol smoke; the
[supplemental review](changes/local-chemical-naming.md) records the process
corrections, scoped local checks, 574 compiler inputs and raw new evidence.

```sh
cargo build --locked --no-default-features -p reshiki --bin reshiki
RESHIKI_JAVA=/absolute/path/to/java RESHIKI_NAMING_HELPER=/absolute/path/to/that/reshiki cargo test --locked -p reshiki --lib naming::
RESHIKI_INCHI_HELPER=/absolute/path/to/that/reshiki cargo test --locked --no-default-features -p reshiki --bin reshiki app::naming::tests
cargo test --locked -p reshiki-process
python3 scripts/verify_chemical_naming_desktop.py
```

Independent rule cases include 2,2,5-trimethylhexane (complete locant comparison),
unsaturated alcohols, acid/hydroxy/amino/oxo seniority, esters/amides, distinct
diazine and pyrazole/imidazole topologies, R/S and E/Z pairs and atom/H/aromatic
spelling permutations. Separate decoder canaries deliberately lose connectivity,
stereo, isotope, charge or tautomer and must fail exact identity comparison.
Actual macOS startup/EOF supervisor death, leader RSS, exit-before-reap ownership
and same-group/escaped-pipe completion tests pass. The escaped-pipe test checks
the completion deadline; it does not assert containment of an escaped process.
Windows pre-input job limits and Linux parent-death tests require matching-host
execution. Local reverse integration tests must point to the
exact built executable, not a Rust test harness or another worktree's cached app.

CI requires the naming suite in the existing macOS ARM64, Windows x64 and Linux
x64 Rust jobs. A repository composite uses the official pinned
[setup-java v5.6.0 action](https://github.com/actions/setup-java/releases/tag/v5.6.0)
to select [Temurin 21.0.11+10](https://github.com/adoptium/temurin21-binaries/releases/tag/jdk-21.0.11%2B10)
using its exact provider selector `21.0.11+10.0.LTS`,
records its exact executable as `RESHIKI_JAVA`, and builds this source's app for
`RESHIKI_NAMING_HELPER`. The required workspace tests exercise independent naming
rules, local parser reconstruction, strict semantic rejection and worker limits.
Windows also runs the opt-in rendered accessibility/scrolled-insertion tests.
The existing six-target geometry matrix additionally checks and runs
`reshiki-process` on matching macOS/Linux/Windows ARM64 and x64 hosts. The release
matrix already builds the complete app for all six targets; its Intel macOS
library tests also select the pinned Java and exact release helper.

This CI-only runtime installation does not distribute Java with the application
or permit runtime downloads. The exact-source local macOS validation uses installed Zulu HotSpot 21.0.8+9;
the 14 forward reference graphs also pass with Java overrides unset and a minimal
`/usr/bin:/bin` PATH, discovering the installed runtime through the user's Nix
profile rather than Apple's installer launcher. Java 11 bytecode compatibility
is checked; actual Java 11 runtime execution is not yet tested.
Strict process-crate compilation/linting passes for macOS, Linux and Windows on
ARM64 and x64; those cross-compiles do not establish other-platform runtime
behavior. Required native CI on production source `d9df6491` completed with
22 successful checks, three intentional skips and zero failures; the
[compact frozen receipt](../tests/fixtures/chemical-naming/process-d9df6491/ci-native-summary.json)
records real runtime banners, all three app helpers, the naming reference suites
and all six matching-host process checks. Later documentation-only heads have
their own CI status. The earlier
[invalid-context run](https://github.com/Ameyanagi/ReShiki/actions/runs/37948361760)
scheduled no native jobs; the subsequent
[shortened Java-selector run](https://github.com/Ameyanagi/ReShiki/actions/runs/37951149210)
stopped all three native hosts before application build/tests. These CI setup
failures are retained separately from local application checks; both narrow
workflow corrections preserve exact runtime/helper guards. The subsequent
[3706 runtime failures](https://github.com/Ameyanagi/ReShiki/actions/runs/37954198084)
were actual admission/RSS/pipe failures after Java setup and app build; their
frozen logs and controlled correction tests remain separate from the later
passing production source. Offline naming
tests use only embedded parser resources and local fixtures. Explicit network
denial has been exercised for the current macOS naming, process and escaped-pipe
suites using the macOS sandbox; it is not claimed for other CI operating systems.
