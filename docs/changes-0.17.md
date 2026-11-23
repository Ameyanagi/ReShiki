# ReShiki 0.17.0

ReShiki 0.17.0 brings catalytic cycles, figure panels and connected lab equipment
to the drawing canvas. It adds editable tables and TLC plates, gives the
Properties panel controls for the current selection, and streamlines GitHub
browsing and figure export. Large drawings, image exports and recovery
discovery use less repeated work.

[Release downloads](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.17.0) ·
[Previous release](changes-0.16.md) ·
[Release validation](changes/release-0.17.0-validation.md)

## File compatibility

ReShiki saves a drawing at the lowest document version that holds all its
features. Ordinary drawings still save as version 30. The new drawing features
and explicit caption precision use version 40. Older ReShiki versions ask
you to update when you open a drawing that uses a newer document version.

| Document version | Examples of features that require it                                                                                                   | Earliest release that opens it |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------ |
| 30               | Ordinary drawings and imported structures without newer features                                                                       | 0.13.1                         |
| 31–34            | Custom arrow-label gaps, thermal ellipsoids, 3D model metadata and two-sided residues                                                  | 0.14.0                         |
| 35–37            | Arrow interior nodes, custom crossing clearance, enhanced stereo groups and additional radical states                                  | 0.15.0                         |
| 38–39            | Linked property captions, name lines and typed molecule names                                                                          | 0.16.0                         |
| 40               | Editable cycles, panels, callouts, tables, TLC plates, equipment, new graphic effects, compound numbers and per-caption decimal places | 0.17.0                         |

