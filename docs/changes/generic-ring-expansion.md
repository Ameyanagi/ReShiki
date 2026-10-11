# Ring expansion placement and unique chemical products

**Under review; native after evidence is pending.** Contribution: @Ameyanagi,
project creator and maintainer.

## Problem and correction

The six-position benzene fixture previously produced six chemically equivalent
anisoles. Its substituent kept its original coordinates, so attachment bonds
could cross the ring or have unsuitable lengths.

Expansion now keeps the host fixed, rigidly places a detachable substituent
outward and uses the drawing style's normal connector length. Ordinary C/N/O/S
roots with one single-bond branch neighbor use a 120° attachment angle. Internal
fragment distances are preserved. Collision checks include saved/refreshed
label ink and the connector against the host and nonincident moving bonds.

An exact bounded chemical-graph comparison removes equivalent products, keeping
the first enumeration representative. Elements, isotopes, charge, atom maps,
stereochemistry and directed dative bonds remain meaningful. Six equivalent
positions give one anisole; five available positions with an unlike fixed
substituent give three methoxytoluenes: ortho, meta and para.

## Reproduction inputs and before evidence

Open the [nine-atom/eight-bond arene input](../../tests/fixtures/generic-structures/variable-arene-native-refused-v26.rsk)
and choose **Properties → Chemical abbreviations → Expand Generic Structures**.
Its SHA-256 is `a129c109a464b9a1414cce4d09aded6ac6290848e0cfec84c92f67dd520f7be2`.
The [ten-atom/nine-bond substituted input](../../tests/fixtures/generic-structures/variable-methoxytoluene-v26.rsk)
has five candidate positions and SHA-256
`6eaa850521b62f9a0538bd3ea107b45acbdb2314b39d274166f90685a90ead82`.

Actual macOS before captures were produced by `ce40d2386207807514d0fa9b793b2ba16ebcb62d`,
signed executable SHA-256
`0a036aec87ad3058f94d8d8099a7f1bb1c16547161c5652e7bbe971a0ca2b102`.
The [historical combined review](coordination-join-integration.md) retains its
six-product output and actual producer. Dedicated unmodified desktop captures
also record six anisoles and five methoxytoluenes with duplicated positional
products and malformed attachment geometry. After captures will be added from
the qualified new build; none is claimed here yet.

![Historical ring-crossing defect, not an accepted result](../images/coordination-join-integration/variable-arene-layout-todo.jpg)

## Automated verification

The complete local model suite passes **351 tests, zero failures, ten ignored**.
This includes 59 generic tests: all six raw arene poses, all five substituted
ring poses, one/three unique output graphs, 120° attachment angles, fixed host
coordinates, rigid fragment distances, saved/refreshed ink bounds, native
round-trips, stereo/map/isotope distinctions and source-atomic refusals for
crossing, touching, precision and work-limit cases. The ten ignored tests
include the two explicitly deferred six-contact Co(en)3 tests under issue #246;
those are separate from this correction.

The first focused run passed 55 and failed three assertions. The final revision
adds a folded-branch connector collision guard and corrects test fixtures:
five reversible binary sites have 20 unique products, real implicit hydrogen
refresh must differ from explicit-H storage, and an ink-collision fixture must
overlap the actual refreshed glyph box. Original failure logs are retained.

Native new-tab history, save/reopen, matched after captures, normal commit
checks, a qualified default build and exact-head CI remain pending.

## Limits

Raw admission remains at 500 candidates and 100,000 raw atoms before duplicate
removal. Copies, chemistry, layout, display measurement and identity comparison
share the existing 5,000,000 work allowance. An exhausted budget or invalid
candidate rejects the request without installing partial output.

This is a local rigid placement, with explicit refusals for shared/bridged
anchors, unsupported concave or overlapping poses, and mixed variable/link/repeat
placement on the same component. It does not redraw internally crowded fragments
or arbitrary independent artwork. Explicit hydrogen vertices may remain distinct
from attached-hydrogen representations. See the [workflow](../generic-structures.md)
for other retained restrictions.

Join selection/placement accuracy and Co(en)3 appearance remain TODO;
[issue #246](https://github.com/Ameyanagi/ReShiki/issues/246) stays open. Combined
native Windows and measured above-100% DPI acceptance remain pending.
