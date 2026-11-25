# Orbital surfaces as vector shapes (xTB Phase 2d prototype)

Status: the renderer below, and since Phase 5 real orbitals chosen in the
results palette (see [Phase 5](#phase-5-real-orbitals-in-the-drawing)). The
floating viewer is not built yet.

The owner chose two ways to show orbital isosurfaces: an interactive viewer
that turns smoothly, and vector shapes in the drawing that export to SVG and
PDF. The vector half carries the risk: SVG and PDF have no depth buffer, so
the drawing order of shapes has to be right everywhere. This prototype proves
that with an analytic field before any xTB data exists.

## The problem

The 3D model renderer (`crates/model/src/models3d.rs`, `push_model`) draws
each atom as a shaded disc and each half-bond as a shaded quad. It sorts them
by one depth per item (painter's algorithm) and handles overlapping balls
exactly with 2D contact regions (`models3d/contact.rs`). That works because
balls and sticks are few, convex and rarely interpenetrate in ways a single
depth cannot order.

An isosurface breaks those assumptions:

- A lobe of a p orbital passes through its own atom's ball: near the nucleus
  the surface lies inside the ball, further out it is in front of it on one
  side and behind it on the other. No single position in a depth sort is
  right for the lobe, or even for one triangle that straddles the ball.
- A lobe wraps around a bond: in front of it at one end, behind it at the
  other.
- Two lobes overlap on screen, one in front of the other, and a fold of a
  non-convex lobe overlaps another part of the same lobe.

Sorting triangles by their centroids (what matplotlib and most quick vector
exporters do) gets most pixels right and some visibly wrong; review finding
19 asks for explicit hidden-surface clipping or splitting instead.

## Decision summary

1. The surface is a closed, consistently oriented triangle mesh per phase
   sign, built in model space (Å) and cached outside the document.
2. Each frame, in screen-depth space (the projective map the renderer
   already uses for x and y, plus a matching depth): drop back faces; cut
   away every part of a front face that a ball or bond hides, using
   occluders built from the shapes the renderer actually paints; then keep
   of each piece only what no nearer overlapping piece covers. Two
   non-intersecting flat pieces have one fixed front-to-back order wherever
   they overlap, so this needs no global sort and no BSP tree.
3. Merge what is visible into a few shapes per phase: nested shading bands
   (each one filled region) and the visible outline runs. Triangles never
   reach SVG, PDF or the document.
4. Draw the molecule exactly as today, then the merged surface shapes on top,
   then the labels of atoms in front of the surface.
5. Files stay light (owner, 2026-11-25): a figure stores its parameters and a
   compact snapshot of the merged 2D shapes (≤ 300 KB), never meshes or
   grids. Export shows a size estimate and warns above 5 MB.

## Data

Model space is the frame of `Model3d::atoms[*].xyz` (Å).

    Mesh {
        positions: Vec<[f32; 3]>,      // model space, Å
        normals:   Vec<[f32; 3]>,      // unit, outward (away from the lobe)
        triangles: Vec<[u32; 3]>,      // counter-clockwise seen from outside
        positive:  Vec<bool>,          // phase sign of each triangle
    }

- Closed and two-manifold: every edge belongs to exactly two triangles with
  opposite directions. This is what makes back-face culling exact (an opaque
  closed surface never shows its inside) and keeps the Euler characteristic
  meaningful for tests.
- Per-vertex normals come from the field gradient at the vertex, not from
  the facets, so shading stays smooth after simplification.
- Meshes are derived data. They are never stored in `Document`, never
  cloned into undo history and never written to native files (review
  finding 17). A small thread-local cache keyed by (model geometry bits,
  field, isovalue, detail) holds the last few. Phase 5 will key it by the
  orbital result instead of the analytic field.

Colour follows the orbital phase convention the 2D orbital shapes already use
(`scientific::Phase`, `orbital_parts`): the positive phase is the coloured
lobe and the negative phase the paper-coloured one. `SurfaceStyle` maps a
phase style and a colour to two base colours:

| Phase style     | positive lobe                            | negative lobe                              |
| --------------- | ---------------------------------------- | ------------------------------------------ |
| Solid (default) | colour                                   | paper, lightly greyed so it reads on white |
| Shaded (gray)   | colour lightened two thirds toward paper | paper                                      |
| Gradient        | as Solid (the shading is the 3D light)   | as Solid                                   |
| Open            | paper                                    | paper (outline carries the shape)          |

The prototype uses Solid with a mid blue (`#3d6fd6`) so screenshots read at
a glance; the UI lane exposes the choice.

## Building the mesh

    field f(x) ──► sample grid (bounded) ──► marching tetrahedra at +iso and at
    −iso (on −f) ──► weld vertices by grid edge ──► orient outward ──► vertex
    normals from ∇f ──► quadric edge-collapse simplification to the budget

- `iso` must be finite and positive.
- Grid: an axis-aligned box around the atoms, cubic voxels, at most 96
  voxels on the longest side and at most 1.5 million samples. If any sample
  on the box's outer layer has |f| ≥ iso, the surface would be cut: the box
  grows by a quarter on that side and is sampled again, up to three times;
  after that the mesh is flagged `truncated` and the caller reports it (the
  analytic demo never is). Then one extra layer of samples on every side is
  forced to zero, so every surface closes even when truncated.
- Sampling resolution limits what can be seen: a lobe thinner than about
  two voxels can vanish. Phase 5 picks the spacing from the basis (tightest
  exponent) and reports it.
- Marching tetrahedra, written from the published method (each cube split
  into six tetrahedra around its main diagonal, the same split in every cube
  so neighbouring faces match). It has no ambiguous cases and needs no case
  tables. Marching cubes with the original 15-case table can leave holes at
  ambiguous faces, which would break the closed-mesh assumption.
- Ties: a sample within 1e-6·iso of the level is moved just outside it
  before extraction. With no sample exactly on the level, the level set of
  the piecewise-linear interpolant is a two-manifold (also at critical
  levels such as a double cone, which the grid resolves into a pinched
  neck or two sheets), every crossing lies strictly inside its grid edge,
  and welding by grid-edge id gives a closed, consistently oriented mesh.
  Tests check edge use, vertex links (one cycle per vertex) and Euler
  characteristics.
- Orientation: a triangle's normal points away from the tetrahedron corners
  inside the surface. Vertex normals: `−∇f/|∇f|` for the positive surface,
  `+∇f/|∇f|` for the negative one (central differences of f at the vertex);
  where |∇f| is near zero, the area-weighted normal of the meshed triangles.
- Work bounds: extraction stops at 3 million raw triangles and retries on a
  grid with twice the spacing (a pathological field such as a ±1
  checkerboard cannot exhaust memory).
- Simplification: Garland–Heckbert quadric error metrics with edge
  collapses, written fresh from the 1997 paper. A collapse is refused if it
  would break the link condition (keeps the mesh manifold and closed), flip
  any neighbouring triangle normal, produce a needle triangle, or make any
  new triangle intersect a non-neighbouring triangle of either sign (checked
  against a spatial hash). The last check keeps the mesh embedded, which the
  visibility step relies on. Target counts: 20 000 triangles for the drawing
  and export, 4 000 while the model is turning. If no legal collapse is left
  above the target (thousands of tiny separate lobes), the mesh stays larger
  and is reported; the merged output stays small either way.

## Each frame

### 0. Screen-depth space and the frame

The renderer projects a rotated point `p = (x, y, z)` (view space, Å, z
toward the viewer) to the drawing as `origin + scale·f·(x, −y)` with
`f = c / (c − z)` under perspective (camera distance `c`) and `f = 1`
without. Adding the depth `Z = scale·f·z` gives a map

    S(p) = (X, Y, Z) = (origin.x + scale·f·x, origin.y − scale·f·y, scale·f·z)

that is projective (one common denominator `c − z`), so it maps planes to
planes and straight edges to straight edges, and keeps depth order along
every sight line. In S-space every sight line is parallel to Z: the view is
orthographic, the eye is at Z = +∞, and a point hides another exactly when
it has the same (X, Y) and a larger Z. All visibility work below happens in
S-space; X and Y are already drawing units.

The map needs every surface point in front of the camera. The view's reach
(the radius that sets the scale and the camera distance, `View::new`)
therefore includes the farthest mesh vertex when a surface is shown, as it
already includes a crystal's cell. The camera sits at 1.2 × reach or more,
so the eye is never inside a lobe, and the frame and Card hold the whole
picture. Showing a surface may make the molecule smaller in its frame;
that is the same rule crystals follow.

### 1. Cull

A triangle is a front face when the eye is on the outer side of its plane
(tested in view space). Back faces are dropped. For a closed, embedded,
outward-oriented surface seen from outside (guaranteed above) this is exact,
also after the clipping below: wherever clipping opens a hole in the
surface, the hole lies in an occluder's shadow, so whatever it would reveal
is hidden too.

### 2. Clip against the molecule as painted

A surface point must disappear exactly where the molecule paints something
in front of it. So occluders are built from the painted shapes, not from
ideal spheres and cylinders (a perspective ball is painted as a disc of
radius `r·scale·f`, which can be much smaller than the sphere's true
outline). Each occluder is a convex solid in S-space:

- Ball: its footprint is the painted disc (centre S(c), radius r·scale·f).
  Its front is a cap over that disc whose depth at the centre is
  Z(c) + Δ, with Δ the S-depth of `c + r·ẑ` minus that of `c`; it falls to
  Z(c) at the rim like a hemisphere. In S-space this solid is a spheroid
  half, convex.
- Half-bond (Ball and stick, Stick): the painted quad from the projected
  atom centre to the projected bond middle with the painted half widths at
  each end, raised to a roof whose ridge runs Δ above the axis (the
  cylinder's front, as a half-hexagon cross-section). Its end at the atom
  lies inside the atom's ball footprint, which hides everything there
  anyway, so trimming at the ball exit does not matter.
- Stick joints: in Stick style each atom is also a ball occluder of the
  stick radius, which covers the joint polygons up to their mitre tips
  (a sub-pixel difference at normal sizes).

The shadow volume of a convex solid in S-space is the solid swept toward
−Z: the intersection of the half-spaces behind each front-facing face of
its proxy and of the vertical planes through its outline edges (the
shadow-volume construction of Crow, 1977, with the eye at infinity). The
proxies:

- Ball: `SEGMENTS = 64` around and `RINGS = 12` rings. The outline polygon
  is inscribed in the painted circle (gap ≤ 0.0012 r). The cap faces are
  circumscribed: each face's plane touches the cap at the face's middle and
  lies outside it elsewhere, so a lobe that lies wholly inside a ball can
  never leak through between cap faces. The price is hiding a surface
  that is in front of the ball by less than the cap's sagitta, which only
  moves the drawn intersection curve slightly.
- Roof: 6 faces along the stick plus the outline of the painted quad.

A convex polygon minus such a volume, without a polygon library:

    remaining = piece
    for each half-space h of the volume:
        split remaining by the plane of h
        the part outside h is visible: keep it (convex)
        remaining = the part inside h; stop if empty
    remaining is hidden: drop it

Pieces are tested against an occluder's footprint bounding box first, and
skipped if they are wholly in front of its front (largest Z of the cap);
a piece wholly behind it is clipped like any other.

Not in the prototype (no surface is drawn for these models, and the
switch says so): crystals (repeat copies are translucent and the cell's
front edges, origin and axis labels are drawn over the model, which needs
compositing the prototype does not do), Wireframe (labels and lines have
no ball to sit on) and thermal ellipsoids (proxy not wired up). During the
2D → 3D morph the surface is not drawn.

### 3. Exact visibility among surface pieces

The remaining pieces are convex, flat and, because the mesh is embedded,
never pass through each other. For two such pieces A and B whose
projections overlap, the depth difference Z_A − Z_B is affine over the
overlap and cannot change sign there without the pieces crossing, so one
of them is in front over the whole overlap. That fixes the visible part
of each piece directly:

    visible(A) = A minus every B that overlaps A on screen and is in front of
                 A at a point inside their overlap

- Overlapping pairs come from a uniform bucket grid of projected bounding
  boxes; pieces that only touch (shared edges and vertices of neighbouring
  triangles) do not count as overlapping.
- The comparison point is the centroid of the 2D intersection of the two
  convex projections (strictly inside it).
- No global order is needed, so cyclic overlaps (which no sort can order)
  are handled like any other, nothing is split to break cycles, and there
  is no BSP tree, no fragment growth and no fallback.
- Cost: on a convex lobe no two front faces overlap, so nothing is
  subtracted; work grows with the area where sheets overlap (a lobe behind
  another, a fold), and only those pieces go through `i_overlay`.
- Pieces with nearly equal depth over their overlap (two sheets of nearly
  touching lobes) may resolve either way; they are at the same place on
  screen within the depth tolerance, so the picture does not change.

### 4. Combine with the molecule

    Card (if any)
    molecule items (balls, half-bonds, labels) in today's order   ← unchanged
    surface layers and outline                                     ← new
    labels of atoms in front of the surface                        ← moved

Correctness, point by point on screen:

- If the molecule paints something in front of the surface at a point, the
  surface there lies in that item's shadow volume and was cut away, so the
  molecule's paint shows.
- If the surface is nearest, its visible part is painted after the
  molecule, and among surface pieces only the nearest is left there.

This guarantees surface-versus-molecule and surface-versus-surface
visibility. The molecule's own paint order is today's, including its known
approximations (a long tilted half-bond sorted by its middle depth); the
surface does not make those better or worse.

Labels: a label is ink over the picture, not a solid. On the sight line
through its atom's centre, if no surface piece is in front of the front of
the atom's ball and no other, nearer ball covers that point, the label is
drawn after the surface (so a lobe behind the atom never cuts the parts of a
wide label that reach past the ball); otherwise it stays in the molecule
order, where the surface or the nearer ball covers it.

Painted outlines: a ball's occluder is its disc grown by half its outline
width, so a lobe behind it never paints over the outer half of the outline.
A half-bond seen nearly end-on (shorter on screen than its width) is not
painted (`View::joints`, the Ball-and-stick length check), so it casts no
shadow; its atom's ball covers that spot.

Outline runs shorter than 2 % of the model's size are folds of the
simplified mesh rather than the lobe's outline and are not drawn.

### 5. Shade and merge

- Lightness: each mesh vertex gets ambient 0.35 + 0.65·max(0, n·l) from its
  smooth normal, the balls' light from the upper left in front. It is
  carried as a vertex value through every cut and interpolated linearly in
  S-space (screen-affine, like Gouraud shading), so cuts never change the
  shading field.
- Bands: each visible part is cut along lightness = k/BANDS (straight cuts,
  as lightness is affine on a flat piece), giving smooth contour lines
  across pieces instead of triangle staircases. `BANDS = 10`.
- Merge: for each phase, layer k is the union (`i_overlay`, already a
  dependency) of every visible part of that phase in band k or lighter.
  Layers are drawn darkest first, each over the last, so there are no
  seams between bands; layer 0 is the phase's whole visible region. The
  two phases never overlap (visible parts are disjoint), so their order
  does not matter. Contours are then thinned with Douglas–Peucker at 0.05
  drawing units.
- Depth fade: band colours use the model's fade at its centre depth, so the
  surface fades as a whole (lobes span the model's depth, unlike one ball)
  and cuts never change a colour.
- Outline (flat look, not Glossy): silhouette edges (between a front and a
  back face) keep a flag through every cut. A silhouette segment of piece
  A is visible where no piece in front of A covers it (the same pairwise
  rule) and no occluder hides it (it was clipped with A). Visible runs are
  chained into polylines, thinned, and stroked with the balls' outline
  colour and width. Drawn last, they also cover the anti-aliasing seam
  where one phase's region meets the other or the molecule behind it.

Rejected alternatives:

- One filled path per triangle (flat shading, painter's order): about 70
  bytes a facet (2.8 MB at 40 000 pieces), hairline seams between
  neighbours that need same-colour strokes, and those strokes repaint
  slivers that clipping removed. Far too large for a stored snapshot.
- A BSP tree for ordering (the first draft): exact, but quadratic even on a
  convex lobe (every supporting plane classifies every other piece) and it
  needs a fragment budget with a lossy fallback. The pairwise rule above
  does not.
- Smooth shading with a linear gradient per triangle: exact Gouraud look but
  ~3× the per-facet cost.
- PDF Gouraud mesh shadings (type 4): compact and exact, but SVG has no
  equivalent and our PDF comes from the SVG.

### 6. The primitive

    Primitive::Surface {
        layers: Vec<(Vec<Vec<Point>>, [u8; 3])>,  // regions, nonzero winding, exact colours,
                                                   // drawn in order
        rims: Vec<Vec<Point>>,                     // visible outline runs
        outline: Option<Outline>,
    }

- SVG: one `<path fill-rule="nonzero" fill="rgb(…)" d="M…L…Z …"/>` per layer
  with coordinates to a hundredth of a unit (`push_command`), and one
  stroked path for the rims. Outer contours and holes have opposite winding,
  which fitting preserves.
- PDF: through the existing SVG → usvg → svg2pdf path, as filled paths. No
  rasters.
- Canvas: model primitives are rasterized into cached tiles by resvg from
  the same SVG (`src/canvas/models3d/tiles.rs`). The surface is one
  primitive of a few dozen paths. Its tile key hashes coordinate bits
  directly (its `Debug` text would be megabytes).

### 7. Storage and size

Owner (2026-11-25): meshes and grids are never saved in `.rsk`. A vector
orbital figure placed in a drawing stores only:

- its parameters: which orbital (a reference to the stored calculation
  result, Phase 2a's data model), isovalue, view (the model's rotation and
  perspective, already stored on `Model3d`), phase style and colour;
- a snapshot of the merged 2D shapes for that view: the layers and rims of
  `Primitive::Surface`, relative to the model frame's centre, so moving the
  frame keeps it valid.

Snapshot encoding (one JSON string field, so the file stays JSON):

    base64( deflate(  // little-endian, as `surface::snapshot` writes it
        magic "RSF1",
        outline flag u8, then if 1: colour [u8; 3], alpha f32, width f32,
        layer count u32, per layer: colour [u8; 3], contour count u32,
            per contour: point count u32, then x f32, y f32 per point,
        rim count u32, per rim: point count u32, then x f32, y f32 per point ) )

Points are relative to the model frame's centre. The magic carries the
version; contours have at least 3 points, rims at least 2, layers at least
one contour.

`miniz_oxide` (deflate) and `base64` are already dependencies of the model
crate. Points could be quantised to 1/100 unit as i16 deltas later if the
f32 form is too large; measured sizes decide.

Rules:

- Snapshot ≤ 300 KB encoded. If the merged shapes exceed it, the outline
  tolerance doubles until they fit (coarser bands, same picture at normal
  zoom).
- Builds without xTB, the web build, Python and printing draw the snapshot
  as is. A build with the orbital data recomputes the shapes when the view
  or isovalue changes and replaces the snapshot on commit.
- Turning the model in a build without the data shows the molecule turning
  and hides the surface until it is turned back (the snapshot is only valid
  for its view); the UI lane decides the message.

Export size estimate: the export dialog calls one function that writes the
drawing's SVG (exact, milliseconds) and reports its length, plus the PDF
estimate from a measured ratio of PDF to SVG bytes for surface-heavy pages
(measured below). Above 5 MB (`EXPORT_WARNING_BYTES`) the dialog warns and
suggests a lower detail.

## Budgets

| Quantity                                                      | Budget                                     | Why                                           |
| ------------------------------------------------------------- | ------------------------------------------ | --------------------------------------------- |
| Grid samples                                                  | ≤ 1.5 M (≤ 96 voxels a side + zero border) | 6 MB of f32; analytic and Phase 5 grids alike |
| Triangles, drawing and export                                 | ≤ 20 000 after simplification              | owner's export limit                          |
| Triangles while turning                                       | ≤ 4 000                                    | keeps rotation interactive on the canvas      |
| Raw triangles before simplification                           | ≤ 3 M, else coarser grid                   | memory bound                                  |
| Mesh build (grid, surface, simplify), once per field          | ≤ 1.5 s release                            | cached; not per frame                         |
| Per frame at 20 000 triangles (cull, clip, visibility, merge) | ≤ 150 ms release                           | redraw after a turn stops                     |
| Per frame at 4 000 triangles                                  | ≤ 30 ms release                            | while turning                                 |
| Stored snapshot (base64, deflated f32)                        | ≤ 300 KB                                   | owner: light files                            |
| SVG bytes, one surface at 20 000 triangles                    | ≤ 1 MB                                     | merged shapes, not triangles                  |
| PDF bytes, same page                                          | ≤ 1 MB                                     | same                                          |
| Export warning                                                | 5 MB per file                              | owner                                         |
| SVG write + SVG→PDF                                           | ≤ 2 s release                              | export stays responsive                       |

The PR reports the measured numbers next to these.

## Developer switch

`RESHIKI_DEV_ORBITAL_SURFACE` (read once in `main` before any mode) turns on
an analytic demo surface for every 3D model, in the canvas and in
`reshiki --cli render`:

- `pi`: π-like orbital, a p function `(n·d) e^(−1.6|d|)` on every heavy atom,
  `n` the normal of the molecule's best-fit plane, all with the same sign
  (ethylene π: one positive lobe above the plane, one negative below).
- `pistar`: the same with signs alternating across bonds (ethylene π*: four
  lobes, which overlap each other on screen when turned).
- An optional `:iso` suffix sets the isovalue (default 0.06).

The switch draws only; nothing is saved. Unset, the renderer runs exactly the
code it runs today (the surface hook returns before any work), which the
existing render tests confirm.

## Proof

- Topology: fields with known level sets: a sphere (one closed surface,
  Euler characteristic 2), two separate lobes of opposite sign (two
  surfaces, χ = 2 each), a torus (χ = 0), each before and after
  simplification; closed two-manifold; consistent outward orientation
  (signed volume positive); sphere radius within 2 % of exact; triangle
  counts within budget.
- Exact occlusion (oracle test): for scenes with a lobe passing in front of
  and behind a ball and a bond, at several rotations and with and without
  perspective, cast rays at a few thousand screen points against the true
  spheres, cylinders and the mesh triangles; the point must lie inside the
  merged region of a phase exactly when the ray first hits the mesh of that
  phase, and inside no surface region when it first hits the molecule or
  nothing. Points within the stated approximation band of an outline or
  intersection curve are skipped and counted.
- Unchanged output: the existing models3d, scene and canvas render tests
  pass unchanged; with the switch unset `primitives()` is byte-identical.
- Export: the same scene to SVG and PDF; SVG has no `<image>`, PDF has no
  image XObject; bytes and times within budget at 20 000 triangles; snapshot
  round trip (encode, decode, same shapes) and its size.
- Evidence: canvas screenshots before (no surface) and after, rendered SVG
  and PDF pages.

## Looks, opacity and smoothness (L6b)

The owner's review of the first renders (25 Nov): "the surface is not
smooth", and "we need alpha and mesh also". Three things were wrong: the
shading was ten flat, visible bands; outlines were slightly lumpy and
polygonal at zoom; and short stray marks showed near edges (a gap in an
outline run). This section replaces the opaque-only, ten-band design above
where they differ.

### Look (persisted with a placed figure)

    Look { fill: Solid | Shaded | Outline | None, mesh: bool, opacity: 20..=100 }

| Field     | Values                               | Default | Meaning                                                                                |
| --------- | ------------------------------------ | ------- | -------------------------------------------------------------------------------------- |
| `fill`    | `solid`, `shaded`, `outline`, `none` | shaded  | flat phase colours; lit and smoothly shaded; paper-coloured fill with outline; no fill |
| `mesh`    | `true`, `false`                      | false   | a light wire grid on the surface, over the fill (`fill: none` needs `mesh: true`)      |
| `opacity` | integer percent, 20 to 100           | 100     | the surface as a translucent film: what lies behind a lobe shows through it            |

Serialized as `{"fill": "shaded", "mesh": false, "opacity": 100}`; reading
validates once (opacity range, `none` without mesh is rejected). The figure
stores the look with its other parameters, never a mesh. The UI lane shows
Look as Solid · Shaded · Outline · Mesh (Mesh = `fill: none, mesh: true`),
a "Mesh lines" check box for the other three, and an Opacity slider. The
silhouette outline is drawn in every look when the model's own look draws
outlines (Flat), as for the balls.

Developer switch: `RESHIKI_DEV_ORBITAL_SURFACE=pistar:0.06,look=shaded+mesh,opacity=60`
(`look=` takes `solid`, `shaded`, `outline` or `mesh`, the first three
optionally `+mesh`).

### Smooth geometry

- After simplification the mesh is relaxed with Taubin's λ|μ smoothing
  (no shrinking), then Newton steps along ∇f project the vertices toward
  their level set, and normals are taken from ∇f again. Refinement
  restores a vertex if its total displacement exceeds half a grid step
  or if the move would flip a face, make a needle or make the mesh pass
  through itself. This removes the lumps that the quadric simplifier
  leaves at lobe tips (25 ms for 20 000 triangles).
- Every merged contour, outline run and mesh line is fitted with cubic
  Béziers (Schneider 1990): outlines, outline runs and mesh lines to 0.04
  drawing units at their input points (tight, so an outline cut by a ball follows the painted rim), shading-tone contours to 0.75 units
  (they are one-level edges under a clip, see below). The result is C1
  except at real corners: a turn over 40° measured over chords at least
  max(3 × error, 1 unit) long, such as where a ball cuts a lobe, so the
  zig-zag of a triangle mesh's silhouette is not a corner. Every span
  between smooth knots is a cubic sharing its neighbours' tangent; a line
  appears only between corners on collinear points. A fitted span is split
  again at its worst input point if its end control segments cross or a
  handle is longer than its chord (low error at the input points alone
  cannot rule out a loop between them), and a contour whose fit crosses
  itself is refitted at half the error before it falls back to its input
  polygon. On benzene π* the fill paths shrink from 72 622 to about 30 700
  commands, 4 % of them lines, with no polygon fallbacks.
- Fills use the nonzero rule: fitted contours of one region may touch or
  cross by twice the error, which even-odd would turn into small holes;
  `visible()` produces outers with positive signed area (counterclockwise
  in XY) and holes with negative signed area, which fitting preserves.
- Sliver filtering uses unthinned contours in regions with holes to find
  each shape's outer and direct holes, then drops the shape and all nested
  contours if its net area is under 0.05 square units or its width (2·net
  area / combined perimeter) is under 0.12 units. Remaining holes under
  0.12 units wide and holes without a containing contour are dropped.
- Outline runs shorter than 1 unit are dropped before fitting. This replaces
  the old 2 % of model size cutoff, which cut legitimate short outline runs
  between crossing lobes and left gaps. Mesh-line runs keep pieces down to
  0.25 units. Strokes have butt ends so a run ends where the nearer piece
  hides it.
- Fitting moves a boundary by at most the fit error; the ray oracle checks
  the fitted outlines too, with the same approximation band as before.
- Paths are written in relative SVG commands to a hundredth of a unit, the
  offsets taken between rounded points so rounding never accumulates.

### Smooth shading

The shading is still nested regions of the interpolated Lambert lightness
(cuts never change the shading field), each drawn over the last, but with
enough tones that one step is at most one 8-bit level, which no 8-bit
display or viewer can show as a step. The tone axis is even in colour, not
in lightness: each phase's ramp (darkening below lightness 0.6, lightening
above) is measured along its steepest channel in each piece, and every
vertex's lightness is mapped onto that measure, so equal tone steps are
equal colour steps:

    tones = clamp(ceil(levels × weight), 2, 128)

where `levels` is that measure for the steeper phase (114 for the blue
phase at the default depth fade, about 45 for the pale one) and `weight` is
the group's share of the final colour (1 when opaque, α(1 − α)^d for depth
group d below). While the model turns, tones are capped at 16 and depth groups at 2; the full
shading returns when it stops.

Each tone region is clipped to its phase's whole region (an SVG
`clipPath`), so tone outlines fitted with the looser error never reach
past the lobe's edge, and nested regions keep any fitting error to a local
tone shift. (Strips of two or three tones were tried to avoid repeating
the region's edge in every tone; where tones crowd near a silhouette the
independently fitted strip edges left gaps that showed the darkest tone,
so nesting stays.) The whole region is merged directly from the resolved
fragments before shade cuts, so its contour and fitted commands stay
identical when opacity changes the tone count. Each tone's fragments are
merged in parallel before the sequential nesting, and the layers are
fitted in parallel.

Considered and not used:

- Canvas: iced 0.14's wgpu meshes take per-vertex colours (true Gouraud),
  but its tiny-skia renderer draws no meshes at all, and the canvas draws
  3D models from cached resvg tiles of the same SVG as the export, which is
  what keeps late labels above the surface and canvas and export identical.
- PDF type 4 (free-form Gouraud) shadings: our PDF is written by svg2pdf
  from the usvg tree that the clipboard, page export and Python share;
  usvg has no mesh paint, so this would mean patching the vendored usvg and
  svg2pdf (and resvg, which matches on usvg's paints) or splitting every
  page into interleaved SVG and shading chunks. At one level per step the
  vector regions already render like a smooth shading in 8-bit viewers, so
  PDF uses the same regions as SVG.
- A linear gradient per band: a band is curved, so one gradient direction
  is wrong along most of it.

### Opacity: depth groups

With opacity α < 1 the hidden layers are needed. For a point on a surface
piece, its depth count is the number of front pieces in front of it there
(the same exact pairwise rule as §3). Depth group d holds everything with
count d; each piece is split into convex fragments by count with the same
half-plane splits used against occluders, so all of it stays exact:

    molecule items (as today)
    group G−1 … group 1, group 0     each composited as a whole at its alpha
    labels of atoms in front of the surface

- Front faces only carry fill (back faces are culled as before), so a ray
  through one lobe is attenuated once, an atom inside a lobe shows through
  its front face, and a lobe behind another shows through it. Balls and
  bonds stay opaque, so a lobe part behind an atom is still cut away.
- Each group is one SVG group with `opacity="α"`: its tones are opaque
  inside the group, so there are no seams or doubled alpha between pieces.
- Rims count covering triangles, with the intervals of a triangle's clipped
  pieces united first. A shared clipping cut along a farther rim therefore
  attenuates that rim once. A front piece counts only if it is more than
  0.5 drawing units nearer in S-space Z at the interval midpoint, so
  shallow mesh folds stay in the same rim group.
- The number of groups is the smallest G with α(1 − α)^G < 1/50, at most 6
  (G = 1 at 100 %, when the output is exactly the opaque one).
- Cost: fragments grow only where sheets overlap; a piece is split at most
  into 4096 fragments (never reached in the demo scenes).

### Mesh look

Lines are the surface's intersections with three families of parallel
planes 0.3 Å apart, perpendicular to the principal axes of the atoms (so
the grid turns with the molecule and does not depend on the mesh
triangles). They lie on the surface, so they are cut exactly like it: a
segment on a triangle is clipped to that triangle's fragments, which
already exclude what the molecule hides and carry the depth count.

- At 100 % only front lines that nothing covers are drawn (hidden lines
  removed). Below 100 % lines behind d front faces are drawn in group d,
  and lines on back faces are cut the same way (back faces are clipped
  against occluders for this only), so the far side of a lobe shows
  faintly through the front.
- Over a fill, lines are a darker shade of the phase colour, about half the
  ball outline width. With `fill: none` there is no film to attenuate them,
  so group d is drawn at α(1 − α)^d.
- Order inside a group: fills, mesh lines, then the outline runs.

### Primitive and snapshot

    Primitive::Surface { groups: Vec<SurfaceGroup> }      // deepest first
    SurfaceGroup { alpha, fills: Vec<SurfaceFill>, strokes: Vec<SurfaceStroke> }
    SurfaceFill { region: Vec<PathCommand>, color, tones: Vec<(Vec<PathCommand>, color)> }
    SurfaceStroke { paths: Vec<PathCommand>, outline: Outline }

Regions, tones and tone clips use nonzero winding. Holes wind opposite
their outer contours.

The snapshot format becomes `RSF2` (RSF1 was never released, so it is not
read):

    base64( deflate(  // little-endian
        magic "RSF2",
        group count u32, per group: alpha f32,
            fill count u32, per fill: colour [u8; 3], path, tone count u32,
                per tone: colour [u8; 3], path
            stroke count u32, per stroke: colour [u8; 3], alpha f32, width f32, path
        path = command count u32, per command: tag u8 (0 move, 1 line,
               2 cubic, 3 close), then 1, 1, 3 or 0 points as x f32, y f32 ) )

Encoding drops alternate tones until the snapshot fits the 300 KiB encoded
size limit, the 16 MiB inflated size limit and the shared limit of
1 000 000 path commands. If the regions and strokes alone exceed a limit,
encoding returns no snapshot.

## Known limitations of the prototype

- One surface (one orbital, both signs) per model.
- An outline run less than half a drawing unit behind a nearer sheet is
  not hidden by it (the rule that keeps mesh folds from breaking outlines);
  at a typical scale of 20 units/Å that is a 0.025 Å gap.
- PDF shading uses the same one-level vector tones as SVG, not PDF
  Gouraud meshes (see "Smooth shading").
- Shading tones are mapped per vertex and interpolated per triangle, so a
  triangle that straddles the ramp's bend (lightness 0.6) differs slightly
  from the exact ramp inside it; at 20 000 triangles a triangle spans a
  small part of the ramp, so the error is under a level or two.
- No surface on crystals, in Wireframe or in thermal ellipsoids, and none
  during the 2D → 3D morph (see section 2).
- The molecule's own paint order is unchanged, with its existing
  approximations.
- Occluders follow the painted disc `r·scale·f`. Balls that touch another
  ball are painted from their true perspective outline instead
  (`contact.rs`), which is larger by `1/√(1 − (r/d)²)` for a ball at
  distance `d` from the eye: under 1 % of the radius at the default
  perspective, more at full perspective, where a lobe just behind such a
  ball's rim can show by that margin. Stick joints' mitre tips and the
  trimmed start of a half-bond in front of its ball are likewise covered
  only approximately (sub-pixel at normal sizes).
- The surface counts toward the view's reach also during the morph, so the
  molecule keeps one size from the first frame of the morph to the end,
  when the surface appears.

## Phase 5 contract (for the orbital lanes)

The analytic demo fixes the renderer side. Real orbitals must hand it a
signed scalar field `f(x)` in the model's own frame (Å, the coordinates of
`Model3d::atoms` and its generated hydrogens, in that atom order), with:

- the orbital identity (index, spin channel) and the sign convention of
  the stored coefficients; density-like cubes (density, spin density, FOD)
  are unsigned and use only the positive surface;
- cube grids: origin, three step vectors and units (xTB writes Bohr),
  converted once at import and registered to the frozen conformer;
- a test on an asymmetric, translated molecule with one nodal plane known
  independently, so a unit, axis or atom-order mistake cannot produce a
  well-formed lobe on the wrong atom.

## Phase 5: real orbitals in the drawing

The prototype above drew an analytic field behind a developer switch. Phase 5
draws calculated orbitals, chosen in the results palette's Orbitals tab.

### Two states: preview and placed figure

- **Preview.** Choosing an orbital in the Orbitals tab draws it on the
  molecule's 3D model at once. The preview is editor state, not a drawing
  edit: it lives on the canvas's drawing copy (like the optimization step
  preview), costs no Undo step, is not saved and goes away when the palette
  closes, another molecule's result is shown or the result turns stale. A
  molecule shown as 2D only is shown as Both in the preview.
- **Placed figure.** "Place on canvas" turns the preview into part of the
  drawing in one Undo step ("Place orbital"): the model's `surface` field
  (below) is set, a 2D-only model becomes Both, and an xTB result gains the
  orbital's coefficient row (pinned, see Storage). From then on the tab edits
  the placed figure: choosing another orbital, the isovalue, the look, mesh
  lines and opacity are each one Undo step (a slider drag is one step, live
  while dragging), and "Remove from canvas" takes it away. A placed figure
  moves, rotates, copies with its model in the document, saves, prints and
  exports like the rest of the model.

### The figure

    Model3d::surface: Option<Figure>      // serde: absent when None

    Figure {
        orbital: OrbitalRef { result: String, spin: Spin, index: u32 },
        iso: f32,                      // a.u. (bohr^-3/2), 0.005..=0.2
        #[serde(flatten)] look: Look,  // {"fill","mesh","opacity"}
        snapshot: Option<Snapshot>,    // written by Document::current()
    }
    Snapshot { view: String, reach: f32, shapes: String }   // shapes: RSF2

- `OrbitalRef::result` is `science::result_key(result)`: 16 hex digits of
  SHA-256 over the result's frame hash, input frame hash, task, settings and
  provenance (not its orbitals, so pinning rows keeps the key). `index` is
  the zero-based MO index in that spin's spectrum (`Spin::Alpha` for
  restricted and same-orbital results).
- A figure is drawn only while its result is in `Document::science` and its
  attachment is current; a stale or deleted result draws nothing (the
  palette says why).
- Documents with a figure need version 42 (`CALCULATIONS`), like results.

### Meshes: an editor-side registry

Meshes are never stored in the document or its history. A process-wide
registry (`surface::draw::register`, `surface::draw::registered`), keyed by
`surface::draw::key(doc, model)`, holds the last few `Arc<[Mesh; 2]>`
(drawing and turning detail). The key hashes the figure's result key, spin,
index and isovalue bits with the model's points (elements and position bits,
in frame order): any change of geometry or orbital is a new key. The
renderer only looks meshes up; the editor builds them off the UI thread and
registers them, then asks for a redraw:

    coefficients (stored rows, or recomputed by the xTB worker)
      -> OrbitalField (basis, one MO row, centres)        surface::orbital
      -> orbital_meshes(field, points, iso)               grid, mesh, simplify, refine
      -> register(key, meshes)