For sharing with older versions, keep a copy for that version. The format
reference lists the individual feature rules. [Document format](architecture.md#document-format-versions)

## Highlights

- **Catalytic cycles.** Create a cycle from selected reaction steps, arrange it on a circle, ellipse or rectangle, and add intermediates by dropping structures on the arrows. Apply **Clean up cycle** in one undoable step. [Reaction cycles](graphics.md)
- **Figures, tables and TLC.** Build editable figure panels, callouts and table cells, and set plate lanes, spots and Rf values from the inspector. [Editable graphics](graphics.md)
- **Laboratory equipment.** Assemble glassware and gas lines with fittings, mass flow controllers, valves and furnaces. Edit tube lengths, inspect cross-sections and build an equipment list from the drawing. [Gas-line controls](gas-line-valves.md)
- **Selection and export.** Properties shows the relevant controls for the selected objects. The Add tab brings templates and figures together. Export offers **Copy as**, **Save as**, and the previous export again. [Drawing integration](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Open from GitHub.** Search repositories or paste a URL above the repository list. Shared repositories show **Read only**, and new items are named in the destination. [GitHub picker](https://github.com/Ameyanagi/ReShiki/pull/1358)
- **Performance.** Reuse rendering work while editing, open recovery listings with less I/O, and use bounded staging for large image exports. [Rendering changes](https://github.com/Ameyanagi/ReShiki/pull/1387) · [Export changes](https://github.com/Ameyanagi/ReShiki/pull/1386)

## Laboratory equipment

- Added ¼-inch compression tubing and fittings, including straight and bent
  tubes, unions, elbows, tees, caps, and two styles of bulkhead fitting. Use
  Section to see where tube ends seat or a continuous tube passes through. [PR #1371](https://github.com/Ameyanagi/ReShiki/pull/1371)
- Test an equipment draft's joints before accepting it. Drag a disposable test
  piece and see the usual matching, adapter and mismatch feedback. The drawing
  keeps its original artwork until Accept.
- Canceling a test drag restores its previous fit, including adapters. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Pick a shape in an equipment draft and set its material and Back, Body,
  Inner or Front layer. The drawing previews each change, and draft Undo
  restores it before you accept the piece.
- Equipment shapes with crossing closed outlines, including bowties, keep
  their material and layer controls when their filled regions have area. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- After placing lab equipment, ReShiki returns to the pointer and opens
  Properties for the selected equipment. **Keep placing** is off by default.
- The Equipment browser reuses prepared thumbnails during placement. [PR #1357](https://github.com/Ameyanagi/ReShiki/pull/1357)
- Choose from three quartz work-tube diameters or run ordinary stainless-steel
  compression tubing directly through a tube furnace. Smaller quartz tubes
  have matching direct 1/8-inch and 1/4-inch compression end seals. [PR #1380](https://github.com/Ameyanagi/ReShiki/pull/1380)
- Added gas-line shutoff, needle, check and safety relief valves, supply and
  back-pressure regulators, pressure instruments, a gauge tee and thread adapters.
  Exterior and Section views share their connection points. The initial lineup
  uses ⅛-inch and ¼-inch compression valves and ¼-inch NPT pressure controls;
  [the gas-line guide](gas-line-valves.md) lists the exact nominal ports. [PR #1375](https://github.com/Ameyanagi/ReShiki/pull/1375)
- Added a mass flow controller to the equipment catalogue. Choose Exterior or
  Section to reveal the gas passage. Both views use the same ¼-inch compression
  inlet and outlet. [PR #1370](https://github.com/Ameyanagi/ReShiki/pull/1370)
- Added a horizontal tube furnace with a separate 32 mm quartz work tube,
  end seals and ¼-inch gas-port adapters. Connect the work tube through the
  furnace bore, then attach its end seals and gas lines. Exterior and Section
  keep the same pieces and connections. Axial travel keeps the mounted end
  seals clear of the furnace casing. [PR #1374](https://github.com/Ameyanagi/ReShiki/pull/1374)
- Set exact cut lengths for straight compression tubes and leg lengths for
  90° bends. The Equipment Parts List shows each tube's material, diameters,
  cut length, end connections, and the fittings it passes through. [PR #1376](https://github.com/Ameyanagi/ReShiki/pull/1376)
- Added an upright tube furnace with an open-centre frame and upright
  controls. New 32 mm work-tube end seals connect directly to ¼-inch or
  ⅛-inch compression tubing, with Exterior and Section views.
- Added a separate furnace-only body without a stand or temperature controller.
  Rotate the same connected piece between upright and horizontal diagrams. [PR #1378](https://github.com/Ameyanagi/ReShiki/pull/1378)
- **Define as equipment piece…** turns selected shapes into a fixed equipment piece. Mark a known length, preview the result, then Accept it in this drawing. Discard keeps the source shapes, and one drawing Undo reverses acceptance. Liquid and moving-part authoring remain unavailable. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Add joint…** defines a fixed ground-glass socket or cone from two rim edges in an equipment draft. Choose a standard size, review the generated joint and outward arrow, then add it to the draft. The first joint sets the piece's scale. Draft Undo stays separate from the drawing, and Accept remains one drawing edit. Standard joints require a compatible filled glass neck. The Equipment panel can save accepted pieces in **My pieces** for use in other drawings. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Equipment drafts include an English name, category, keywords,
  variant fields, author and licence. Metadata uses draft Undo and Redo;
  Accept keeps it with the piece in one drawing edit. Existing localized
  names are preserved. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Lab equipment pieces (preview)**: the `.reshiki-piece` format describes a
  piece of lab equipment as data only: outlines in millimetres, ground-glass
  and plug joints, holds, parameters, movable parts and liquid regions.
  `reshiki --cli pieces check FILE…` checks piece files and names the field to
  fix. Use **Insert equipment…** to place pieces from the catalogue or a
  library. [Command line](agent-cli.md#pieces) [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Lab stands, clamps and joint clips (preview)**: a three-finger clamp
  holds a flask's neck with its back fingers behind the glass and its front
  finger in front, on the canvas and in every export. Clamps mount on a lab
  stand's rod and slide along it while the stand stays put; dragging the
  stand moves everything. **Add clips to all joints** puts a joint clip on
  every seated joint of an assembly. See
  [Editable graphics](graphics.md#hold-equipment-stands-clamps-and-clips). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Hoses (preview)**. Draw hoses between hose barbs or leave an end free.
  Move the ends and bend the middle with handles. See
  [Editable graphics](graphics.md#connect-hoses). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Equipment libraries (preview)**: keep your own equipment pieces in **My
  pieces**, import and export `.reshiki-piece` and `.reshiki-pieces` files,
  and link a lab folder on a shared drive or in git to use its pieces. When a
  library has a newer revision of a piece in your drawing, **Properties →
  Equipment** offers **Update**, after listing what it would release or
  reset. `reshiki --cli pieces render` makes a review image of a piece file.
  See [Editable graphics](graphics.md#keep-and-share-equipment-pieces). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Cow adapter, thermometer and balloon (preview)**. Turn the cow adapter
  between receivers, slide a thermometer through its adapter, and inflate
  or deflate a balloon on a needle. See
  [Editable graphics](graphics.md#move-parts-and-set-liquid-levels). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Movable equipment parts and liquid levels (preview)**. Adjust clamp
  tilt, switch valves and stopcocks, and set liquid levels in round-bottom
  flasks, Schlenk ware and J. Young ware. See
  [Editable graphics](graphics.md#move-parts-and-set-liquid-levels). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Lab equipment (preview)**: click **Insert equipment…** in the tools palette to
  place round-bottom flasks, Liebig condensers, glass stoppers, rubber septa
  and joint adapters. Search names and keywords in English or Japanese, pick
  a size, and click the drawing. A piece moves, turns, scales and mirrors as
  one object and never stretches; copy, paste, Undo, PNG, SVG and PDF export
  and the dark canvas work as for other shapes, and ChemDraw gets each piece
  as a group of curves. Drawings with equipment are saved as document
  version 40 and carry every piece they use. Compatible pieces can snap at
  their joints. [Editable graphics](graphics.md#place-lab-equipment) [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Lab equipment joints (preview)**: pieces snap together by their joints.
  Drag a condenser onto a flask's neck, or place one from the Equipment
  panel: a green ring and a label show the joint it seats on, and the
  preview shows it seated. Joints of different sizes get the adapter that
  joins them; joints that cannot seat say why. Seated pieces move, turn and
  scale as one assembly; Option-drag (Alt-drag) pulls a piece out, and
  **Properties → Equipment** lists each joint with **Detach**. With a piece
  selected, J connects it from the keyboard. See
  [Editable graphics](graphics.md#connect-equipment-by-its-joints). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Straight stainless-steel tubing has length grips at both cut ends. Drag either end to extend or shorten it while the opposite end stays fixed; connected fittings follow, and furnace passages stay seated while the tube covers them. [PR #1384](https://github.com/Ameyanagi/ReShiki/pull/1384)
- Add 10, 25 and 50 mL burettes with glass or PTFE stopcocks, numbered graduations, adjustable liquid levels and selectable open and closed stopcock positions. The barrels can be held by the existing three-finger clamp. [PR #1395](https://github.com/Ameyanagi/ReShiki/pull/1395)
- The equipment catalogue now includes an upright J. Young NMR evacuation manifold. Its bottom socket seats on the J. Young NMR tube, and its side barb accepts the stock 9 mm rubber hose.
- The PTFE spindle presses against the tapered glass seat when the manifold valve is closed and lifts clear when open. [PR #1397](https://github.com/Ameyanagi/ReShiki/pull/1397)
- The equipment catalogue now includes Komagome pipettes in 1, 2, 3, 5 and 10 mL, short and long Pasteur pipettes, single-mark volumetric pipettes in 1, 2, 5, 10 and 25 mL, and intermediate-mark and tip-graduated measuring pipettes in 1, 2, 5 and 10 mL. Droppers have fitted and removed rubber-bulb states. The pipettes have editable drawn liquid levels and colors. Scale marks are schematic.
- An electric pipette controller is available for glass volumetric and graduated pipettes, with two finger buttons for aspiration and dispensing and an adapter-section view of its silicone mount.
- A blue rubber safety pipette filler is available with standard and small-bore-adapter appearances for manual filling of glass pipettes. [PR #1394](https://github.com/Ameyanagi/ReShiki/pull/1394)
- Add spouted graduated cylinders in 10, 25, 50, 100 and 250 mL sizes, and single-mark volumetric flasks in 25, 50, 100, 250, 500 and 1000 mL sizes. Both families have adjustable liquid levels and colors. Flask stoppers switch between seated and removed positions. [PR #1393](https://github.com/Ameyanagi/ReShiki/pull/1393)
- The equipment catalogue includes squat and tall ground-lid weighing bottles,
  screw-cap glass sample vials, and Gay-Lussac pycnometers with capillary
  stoppers. Toggle each closure between closed and removed views, and adjust
  the liquid level and color for sample-transfer and weighing diagrams. [PR #1392](https://github.com/Ameyanagi/ReShiki/pull/1392)
- The built-in equipment catalogue covers all 69 entries in the lab glassware proposal, with size and joint variants. It adds Schlenk and J. Young glassware, distillation and Kugelrohr pieces, filtration and separation equipment, NMR tubes, and bench accessories.
- Separatory funnels can seat in the support ring. Stir bars can seat in compatible round-bottom flasks and show motion arrows. [PR #1361](https://github.com/Ameyanagi/ReShiki/pull/1361)
- Schlenk tubes now include left- and right-facing straight sidearms in 15/25 and 29/42 with the same three-way valve as the S-type. Both styles have a continuous shoulder from socket to vessel. Top-view three-way stopcocks are available with three hose barbs or with one 15/25 or 29/42 ground-glass cone.
- A W-shaped 15/25 rubber septum can cap the 7 mm hose barbs on Schlenk tubes, condensers and other equipment. [PR #1362](https://github.com/Ameyanagi/ReShiki/pull/1362)
- The upper socket joins directly to the chromatography column’s reservoir. [PR #1366](https://github.com/Ameyanagi/ReShiki/pull/1366)
- The thermometer socket joins directly to the glass body on all four distillation-head variants. [PR #1363](https://github.com/Ameyanagi/ReShiki/pull/1363)
- The upper socket joins directly to the vessel on both pressure-equalizing dropping funnels. [PR #1364](https://github.com/Ameyanagi/ReShiki/pull/1364)
- Equipment drafts keep in-flight Assistant replies and defer their drawing changes until the current edit ends.
- Defining equipment keeps 0.6 pt outlines and 0.4 pt details, refusing unsupported visible stroke colors and widths with adjustment instructions. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Ground-glass hatch marks stay inside the frosted area, and the strokes of
  a seated cone end at the socket mouth.
- Revised clamp fingers, support rings, and joint clips remove overlapping
  seams. The clamp seats within the available neck, and the fixed support
  ring seats at its matching diameter.
- The Schlenk flask has a continuous glass side tube, and both rubber
  septa show one seam between the cap and skirt. [PR #1359](https://github.com/Ameyanagi/ReShiki/pull/1359)
- The ground-glass socket blends into the tube on the 15/25 packed fractionating column. [PR #1368](https://github.com/Ameyanagi/ReShiki/pull/1368)
- Removed the stray line beside the liquid on the J. Young NMR tube. Sample height still adjusts from its handle. [PR #1369](https://github.com/Ameyanagi/ReShiki/pull/1369)
- The upper socket joins directly to the vessel on both separatory funnels. [PR #1365](https://github.com/Ameyanagi/ReShiki/pull/1365)
- The ground-glass socket blends into the tube on the 15/25 Vigreux column. [PR #1367](https://github.com/Ameyanagi/ReShiki/pull/1367)
- Show the exposed bored PTFE stem, sealing rings, stepped yellow cap and rounded glass valve body on the J. Young NMR tube. The plug contacts the tapered glass seat when closed and lifts clear when open. [PR #1397](https://github.com/Ameyanagi/ReShiki/pull/1397)
- Curved liquid surfaces meet their colored fill in equipment illustrations,
  including tilted vessels. A surface stays straight where its curve would
  cross a vessel wall or a hole in the liquid region. [PR #1396](https://github.com/Ameyanagi/ReShiki/pull/1396)

## Reaction cycles, TLC and tables

- **Make catalytic cycle** applies immediately as one Undo step. Select a
  cycle and choose **Clean up ▾ → Clean up cycle** to space its intermediates
  evenly and align its reagents and products immediately, also as one Undo
  step. **Tidy cycle** in Properties and **Tidy catalytic cycle** in the command
  palette run the same action. [PR #1360](https://github.com/Ameyanagi/ReShiki/pull/1360)
- Choose **Circle**, **Ellipse**, **Rounded rectangle** or **Rectangle** in a
  catalytic cycle's **Layout** section. Set width and height for noncircular
  cycles and corner radius for rounded rectangles. Each change has its own
  Undo step. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Drop a structure onto a painted ellipse, rounded rectangle or rectangle cycle
  arrow to insert it between its neighbors, or beside a step to add a reagent
  or product. Bowed and reversible arrows use their displayed paths. The move,
  reaction-role change and insertion share one Undo step; pasted structures
  stay loose until dragged onto a cycle.
- Splitting a bowed or paired shaped-cycle arrow keeps its displayed curve
  between structures. Layout edits that move other intermediates show the
  complete result for review before Apply. Tilting a fitted cycle keeps its
  saved arrow paths through a half turn and declines projections that cannot
  keep every child arrow drawable. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Draw native tables with editable cell text and structures, assign or release existing content, and change cell borders in Properties. Pointer edits and erasing respect the owning cell's visible content.

- Keep selection outlines and handles around wrapped and clipped cell text. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Catalytic cycles** store their outline choice in `.rsk` files. Drawings
  without an outline choice keep their circular layout. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Tidy cycle** spaces intermediates evenly along elliptical, rounded
  rectangular and rectangular cycle outlines. Reagents and products stay on
  their original side of the outline, and linked molecule captions move with
  their structures. Each tidy applies immediately as one Undo step.
- Side-arrow joins stay in order along each visible line. Inside participants
  keep their ink clear of nearby corners. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- A catalytic cycle exported as CDXML or CDX now carries each step as a
  ChemDraw reaction step, and opens in ReShiki again as a cycle, with its
  steps, names and settings, while the file is unchanged. A file edited
  elsewhere opens as it was saved there, with a message on how to make the
  cycle again. See
  [Open a cycle in ChemDraw and back](graphics.md#open-a-cycle-in-chemdraw-and-back). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Make catalytic cycle** turns two or more selected structures or texts
  into the intermediates of a catalytic cycle. Its arc arrows, side arrows and
  step names are worked out from where the parts sit, so moving, resizing or
  editing an intermediate refits every arrow. Drag a structure or text onto
  the circle to add an intermediate, or beside a step's arc to add a reagent
  or product; the drop place is outlined and named before you let go.
  **Properties → Catalytic cycle** sets the direction, radius, center label
  and, for each step, its name, a reversible pair of arcs, a hidden arrow, a
  bow and extra space. Reaction detection never takes a cycle's parts.
  ChemDraw gets plain arrows and text. Drawings with cycles are saved as
  document version 40. See
  [Editable graphics](graphics.md#draw-a-catalytic-cycle). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Each step of a catalytic cycle is now a reaction: the step's intermediate
  and reagents go to the next intermediate and its products, and texts are
  its conditions. Pick a step in **Properties → Catalytic cycle**, or
  right-click its arc, to **Copy step as reaction SMILES**, **Auto-map step**
  and review the maps in the card, **Check step balance**, or **Color cycle
  by origin**, which carries colors from one step into the next along the
  atom maps. A structure that joins a cycle leaves its ordinary reaction, and
  automatic reaction copy and the arrow hints no longer take a cycle's parts.
  See
  [Use a cycle's steps as reactions](graphics.md#use-a-cycles-steps-as-reactions). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Draw a TLC plate by searching **TLC plate tool** in the command palette.
  Properties edits its lanes, spots, Rf values, dimensions, colors and reference
  lines. Each edit has one Undo; standard Rf labels follow their values while
  authored labels and untouched native dimensions are preserved.
- With a TLC plate and another shape selected together, Properties shows the
  shape's width and colors, and Apply preserves the plate. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- ChemDraw TLC plates open as one editable drawing object with lanes, filled or
  hollow spots, origin and solvent-front lines, physical spot sizes and styled
  Rf labels. Native ReShiki files and editable CDXML/CDX keep the plate data;
  canvas, SVG and PDF use its shared geometry. Unsupported custom spot artwork,
  tails and appearance settings produce an import notice.
- Native TLC exchange keeps ChemDraw's five-point lane margins and authored Rf
  captions, and writes binary CDX show flags in the form ChemDraw can reopen.
  Out-of-range Rf text sizes use the normal caption fallback while retaining
  authored text, styling and placement. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Inserting an intermediate into a bowed catalytic cycle step brings side
  structures stranded beyond the shortened step back beside it. Their
  reaction roles and the step's settings stay the same, and one Undo restores
  the insertion and its placement. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Noncircular catalytic cycles keep their proportions, rotation and direction
  when resized. Linked name and property captions move with their structures. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Keep ChemDraw table contents in their cells through CDXML/CDX exchange and draw flat table borders at grid intersections.

- Keep new atoms and linked molecule captions in their table cell during ordinary drawing edits.

- Create attached mechanism arrows between targets in the same table cell, and ignore cell-clipped targets when choosing their ends.

- Keep a selected table centered when rotating or flipping it. Preserve readable captions and their alignment inside the cells, and keep clipped source text editable.

- Keep new callout captions, leaders, centroids and attachment points in the table cell of their targets.

- Keep a cycle, its reaction legend, and equipment attachments in their shared table cell when creating or editing them.

- Keep cell labels readable over filled shapes and make borderless tables selectable. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)

## Drawing and figure styling

- Expanded compression tubing and joints to 1/16, 1/8, 1/4 and 3/8 inch,
  plus 6 mm. Straight and bent tubes have matching unions, elbows, tees,
  caps and bulkhead fittings, with reducing unions between selected sizes.
  Section shows tube stops and continuous bored-through passages. [PR #1373](https://github.com/Ameyanagi/ReShiki/pull/1373)
- **Compound numbering.** Right-click a molecule and choose **Number compound**
  to put a bold linked number below it. **Renumber** in the context row puts
  automatic numbers in reading order in one step. Edit a label to keep an
  override such as **2a**. Numbers follow molecule moves, save with their links
  in `.rsk` files, and remain separate from chemical names and property captions.
  Whole-molecule copies get fresh automatic numbers; edited copies keep their
  text. Resize previews keep numbers below wrapping name captions, matching
  their final position when the drag ends. Other formats keep the visible labels
  as plain text. [PR #1336](https://github.com/Ameyanagi/ReShiki/pull/1336)
- **Insert a step by dropping on an arc**: drag a structure or text onto a
  catalytic cycle's arc and the arc lights up with a tag such as **Insert
  between 1 and 2**; let go and it becomes a new intermediate there. Beside
  an arc, the step's arc lights up with **Reagent of step 1** or **Product of
  step 1**. The drag tag now sits below the dragged piece instead of over it.
  See [Editable graphics](graphics.md#draw-a-catalytic-cycle). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- The **Catalytic cycle** card shows one row per step, with chemical names,
  formulas or text labels. Open a step to see its reagents, products and
  actions. **Layout** and **Purpose and properties** open when needed.
  **Reaction roles** also shows chemical names or formulas. See
  [Editable graphics](graphics.md#draw-a-catalytic-cycle). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Select a cycle's arrows or its structures**: with a catalytic cycle
  selected, a click on its circle or an arc selects only the circle and its
  arrows. Right-click → **Select structures in cycle** selects every
  intermediate, reagent and product. See
  [Editable graphics](graphics.md#draw-a-catalytic-cycle). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Take one piece out of a big object**: a click on a group, panel,
  catalytic cycle or equipment assembly selects all of it, and a second click
  selects the piece under the pointer, as in Keynote and PowerPoint. Drag that
  piece to move it inside the object; let go outside, and it leaves the cycle,
  panel or group, or comes off its assembly. A tag such as **Release to take
  it out of the cycle** shows this before you let go, and the cycle's arcs
  already close up without it. Option-drag (Alt-drag) still does it in one
  go. See [Editable graphics](graphics.md#draw-a-catalytic-cycle). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Tidy a catalytic cycle** with **Clean up ▾ → Clean up cycle**. It spaces
  intermediates evenly and lines up reagents and products beside their arcs
  immediately, as one Undo step.
- Side arrows meet the visible line of a reversible step, with room between
  nearby joins. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Reference and illustration objects.** Give a molecule or arrow a
  **Purpose** in **Properties** or the right-click menu: **Chemistry**,
  **Reference** or **Illustration**. A reference structure beside the product
  keeps its formula and captions but never joins the reaction. Illustration
  molecules and arrows are pictures: chemical copy, **Check drawing**, the
  whole-drawing formula and condition text skip them. A dashed outline marks
  them while you edit, never in exports.
- **Preview reaction.** Right-click a reaction arrow to outline its reactants,
  products, reagents and conditions, read from assigned roles or from the
  arrow's row. Esc hides it.
- `.rsk` files with a Reference or Illustration object use document
  version 40. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Color by origin.** Right-click a reaction arrow and choose **Color by
  origin**: each reactant gets its own palette color, carried into the
  products along the atom maps. New bonds are highlighted and new rings filled,
  with an optional legend whose names you can edit. Unmapped reactions run
  **Auto-map** first. **Reaction roles** keeps the settings, so **Update
  colors** repaints after an edit. Several steps keep one color per fragment. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Wrap in panel** puts a selection on a tinted, rounded panel with an
  optional title bar or a title set into the border, and dividers that split
  it into titled columns or rows. **Properties** shows the panel's title,
  fill, outline and shadow first. **Lasting layout…** keeps molecules, groups
  and whole reactions in a row, column or grid whose gutters hold when an item
  changes size. Page setup offers ACS, RSC and Elsevier TOC graphic sizes, and
  **Templates → Figure layouts** lists four starting layouts by name;
  **Clear contents** deletes a panel's placeholder rings in one step. Drawings with
  panels or layouts are saved as document version 40. See
  [Editable graphics](graphics.md#put-a-figure-on-a-panel). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Arrow bodies**: **Properties → Body** and the arrow tool's context row give
  an arrow a **Block**, **Hollow block**, **Chevron** or **Gradient** body. The look
  never changes what the arrow means: a block reaction arrow keeps its roles
  and conditions. The arrow palette offers each body as a preset. See
  [Reaction and mechanism arrows](arrows.md).
- **Condition icons**: the **Icons** tool has a **Reaction conditions**
  category with light, electrode, heat, microwave, ultrasound, cold and
  enzyme icons. Click one onto an arrow and group them, and the icon moves
  with the arrow. It is an ordinary shape, so the reaction and its condition
  text are unchanged.
- An image exported on a black or custom background paints shapes filled with
  the paper color in that background color, so a condition icon's disc
  matches the page.
- **Arc arrows**: **Make arc** in **Properties** and the **Arc** preset draw
  circular arc arrows. Drag the diamond to change the sweep, or type it in
  **Sweep (°)**. ChemDraw arc arrows open as arc arrows and export as ChemDraw
  arc arrows. A **Gradient** body is shaded like ChemDraw's Shaded fill.
- **Callouts**: right-click an atom, a bond, a molecule or a text and choose
  **Add callout**. The callout's leader stays on what it points at when you
  move it.
- Drawings with these looks save as `.rsk` version 40. CDXML/CDX keep block,
  hollow and gradient arrows and arcs as ChemDraw arrows; chevrons and
  callouts travel as grouped shapes, with a notice. Condition icons travel as
  ordinary shapes. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Dark canvas and colored cards.** Structures, arrows and text on a filled
  shape take the ink that reads on the fill: black on a light grey card even
  on the dark canvas, and white on a dark card on the light one. Panel titles
  in the border sit on the panel's own fill instead of a black or white box.
  The **Previous work vs this work** layout uses theme colors, so it reads on
  both canvases. A new gradient starts from **Paper**, the canvas color, so a
  default gradient keeps its range on the dark canvas. ChemDraw files (CDXML,
  CDX) are always written with the light canvas's look and no page rectangle.
  Imported ChemDraw and MOL files open on the canvas you chose last, as new
  drawings do. See [Drawing styles](drawing-styles.md#palette-colors). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Icons**: a new **Icons** tool beside **Equipment** opens a searchable
  panel of line icons: the reaction condition icons (light, electrode, heat,
  microwave, ultrasound, cold, enzyme) first, then all 5,203 outline icons of
  Tabler Icons 3.50.0 (MIT) by category. Search in English or Japanese,
  click an icon and click the drawing to place it one bond length across.
  An icon is an ordinary shape on a paper-colored disc: recolor, scale and
  group it with an arrow, and it exports to PNG, SVG, PDF and ChemDraw (as
  curves). The condition icons moved here from the **Chemical symbols**
  palette. See [Editable graphics](graphics.md#place-an-icon). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Symbols**: **Ω** in the Style bar and **Insert symbol…** in the command
  palette open a symbol picker with arrows (⇌ ⟶ ↑ ↓), math (± ° ‡ ≈), Greek
  letters, charges (⊕ ⊖ • ⁺ ⁻), conditions (hν Δ e⁻) and check marks, plus
  your recent symbols. While you edit a caption, shortcuts such as `->`,
  `<=>`, `+-` and `\alpha` turn into their symbols as you type; Undo brings
  back what you typed. The browser build draws every picker symbol too. See
  [Style toolbar](typography.md). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Text in shapes.** Double-click a rectangle, rounded rectangle, ellipse or
  circle and type: the text is centred in the shape and wraps to its width. It
  moves with the shape, rewraps when you resize it, and dragging the text
  drags the shape. Symbols, typed shortcuts and sub- and superscripts work as
  in any caption. See [Editable graphics](graphics.md#type-text-inside-a-shape). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Text inside a shape turns with the shape and wraps along it, on the canvas,
  in exports and in ChemDraw files. In a mirrored shape it stays readable.
  Before, it stood upright in a turned shape. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Captions can turn. Select only captions and drag the rotate handle, or type
  an angle in the **°** field, to turn them in place. A turned caption keeps
  turning with the drawing; upright captions stay upright. Turned captions
  export turned to PNG, SVG, PDF and ChemDraw. Turned captions in ChemDraw
  files now open turned instead of upright. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Callouts stay on the page**: a new callout whose text would run past the
  page's right margin goes up and to the left of what it points at instead,
  and text still outside the margins is moved onto the page.
- **Paste keeps the zoom**: the view scrolls only when part of the pasted
  drawing would be off screen, instead of zooming out to show the whole drawing. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Page setup**: choosing a paper size or an orientation applies it at once,
  as one Undo step, so the sheet appears without scrolling to **Apply page
  setup**. Only the size changes: typed margins and grid stay typed, and
  margins too wide for the new size go back to their defaults. A
  table-of-contents size makes one sheet with no margins. Print uses the applied pages, as
  page PDFs do. The size list is now the first control, labeled **Size**, opening
  Page setup closes the View strip, and the strip's **Done** is now **Close
  view options**. See
  [Publication pages](publication-pages.md). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **⌘K opens the command palette** (**Ctrl+K** on Windows and Linux); press it
  again to focus search while keeping the query and filter. **:** still opens it.
- The palette has the new drawing commands: cycles, equipment, callouts,
  panels, layouts, arrow bodies, highlight styles and fill effects. A label
  that needs its section shows it, as in **Panel: Outline**.
- One name per feature: **Hollow block** for the outlined arrow body,
  **Opacity** for see-through ring fills, **Rounded corners** for panels.
  Palette search also accepts British spellings such as "colour". [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Reopen where you left off**: a `.rsk` drawing reopens at the zoom and
  scroll position it was saved at, instead of fitting the drawing or its first
  page. Older ReShiki versions open these files as before. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Zoom keys and stacking order**: ⌘= (or ⌘+) zooms in and ⌘- zooms out
  (Ctrl on Windows and Linux). A shape's right-click **Arrange & transform**
  menu now has **Bring to front** and **Send to back**, as the selection
  toolbar's **Order** menu does. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Shapes take a **Linear** or **Radial** **Gradient** fill and an
  **Opacity (%)**, which makes a fill, or the lines of a shape without one,
  see-through. Ring interiors have a **Fill effect**: **Gradient** or
  **Opacity**. A gradient or see-through rectangle behind the drawing makes
  a page or panel background, and the **Gradient** arrow body now uses the
  same gradient fill. ChemDraw's faded fills (FadePercent) open as
  see-through fills and export back to ChemDraw; gradients export as the
  nearest flat color, with a notice. These drawings are document version 40.
  See [Editable graphics](graphics.md). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Highlights can glow. **Color → Apply to → Highlights → Highlight style**
  paints every atom and bond highlight in the drawing **Flat**, as a **Soft
  glow** or as a **Disc** behind each atom. Ring interiors add **Soft glow** and
  **Disc** to their **Fill effect**. Glows and discs stay with their atoms,
  draw on the canvas and in PNG, SVG and PDF, and export to ChemDraw as flat
  highlights and ring fills, with a notice. These drawings are document
  version 40. See [Editable graphics](graphics.md). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Rectangles, rounded rectangles, ellipses and closed paths can be **Shaded**,
  from white at the upper left to the fill color, and carry a grey drop
  **Shadow**, as in ChemDraw. Lines, curves, paths and arrows have a **Wavy**
  stroke, graphic lines take arrowheads, and rounded rectangles keep a set
  **Corner (pt)**. ChemDraw files with shaded,
  shadowed or wavy shapes now open with those styles, and CDXML and CDX export
  writes them back as ChemDraw's own shapes. These drawings are document
  version 40. See [Editable graphics](graphics.md). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Color by origin** legends now land on the reaction's page, clear of other
  objects, so **Export pages as PDF** keeps them whole. A legend wider than its
  page lists one entry per line.
- Retyping a whole legend keeps each swatch's color, and turning **Mark new
  bonds** or another entry on or off keeps the names you gave the others.
- **Copy as → Reaction SMILES** no longer needs the color legend in the
  selection.
- **Properties** no longer shows atom-map numbers in a molecule's Canonical
  SMILES, and its **Copy SMILES** copies it without them; reaction SMILES still
  carry the maps.
- A new ring's fill is see-through, so it no longer merges with the new bonds'
  highlight.
- **Figure layouts** placeholder rings and arrows are illustrations, so they
  never form a reaction. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Colour emoji such as 💡 🔥 🧪 now show on the drawing in their own colours,
  sharp on Retina screens and on the dark canvas. Before, the drawing left a
  blank space where they were, although the text editor and PNG and SVG
  export showed them. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Emoji made of several characters show as one emoji in text on the canvas and
  in every export: ⚡️ and ❄️ in colour, 👩‍🔬 as one picture, and 👍🏽 and flags
  such as 🇯🇵 without a gap after them. Before, ⚡️ came out black and white,
  👩‍🔬 split in two, and a gap followed a skin tone or a flag.
- Backspace removes a whole emoji or accented letter while you edit text.
  Before, it removed one part at a time and left half a flag behind. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Colour emoji such as 💡 show in PDF exports and in drawings copied as PDF,
  which Keynote, PowerPoint and Word paste on macOS. Before, they were
  invisible there. The emoji are drawn as pictures; the text beside them stays
  text you can select and search. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- CDXML and CDX drawings now show Greek letters and other symbols from
  ChemDraw's Symbol and Symbol MT fonts correctly, including α where the
  import used to show "a". [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **CDXML export**: when CDXML would leave something out of the drawing, a
  dialog lists it, one item per line, before you choose where to save. The
  status line no longer carries the long notices. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Shortcuts after a right-click menu**: with Keyboard drawing off, a key
  pressed after a context menu closes acts on what the pointer touches next,
  or on the selection. Before, with the pointer already on the sidebar, **1**
  could turn the right-clicked double bond into a single bond instead of
  choosing the Single bond tool. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- On the dark canvas, a **Shaded** shape keeps its light highlight instead of
  turning dark in the middle, so a shaded ball no longer looks hollow. See
  [Editable graphics](graphics.md). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- 3D ball-and-stick, stick and ellipsoid views draw each half of a bond at its
  own depth. A bond that points toward you now shows in front of its atom and
  starts where it leaves the ball, so a carbon's four bonds read as
  tetrahedral. In stick views a stick that points toward you is no longer cut
  by the sticks behind it at their shared atom, and a stick that passes in
  front of another shows its edge. Space-filling views are unchanged. [PR #1307](https://github.com/Ameyanagi/ReShiki/pull/1307)

## Desktop, GitHub and export

- About shows one update action at a time. Click the version to copy it
  for a bug report, or open Settings to change update preferences. [PR #1328](https://github.com/Ameyanagi/ReShiki/pull/1328)
- New drawings open directly to a blank canvas.
- **Open from GitHub** shows shared repositories as **Read only** and omits
  the ReShiki source repository from the repository list.
- The GitHub picker puts repository search and URL paste above the repository
  list. **New drawing** and **New folder** icons add an editable row or tile
  where the item will be created. Press Enter after naming it.
- The workspace header uses a search icon to open the command palette.
- The Icon Browser uses a short **Search icons…** prompt.
- The **TLC plate tool** is available through the command palette.
- Caption symbol shortcuts accept a backslash or yen prefix, such as
  `\alpha` and `¥alpha` for α. [PR #1358](https://github.com/Ameyanagi/ReShiki/pull/1358)
- Open the command palette from the search icon in the header, **Cmd/Ctrl+K** or
  **Cmd/Ctrl+Shift+P**. Import and Export remain in the side panel and search. [PR #1339](https://github.com/Ameyanagi/ReShiki/pull/1339)
- **Identity** in Properties puts the name, formula, molecular weight, SMILES,
  InChI and InChIKey together with one-click copying. **Name it** generates a
  name on request, **Add name below** places it, and **Descriptors** keeps the
  other calculated properties in a collapsed card. [PR #1330](https://github.com/Ameyanagi/ReShiki/pull/1330)
- Properties follows the selection, with object controls first and Delete last.
  Clear the selection for drawing settings, or choose a drawing tool for its
  next-object defaults. Molecular properties remains available for selected
  molecules and explicitly requested fragment analysis.
- Explicit analysis stays open while its property captions are pinned or
  edited. Close analysis or Escape returns focus to the drawing; a new target
  ends the request. Next-object defaults support keyboard and native
  accessibility actions independently of the context row, as does the selected
  atom's Edit atom label command.
- Projected molecules keep rear-opacity controls when only an atom or bond is
  selected, without requesting molecular analysis. [PR #1349](https://github.com/Ameyanagi/ReShiki/pull/1349)
- **Export again** repeats the last successful picture export for a drawing,
  using its filename, background, resolution and editable-drawing choices.
  Normal export starts in the last picture-export folder. [PR #1329](https://github.com/Ameyanagi/ReShiki/pull/1329)
- **Export by destination.** The Export tab starts with **Copy for Word &
  PowerPoint**, **Picture file**, **Structure**, and **Pages & printing**. Shared
  picture options appear once. Background starts at each destination's Settings
  default; choosing a swatch applies it to copies and picture files from this
  panel. Structure offers both **Copy** and **Export…**. A short **Check drawing**
  notice links to existing findings. When Export is open, the Copy shortcut
  uses its Office copy action. **Export again** keeps its saved recipe. [PR #1338](https://github.com/Ameyanagi/ReShiki/pull/1338)
- Search chemical names, SMILES, and templates together in **Add**. Choose a visual template or press Enter to insert a structure at the center of the current view. The template gallery and advanced import options remain available. [PR #1335](https://github.com/Ameyanagi/ReShiki/pull/1335)
- Choose journal dimensions and colors from one status bar button. The chooser keeps separate **Journal preset** and **Color theme** lists, with both style and theme editors in reach. [PR #1343](https://github.com/Ameyanagi/ReShiki/pull/1343)
- Commands and shortcuts share one palette. Help, **?** and **F1** open **Keys**. The search icon and **Cmd/Ctrl+K** open commands. Use **All · Insert · Edit · View · File · Keys** or **Tab** to filter, and see the selected description and before/after example below the list.
- Press **F6** or **Shift+F6** to move between search and the available **Run**, **About**, and **Examples** actions. [PR #1351](https://github.com/Ameyanagi/ReShiki/pull/1351)
- **View** opens a menu above the status bar without resizing the canvas.
  Grid, rulers, crosshair, smart guides, arrange controls and page setup are
  together. The Sun/Moon canvas toggle stays in the bottom status bar.
  Ruler units are in **Settings → Drawing**;
  interface appearance and name display preferences stay in Settings. [PR #1342](https://github.com/Ameyanagi/ReShiki/pull/1342)
- Decision prompts fit their contents, scroll when space is limited, and wrap actions in narrow windows. They open near the top on macOS and centered on Windows and Linux, with a close button and the existing Escape and Enter answers. [PR #1350](https://github.com/Ameyanagi/ReShiki/pull/1350)
- Chemical names and Assistant keep their existing layouts with the inspector tabs visible above them. Their existing Back and Close controls, or Esc outside a text field, return to the preceding page. A new selection returns to Properties.
- Unfinished drawing style and color theme drafts stay intact while visiting Chemical names or Assistant, including through command search. Returning to the editor restores the draft; leaving for another panel or discarding the editor still ends it. [PR #1353](https://github.com/Ameyanagi/ReShiki/pull/1353)
- ReShiki can quit or restart for an update on macOS while its startup
  GitHub sign-in read is waiting for a Keychain prompt. ([#1240](https://github.com/Ameyanagi/ReShiki/issues/1240)) [PR #1327](https://github.com/Ameyanagi/ReShiki/pull/1327)
- Enlarge and center the Search icon to match the other main toolbar controls. [PR #1381](https://github.com/Ameyanagi/ReShiki/pull/1381)
- Headless molecule imports now run in plain `wasm32-unknown-unknown` hosts
  without JavaScript clock imports. These hosts retain bounded chemistry work
  and memory allowances. Browser builds keep their JavaScript-backed clocks
  through the `reshiki-io/browser` feature; native timing is unchanged. ([#1372](https://github.com/Ameyanagi/ReShiki/issues/1372)) [PR #1377](https://github.com/Ameyanagi/ReShiki/pull/1377)
- Restoring the GitHub version an older drawing is based on now leaves nothing
  to send, even when ReShiki saves it in the current format ([GitHub](github.md)). ([#1294](https://github.com/Ameyanagi/ReShiki/issues/1294)) [PR #1305](https://github.com/Ameyanagi/ReShiki/pull/1305)
- The star card can appear as soon as your first GitHub sign-in finishes and
  the drawing window is ready. If GitHub denies a star from the card or About,
  ReShiki shows which permissions to check and lets you try again
  ([Privacy policy](privacy-policy.md#github-star-request)). [PR #1323](https://github.com/Ameyanagi/ReShiki/pull/1323)

## Chemistry and file exchange

- A ChemDraw file (CDX or CDXML) can open even when it contains unsupported
  styles or objects. The import message lists substitutions and omissions,
  including unsupported BioDraw shapes and gel plates. Supported TLC plates
  open as editable objects. [ChemDraw import limits](graphics.md#chemdraw-styles-reshiki-cannot-draw-yet)
- Arcs from older ChemDraw files, drawn bracket pairs, daggers, the lone pair
  bar and its hybrid and filled orbitals now import as drawn.
- CDX files written by ChemDraw: no-go arrows keep their cross or hash, lone
  pair symbols open, and clip art with four-byte colours, unknown properties
  and repeated object identifiers no longer refuse the file. ReShiki now writes
  no-go marks, lone pair symbols and arc angles in CDX with ChemDraw's values.
  A CDX file that an earlier ReShiki version saved with a hash no-go mark or a
  lone pair symbol shows a cross or a lone pair bar. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- The journal preset selector adds **ACS Document 1996**, with the values of
  ChemDraw's ACS Document 1996 stationery. **JACS / ACS** keeps the same
  values rounded as ACS publishes them
  ([Journal drawing presets](journal-drawing-presets.md)). [PR #1309](https://github.com/Ameyanagi/ReShiki/pull/1309)
- Hover over a heteroatom and press **Delete** or **Backspace** to change it to
  carbon. Press the key again to delete the atom. [PR #1291](https://github.com/Ameyanagi/ReShiki/pull/1291)
- **Formula and masses below a structure.** Right-click a molecule and choose
  **Show formula & masses below** to add its Formula, Mol. Wt. and Exact Mass
  lines in one step. The menu beside the **Weight (g/mol)** and **Exact mass (Da)** pins in
  **Molecular properties** sets each caption's decimal places, from 0 to 5;
  Exact Mass keeps 4 by default. A caption with its own decimal places saves
  as `.rsk` version 40, which ReShiki 0.16.0 asks you to update for. [PR #1285](https://github.com/Ameyanagi/ReShiki/pull/1285)
- **Clean up reaction** (Shift+Cmd/Ctrl+X, also in the Clean up ▾ menu) lines
  up reactants, products and `+` signs on the arrow, spaces them evenly, gives
  every arrow the same length and centres the conditions above and below it,
  in one Undo step ([Clean up](selection-cleanup.md#clean-up-reaction)).
- The Clean up ▾ menu adds **Also set bond length to the style**. It is off by
  default, so Clean up still keeps each structure's size, and ReShiki remembers
  the choice. [PR #1314](https://github.com/Ameyanagi/ReShiki/pull/1314)
- SVG artwork: **Import SVG as drawing…** inserts SVG or SVGZ files as grouped editable paths. It keeps solid colors and names any approximations or omitted content. **Open** still restores SVG files with an embedded ReShiki drawing. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Dipole arrows exported as binary ChemDraw files now reopen in ChemDraw.
  Their editable arrow geometry and dipole marks are preserved. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Open ChemDraw CDX drawings containing atoms marked with unknown stereochemistry, matching CDXML import. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- CDXML/CDX export keeps the holes and overlaps of opaque compound paths, including imported SVG artwork. Original Bézier curves and outlines remain editable, and holes reveal the artwork behind them. Faded and shaded compound fills retain their existing export behavior.
- Removing fill from an imported borderless path gives it a normal outline and applies the edit to the full selection. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- `reshiki --cli help convert` and the [CLI guide](agent-cli.md#nmr-labels)
  now list `--labels value`. The CLI guide, the
  [NMR guide](nmr-prediction.md#labels-from-scripts) and the
  [0.16 notes](changes-0.16.md#captions-names-and-nmr) say which formats the
  command line writes with NMR labels: for an `.rsk` drawing, use Python or MCP. ([#1295](https://github.com/Ameyanagi/ReShiki/issues/1295)) [PR #1303](https://github.com/Ameyanagi/ReShiki/pull/1303)
- **Auto-map** now maps a Diels–Alder reaction correctly: the product's double
  bond comes from the two inner diene carbons, so **Color by origin** marks the
  right bonds. Auto-map now prefers maps that move no hydrogens over maps that
  keep bond orders. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Dashed lines, shapes and arrows now draw as ChemDraw does: dashes and gaps
  2.7 pt long with square ends, whatever the line width. A bold dashed line
  was a few long round-ended strokes. See [Graphics](graphics.md).
- No-go crosses and hashes now reach five line widths either side of the
  shaft, as in ChemDraw; they were a fixed 2 pt and looked small on a ChemDraw
  arrow.
- ChemDraw files: an unbalanced equilibrium arrow keeps its short arrow, the
  forward or the reverse one, and a retrosynthesis arrow's head is as wide as
  ChemDraw draws it, so both shafts run into it. **Reverse length** now goes
  up to 5, which makes the forward arrow the short one, and CDXML and CDX
  saves keep the ratio. See [Arrows](arrows.md).
- ChemDraw files: a doubled curve opens as two grouped parallel curves, the
  Rac, Abs and Rel stereo flags keep their rounded box, and a label written
  hydrogens first, such as H2O, keeps its hydrogens on the left. See
  [ChemDraw styles ReShiki cannot draw yet](graphics.md#chemdraw-styles-reshiki-cannot-draw-yet). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- ChemDraw files with many pages, such as the Bio Instruments, Anatomy,
  Animals, Hexoses and Amino Acids template libraries, now open. Their pages
  are placed in rows, in page order, so **Fit** shows the whole library instead
  of a thin column. Before, a long column of pages ran past ChemDraw's
  coordinate range or lost precision, and the file was refused.
- An abbreviation label with more than one font or size, such as "D-Trp" with
  a small D, opens in the style of its first character with a message, as
  element labels already did. Before, the file was refused.
- Older CDX files that keep atom labels straight in a group, such as the
  Anatomy template, and CDX files whose atom number or stereo labels ChemDraw
  tagged with a bare name now open.
- When a file cannot open, the status bar names it, a CDX file's error names
  CDX instead of CDXML, and an empty tab opened for the file closes again. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- A contracted group left of its bond, such as PPh₃ on Pd(PPh₃)₂, reads Ph₃P
  again after a ChemDraw (CDXML or CDX) round trip, as ChemDraw shows it,
  instead of printing PPh₃ over the metal. Formula labels ReShiki's catalog
  doesn't know now open with both spellings. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- ChemDraw's plus, minus, circled signs and daggers open upright, where they
  came in turned by 45° so that "+" read as "×". A radical cation or anion
  has its dot and upright sign where ChemDraw draws them, a single electron
  is the small dot ChemDraw draws, and a lone pair puts its dots where
  ChemDraw does. A free radical ion keeps its sign upright when turned.
- ChemDraw's shaded orbitals open with their shading. Orbitals have a new
  **Shaded phase** in **Properties**, white at the upper left of each lobe to
  the line color at its edge, which ChemDraw's plain p, hybrid, dz² and dxy
  orbitals and its shaded s, oval and lobe take on import. Before, the phase
  was lost or turned flat grey. ChemDraw's dxy opens with its lobes along and
  across its axis, as ChemDraw draws it. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- Titles, legends and labels that ChemDraw draws on shaded or shadowed boxes,
  as in the Citric Acid Cycle and Cyclic Peptide samples, stay readable when
  the file opens. Before, the boxes covered them. A shape now opens in front of
  the chemistry only when ChemDraw stacks it above every structure, text and
  arrow.
- Lines and arcs in ChemDraw files open as graphic lines and curves, like
  a line drawn in ReShiki, so **Properties** shows **Graphic style** and you
  can add an arrowhead. Before, they opened as reaction arrows with the
  **Chemistry** purpose. Arrows with heads, and lines a ChemDraw reaction step
  lists, still take part in reactions.
- A line without heads is written to CDXML and CDX as ChemDraw's line, so it
  opens again as a line instead of a curve. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Auto-map** and **Color by origin** keep abbreviations such as PPh₃
  contracted instead of expanding them, in ordinary reactions and in
  catalytic-cycle steps.
- **Color by origin** hides the atom-map numbers, so a colored figure stays
  uncluttered; **Show maps** in **Reaction roles** shows them again. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- **Reaction roles**: a selected reaction arrow has **Reaction roles…** at the
  top of its properties. In the arrow list, an arrow inside a titled panel is
  named by that title (for example "Arrow 2 · This work").
- **Atom-map review**: when a proposal is ready, its heading, summary, preview
  and **Apply maps** are in view, with the pair list below; a catalytic-cycle
  step's review in Properties too. The preview frames the reaction under
  review, with all its reactants, reagents and conditions, at a readable size.
- **Preview reaction**: the arrow's outline covers the arrow and the conditions
  along it only, so it no longer spans a **+**, a molecule's caption or a
  scheme title. [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)
- A bond added from a ring atom, by clicking it with a bond tool or pressing
  **1** over it, points out of the ring instead of into it. This covers an atom
  joining two rings by a single bond and the shared atom of fused rings such as
  a 6–7 system. The ring tools follow the same rule: a ring connected to such
  an atom with **Option/Alt**-click, or one sharing it, is placed outside the
  rings. [PR #1289](https://github.com/Ameyanagi/ReShiki/pull/1289)
- Pointing at a silicon, boron, germanium, selenium or tellurium atom that still
  has implicit hydrogens, such as SiH₂ in a silacyclohexane, and pressing **1**
  or **0** adds a bond, as it already did for carbon and nitrogen. Ring,
  template and keyboard Connect attachment use the same valences. [PR #1292](https://github.com/Ameyanagi/ReShiki/pull/1292)
- SVG, PDF and EMF pictures, copied or exported, draw plain bonds as stroked
  lines in the drawing's line width. Changing the stroke width in Adobe
  Illustrator or Inkscape now changes every bond, not only the inner lines of
  ring double bonds. Corners and ends look as before; wedges and bold bonds
  stay filled ([Figure export](figure-export.md#bonds-in-vector-files)). [PR #1299](https://github.com/Ameyanagi/ReShiki/pull/1299)
- **Properties → Transform** no longer refuses a width or height with "This size
  cannot be reached while keeping text and line widths fixed". When the size
  depends only on text, arrowheads or line widths, the drawing stays as it is
  and the status bar says why and what the size remains; a size below what the
  text allows applies the closest size and says so ([Selection transforms](selection-transforms.md)). [PR #1290](https://github.com/Ameyanagi/ReShiki/pull/1290)
- Wedge and hash bonds between two labeled atoms, such as Ru–Cl in a metal
  complex, no longer shrink to a dot: when the label margin leaves too little
  room, the bond goes closer to the labels. ([#1287](https://github.com/Ameyanagi/ReShiki/issues/1287)) [PR #1302](https://github.com/Ameyanagi/ReShiki/pull/1302)
- A CDXML file ReShiki writes opens again with its panels and its Reference and
  Illustration purposes, so a reference compound no longer joins the reaction
  after the round trip. ChemDraw has neither, and CDXML and CDX export now say
  so. See [Object purpose](reactions.md#object-purpose).
- CDXML and CDX export write a panel's shadow, and the shadow of a rotated
  rectangle or a path, as a grey shape behind it. An unfilled shadowed rectangle
  or oval on a colored ChemDraw page, such as a CDXML file exported from the
  dark canvas by an earlier build, opens with the page color inside, as
  ChemDraw draws it, instead of a white face. See [Editable graphics](graphics.md). [PR #1356](https://github.com/Ameyanagi/ReShiki/pull/1356)

## Performance

- Browser clipboard preparation can run in a bounded local worker while
  preserving available formats, editable picture data and per-format notices.
  WebKit and unrecognized browser hosts keep clipboard preparation local to
  preserve their user-gesture access; separate exports still use the worker. [PR #1391](https://github.com/Ameyanagi/ReShiki/pull/1391)
- Browser figure exports prepare and render in workers so the editor and host
  page remain responsive. Exports retain added fonts and existing output
  formats, with the existing local exporter as a fallback when a worker cannot
  initialize. Timeouts and cancellation stop the worker without repeating a
  dispatched export on the page. [PR #1391](https://github.com/Ameyanagi/ReShiki/pull/1391)
- Reuse equipment, ring, template, bond, and drag previews when the drawing and pointer inputs stay the same.
- Retain the drawing under an equipment preview when the piece can be drawn independently. [PR #1390](https://github.com/Ameyanagi/ReShiki/pull/1390)
- Clipboard Copy reuses PDF and SVG preparation and skips unused editable
  picture payloads while preserving native Copy Image eligibility and metadata. [PR #1386](https://github.com/Ameyanagi/ReShiki/pull/1386)
- Release unused GPU image resources and return empty image atlases to their initial size. [PR #1389](https://github.com/Ameyanagi/ReShiki/pull/1389)
- Reduced repeated allocation and undo-memory accounting work while editing
  drawings, preserving chemistry-change detection and undo behavior.
- Equipment lookups load definitions as needed and reuse a catalogue interface
  index when finding compatible adapters, keeping the same matching choices.
- Depiction repair reuses bounded topology distances and detached fragments
  across stages while preserving coordinates and work-limit accounting.
- NMR prediction reuses molecular adjacency and site sphere distances during
  fallback without changing environment identifiers or deadline checks. [PR #1385](https://github.com/Ameyanagi/ReShiki/pull/1385)
- Python PNG rendering converts pixels in place. Batch rendering shares a
  128 MiB budget for active output bitmaps across simultaneous batches,
  preserving input order, warnings and per-item failures. [PR #1386](https://github.com/Ameyanagi/ReShiki/pull/1386)
- Reuse vector layers and raster tile plans in mixed drawings, and skip offscreen pictures and text before preparing their pixels or outlines when safe bounds are available. [PR #1387](https://github.com/Ameyanagi/ReShiki/pull/1387)
- Open the workspace without waiting for recovery draft discovery. Draft review keeps lightweight summaries, and selected drawings load in the background before they open as recovered tabs. [PR #1388](https://github.com/Ameyanagi/ReShiki/pull/1388)

## Known limits

- Some editable objects, including panels and catalytic cycles, become groups of drawing objects in ChemDraw exchange. Use an `.rsk` copy to retain their editing controls. [Exchange behavior](graphics.md#limits)
- macOS downloads are signed and notarized. Windows and Linux downloads are unsigned. [Code signing policy](code-signing-policy.md)

## Build and verification

See the [0.17.0 release validation record](changes/release-0.17.0-validation.md).
