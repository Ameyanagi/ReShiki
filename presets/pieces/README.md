# ReShiki equipment pieces

A piece is one item of lab equipment: a flask, a condenser, a clamp, a hose. Built-in pieces and pieces you make share one file type, `.reshiki-piece`. The file holds data only: outlines, joints, holds, parameters, movable parts and liquid regions. It has no scripts, no formulas and no links to other files, so a piece someone emails you cannot run anything.

The format is a **preview** (`"format_version": 0`). ReShiki can check pieces now, and the built-in pieces can be placed in a drawing; placing your own pieces comes in a later release. Release builds that know only format 1 refuse format 0, so nothing written before the format is frozen is read wrongly later.

## Check a piece

```sh
reshiki --cli pieces check my-flask.reshiki-piece other.reshiki-piece
```

It prints one JSON line with a result per file, and exits 1 when any file fails. Each message names the field to fix, for example `Joint "top" is 3.2 mm from the outline. Place it on the rim of the opening.` [schema.json](schema.json) describes the structure for editors; the check adds the geometry the schema cannot express.

## Format

UTF-8 JSON, at most 1 MB. Lengths are millimetres, in the piece's own frame, with y pointing down (as in SVG). An unknown field is an error that names the field.

| Field                      | Meaning                                                                                                                                                  |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `format`, `format_version` | `"reshiki-piece"`, `0` (preview)                                                                                                                         |
| `id`, `revision`           | `namespace/name`, each 1–64 of `a–z 0–9 -`; `reshiki/` is for built-ins. Bump `revision` when you change a shared piece.                                 |
| `family`, `variant`        | Pieces that differ only in size share a family; `variant` holds the size fields, such as `{"volume_ml": "100"}`                                          |
| `names`, `keywords`        | `names.en` is required; other languages by tag, such as `ja`. Search covers every name and keyword.                                                      |
| `category`                 | `flask`, `schlenk`, `condenser`, `distillation`, `adapter`, `addition`, `filtration`, `nmr`, `chromatography`, `vessel`, `heating`, `holding` or `other` |
| `credits`                  | Optional `author` and `license`                                                                                                                          |
| `units`                    | `"mm"`                                                                                                                                                   |
| `grounded`                 | `true` for equipment that stays put when something attached to it moves: a stand, a hotplate                                                             |
| `shapes`                   | The drawing (below)                                                                                                                                      |
| `joints`, `holds`          | Where other pieces connect (below)                                                                                                                       |
| `params`, `parts`          | Values a user changes, and the shapes they move (below)                                                                                                  |
| `liquids`                  | Closed regions a liquid level fills                                                                                                                      |
| `labels`                   | Named points where a caption or a molecule can be pinned                                                                                                 |
| `clip`                     | `{"size": "15/25"}` for a joint clip. Its frame is the seat's: the socket mouth at the origin, +y into the socket.                                       |
| `tube`                     | `{"width_mm", "material", "color", "ends"}` for tubing drawn along a centre line stored on each placed copy                                              |

### Shapes

Each shape has an `id`, a `layer`, a `material`, a `role` and a `path` in SVG path syntax (`M L H V C S Q T A Z`).

- `layer`: `back`, `body`, `inner` or `front`. A setup paints every back layer, then the glass, then the inner layers, then the body details, then every front layer, so one clamp finger passes behind a neck and the other in front.
- `depth` (−8 to 8) orders shapes within a layer across a setup.
- `material`: `glass`, `frosted`, `ptfe`, `rubber`, `plastic`, `metal`, `porcelain`, `cork`, `paper`, `silica`, `sand`, `liquid` or `ink` (painted like outlines, for a stopcock bore). `color` overrides the paint.
- `role`: `glass` shapes on the body layer join one silhouette, like fused glass; `solid` is filled and outlined; `detail` is a line.
- `style`: `line` or `shaded` limits a shape to one style, such as a highlight.
- `part`: the movable part the shape belongs to.
- `when`: `{"stopcock": ["open", "all"]}` draws the shape only in those states.

### Joints

- **Taper** (ground glass): `size` is one of `10/18`, `14/20`, `14/23`, `15/25`, `19/22`, `19/28`, `19/38`, `24/29`, `29/42`; `gender` is `M` or `F`. ReShiki draws the socket or cone itself, so every 15/25 joint looks and seats alike. For a socket (`F`), `at` is the centre of the mouth and `dir` points out of it; draw your neck so that it meets the socket's base, L + 4 mm inside the mouth, at the size's neck diameter (13 mm for 15/25). For a cone (`M`), `at` is the shoulder centre and `dir` points to the tip; your tube ends 3.6 mm behind the shoulder. A cone that is not ground glass, such as a rubber septum or a PTFE sleeve, sets `"draw": "none"` and draws its own end; its `at` must sit on that drawing.
- **Plug**: `family` is `hose`, `stopper`, `septum`, `thermometer`, `nmr` or `stir`, with `size_mm`. `at` sits on the end of the barb, stem or opening.
- `part` moves the joint with a part. `poses` gives it another position in a state: `{"turn": {"r1": {"at": [..], "dir": [..]}}}`.

A seated cone sits 0.06 L outside the socket mouth (2.52 mm for 29/42).

### Holds

A segment something grips or slides along: `kind` is `rail` (a stand rod), `slider` (a boss head), `grip` (a neck) or `jaw` (a clamp), with `from`, `to` and `diameter_mm` (one number, or `[min, max]` for a jaw).

### Parameters and parts

- `range`: `min`, `max`, `default` and `unit` (`mm`, `deg` or `fraction`).
- `states`: a list of `states` and a `default` state.
- `fill`: the liquid level of a `liquid` region, from 0 to 1: the share of the region's drawn area below the surface. The surface stays level on the page when the piece turns.
- `color`: the colour of a `liquid` region.

A part names one parameter and a `motion`, measured from the parameter's default, so the shapes as drawn are the default pose:

- `translate`: millimetres per unit, `[x, y]`.
- `rotate`: `{"pivot", "degrees_per_unit"}`.
- `scale`: `{"pivot", "per_unit"}`, a uniform factor of 1 + per_unit × (value − default), for a balloon.
- `stretch`: `{"pivot", "axis", "per_unit"}`, the same along one axis only, for clamp fingers around a thicker neck.
- `per_state`: an SVG matrix `[a, b, c, d, e, f]` per state, for a valve that lifts.

A part may name a `parent` part, up to 4 deep, and a `handle` (`at`, `gesture` `drag` or `toggle`).

## Limits

256 shapes, 32 joints, 16 holds, 16 parameters, 32 parts and 8 liquid regions per piece; 20,000 path points; coordinates within ±2000 mm; text up to 200 printable characters; 64 keywords.

## For developers

The format, validator, evaluator and paint plan live in `crates/model/src/equipment/`. The fixtures in `crates/model/src/equipment/fixtures/` cover the hard cases: a three-way stopcock, a J. Young valve, a cow adapter turned to each receiver, a clamp that tilts and fits the neck, a thermometer's depth, an NMR sample height, a balloon, hoses, angled side necks and a Kugelrohr train. `paint-plans.json` pins the geometry each one produces at every state and at both ends of every range. After an intentional change, run `RESHIKI_UPDATE_PIECE_GOLDENS=1 cargo test --locked -p reshiki-model --lib equipment`, then `bun fmt crates/model/src/equipment/fixtures/paint-plans.json`, and review the diff.