The canvas, figure export, clipboard images and printing in the editor
process all find the mesh there. Python, the web build, the CLI and builds
without the data find none and draw the snapshot.

### Evaluating orbitals

`surface::orbital::OrbitalField::new(basis, row, centres)` evaluates one MO
of the canonical AO contract (`docs/plans/xtb-data.md`) at points in the
model frame (Å; converted to bohr inside), in atomic units, so isovalues
match cube files and other programs (default 0.05, presets 0.02, 0.05, 0.10).
It is written from the contract's formulas (Cartesian monomials or real solid
harmonics without the Condon–Shortley phase, unit-norm AOs), not ported from
any program. Shells are screened by distance (a shell is skipped where its
most diffuse primitive is below 1e-10 of its peak). The grid is sampled in
parallel; the box is the atoms ± 3 Å and grows as for the demo. Proof: the
signed PySCF and ORCA reference values in `tests/fixtures/qc/reference`
(the same points the readers' tests use) agree to 1e-6 relative, and an
asymmetric, translated molecule with a known nodal plane puts each lobe on
the right atom.

### Snapshot

`Document::current()` (every file, clipboard document and recovery draft)
refreshes each figure's snapshot when its mesh is registered: the surface
primitive for the model's current view, RSF2-encoded relative to the model
frame's centre (≤ 300 KB), the view signature (a hash of everything that
changes the picture except the frame's position: the model's settings,
rotation, size, style, look, points, the figure's parameters and the
canvas theme) and the view reach. Without a registered mesh, the renderer
draws the snapshot when the signature matches the model now, and nothing
when it does not (the model was turned in a build without the data). The
reach keeps the molecule the same size whether the mesh or the snapshot is
drawn.

### Storage

`Science::saved` keeps the coefficient row of every placed figure: the
document passes the pins (`(Spin, index)` per result key) to
`CalcResult::for_file`, so the light form keeps that row and the basis.
Placing a figure on an xTB result adds the recomputed row and the basis to
the stored result (`science::pin_orbital`), so the figure reopens with the
same signs even where recomputing could rotate a degenerate pair.

### Colours

Positive lobes blue, negative orange (the mock's phase colours), each with a
light- and a dark-canvas variant; the swatches in the tab show them. The
analytic demo keeps its own colours.

### Developer switch

`RESHIKI_DEV_ORBITAL_SURFACE` is gone from `main`, so no user path reaches
the analytic demo. Tests keep `surface::demo_on_this_thread`.

## Review outcomes

An adversarial review (gpt-6.2-astra, read-only) of the first draft raised
18 findings; this version resolves them as follows.

| #   | Finding                                                     | Resolution                                                                  |
| --- | ----------------------------------------------------------- | --------------------------------------------------------------------------- |
| 1   | Sphere proxy larger than the painted ball under perspective | Occluders built from painted footprints in S-space (§0, §2)                 |
| 2   | Eye inside a lobe; frame and Card too small                 | Reach includes the surface (§0)                                             |
| 3   | Translucent crystal copies cannot be clipped                | No surface on crystals in the prototype (§2)                                |
| 4   | `currentColor` resolves to black in usvg                    | Gone: merged layers carry explicit fills (§6)                               |
| 5   | Prism footprints differ from painted sticks, joints, wires  | Roof occluders from painted quads; Stick joints as balls; no Wireframe (§2) |
| 6   | Molecule's own order not exact                              | Guarantee narrowed to surface vs molecule and surface vs surface (§4)       |
| 7   | Labels and cell ink                                         | Label overlay rule (§4); no crystals (§2)                                   |
| 8   | Inscribed cap leaks an enclosed lobe                        | Circumscribed cap faces, 12 rings (§2)                                      |
| 9   | Seam strokes repaint clipped slivers                        | No per-piece strokes; nested layers have no seams (§5)                      |
| 10  | Zero padding fakes a boundary                               | Grow the box until it holds the surface, else flag `truncated` (mesh)       |
| 11  | Ties and singular levels                                    | Samples moved off the level; manifold by construction; link test (mesh)     |
| 12  | QEM can make sheets intersect                               | Intersection check against non-neighbours of both signs (mesh)              |
| 13  | BSP quadratic on a convex lobe                              | No BSP: pairwise visibility (§3)                                            |
| 14  | Fragment guard too late, lossy fallback                     | No fragment growth, no fallback (§3)                                        |
| 15  | Budgets did not measure the delivered file and frame        | Merged output; tile key hashes bits; end-to-end numbers in the PR           |
| 16  | Raw triangle count and simplifier exhaustion unbounded      | 3 M raw cap with coarser retry; over-target result reported (mesh)          |
| 17  | No signed-field contract for Phase 5                        | Phase 5 contract above                                                      |
| 18  | Cuts change shading                                         | Shading is a continuous vertex field; per-phase fade (§5)                   |

## Second review round

An adversarial review of the implementation (gpt-6.2-sol on the geometry
core, gpt-6.2-astra on the integration and snapshot) raised 31 findings.
Fixed: a simplifier collapse that could make the mesh pass through itself
(new faces are now also tested against each other and against faces sharing
only a vertex); duplicate vertices after clipping that hid an overlap; a
non-transitive depth tie rule (now exact sign, ties by index); rims dropped
with fully covered fills; contour thinning that could drop a thin hole or
make a contour cross itself (such contours stay unthinned); non-finite field
samples; an oracle that let a wrong contour exempt itself; a cap test that
never reached the cap planes; the best-fit plane on symmetric molecules (now
the existing Jacobi solver); a label promoted over a nearer ball; outline
margins; end-on half-bonds; box-capped (truncated) meshes are not drawn;
snapshot decoding bounds, outline validation and thinning; the switch now
also reaches render workers; the export estimate (`export_estimate`,
`EXPORT_WARNING_BYTES`).

Left as is, with reasons: coordinates far outside drawing scale (offsets of
2⁴⁰ or 10¹⁵ drawing units, grid origins at 10⁹ Å, isovalues of 10⁻⁹),
20 000 pieces sharing one bounding box (a closed surface cannot produce
it), `pistar` on odd rings (one bond necessarily joins equal signs; the
demo picks it by traversal order), and building the mesh on the calling
thread (about 0.4 to 1.4 s once per model in release on an idle machine;
Phase 5 builds orbital meshes off the UI thread).

A third pass (gpt-6.2-sol) on the fixes led to: the end-on rule now uses
the painter's own threshold; Stick's twice-as-wide contour counts in the
outline margin; molecules without a field are no longer cached (they could
evict real meshes); the simplifier tests only moved faces against the
others; very long contours skip the thinning guard; empty snapshot layers
are rejected. Known and accepted for the prototype:

- Thinning checks each contour on its own, so a hole within 0.05 drawing
  units of its outer contour could cross it.
- A label drawn over the surface is checked against nearer balls at its
  anchor only, not against a nearer bond crossing it or a ball covering
  part of a wide glyph.
- Outline runs shorter than 2 % of the model are dropped as mesh folds;
  a genuinely short visible piece of outline is dropped with them.
- The coarse detail can stay above 4 000 triangles when the simplifier has
  no legal collapse left (ethylene and benzene π: 5 130 and 7 412).
- The export dialog does not call `export_estimate` yet; wiring it is the
  UI lane's work.

## References

- W. E. Lorensen, H. E. Cline, Marching cubes, SIGGRAPH 1987; tetrahedral
  decomposition as in A. Doi, A. Koide, IEICE Trans. 1991.
- M. Garland, P. Heckbert, Surface simplification using quadric error
  metrics, SIGGRAPH 1997.
- H. Fuchs, Z. Kedem, B. Naylor, On visible surface generation by a priori
  tree structures, SIGGRAPH 1980.
- F. Crow, Shadow algorithms for computer graphics, SIGGRAPH 1977.
