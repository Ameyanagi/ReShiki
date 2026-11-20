# Editable graphics

Shapes, brackets, lines, curves, pen paths and arcs are editable drawing
objects. They can sit behind or in front of the chemistry and keep their exact
geometry in `.rsk` files.

## Use it

### Draw a shape

1. Choose rectangles, ellipses, brackets, graphic lines, Bézier curves or arcs
   in the main palette. The shape selector in the context row also offers
   rounded rectangles, parentheses and braces. Arcs offer 90°, 120°, 180°, 270°
   and 360° sweeps.
2. Drag to size the object. Shift makes a square or circle, or keeps a line or
   curve at 45° steps. Escape cancels.
3. Set stroke color, fill color, line width and solid, dashed, dotted or wavy
   strokes in **Properties**. As in ChemDraw, dashes and gaps are 2.7 pt long
   (ChemDraw's default hash spacing) with square ends whatever the line
   width. Number and hex fields apply when you press Enter.
   A rounded rectangle also has **Corner (pt)**; leave it empty to round 18% of
   the shorter side.

### Type text inside a shape

Double-click a rectangle, rounded rectangle, ellipse or circle and type. The
text is centred in the shape and wraps to its width. It moves with the shape,
rewraps when you resize it, and dragging the text drags the shape. The usual
text editor applies, so symbols, typed shortcuts and sub- and superscripts all
work. Applying empty text adds nothing; Delete on the shape removes its text
too. The text stays upright when the shape is rotated. ChemDraw files get the
text as a caption centred over the shape.

### Shade or shadow a shape

Select a rectangle, rounded rectangle, ellipse or closed path. In
**Properties**, below the fill color:

- **Shaded** fills the shape from white at the upper left to its fill color at
  the far edge, as ChemDraw's Shaded style does. The highlight stays light on
  the dark canvas too, so a shaded ball still looks like a ball. Rectangles shade toward their
  right and bottom edges; ellipses and paths shade around a highlight. A shape
  without a fill takes its stroke color.
- **Shadow** adds a grey drop shadow 4 pt below and to the right. A tilted
  rectangle or ellipse casts it along its own axes, as ChemDraw does. The
  inside of a shadowed outline is painted white, so the shadow shows only at
  the edge.

Each change is one Undo step and applies to every selected shape. With nothing
selected, the settings apply to the next shape you draw. Shading and shadows
are drawn on the canvas and in PNG, SVG and PDF. Clicks and snapping use the
shape itself, not its shadow.

### Gradients and opacity

Select a rectangle, rounded rectangle, ellipse or closed path. In
**Properties**, below **Shaded** and **Shadow**:

- **Gradient → Linear** blends the fill from a start color into the fill color
  across the shape. **From** sets the start color and
  **Angle (°)** its direction: 0 runs left to right, 90 top to bottom. The
  gradient turns with a rotated rectangle or ellipse.
- **Gradient → Radial** blends from the start color at the middle of the shape
  into the fill color at its edge.
- **From** starts as **Paper**, the canvas color: white on the light canvas,
  black on the dark one, so the gradient keeps its range on both. Type
  `Paper` or a color.
- **Opacity (%)** controls how much of the drawing shows through the fill.
  Its outline stays solid. A line, an arrow line or a shape without a fill
  fades its lines instead. 100 is opaque.

A gradient or reduced opacity replaces **Shaded**. A shape without a fill
takes the tint of its stroke color as the fill, or the stroke color itself for
Ink or a custom color. For a gradient page or panel background,
draw a rectangle over the area, give it a gradient and choose **Send to back**.

To change a ring interior, select the whole ring, open the color button's
**Ring interiors** and choose a **Fill effect**: **Flat**, **Gradient** (the
paper color at the middle of the ring into its color), **Opacity** (half
opacity), **Soft glow** (a halo of the color around the ring) or **Disc** (a round disc behind the ring,
a little wider than its atoms).

### Glowing highlights

Open the color button, choose **Apply to → Highlights** and pick a **Highlight
style**. It applies to every atom and bond highlight in the drawing:

- **Flat**: ChemDraw's band along bonds and pill around atoms.
- **Soft glow**: the same shape, grown and blurred into a halo.
- **Disc**: a round disc behind each highlighted atom; bonds keep their band.

The style is one Undo step. Glows are drawn on the canvas and in PNG, SVG and
PDF, and export bounds include their soft edge.

Each change is one Undo step. Gradients and fills with reduced opacity are drawn on the
canvas and in PNG, SVG and PDF, on the light and dark canvas.

### Wavy lines

Choose **Wavy** as the stroke pattern of a line, curve or path, or of an arrow
in its **Properties**. The wave is a row of half circles, about 3.6 pt long
for lines up to 1 pt wide, and longer for bolder lines. A wavy line runs
straight under each arrowhead and into it, and the head stays solid.

Select a graphic to move, copy, duplicate, rotate, reflect or resize it.
**Send to back** and **Bring to front** place it behind or above the chemistry.

### Draw a pen path

1. Choose **Pen path** and drag the first segment.
2. Click to add a straight segment, or drag to add a curved one. The drag sets
   the new node's direction.
3. Click the first node to close the path, or choose **Finish**. **New path**
   starts a separate object.

**New path** and **Finish** show while the **Pen path** tool is active. Each
segment is one Undo step; Escape cancels the segment you are drawing.

Select a graphic line, open pen path or Bézier curve and use **Properties → ARROWHEADS**
to set **End**, **Start**, shape and size. Number fields apply on Enter.
The context row's **Start head** and **End head** add a full head; **No start
head** and **No end head** remove it. Each change applies to all selected open
paths and curves in one Undo step. Heads follow the end directions, and the
line stops under each head. These paths remain free drawing objects and do
not attach to atoms.
On a multi-segment path, head sizing uses the whole path when an end segment
is too short.

Select pen paths, Bézier curves or curved arrows and choose **Smooth** or
**Symmetrize** from the canvas menu or command palette. Both commands apply to
the whole selection in one Undo step. They are also buttons in the
inspector's Pen path panel (as **Smooth path** and **Symmetrize**) and the
curved arrow panel.

**Smooth** aligns cubic handles at joints bent less than 20°, following the
straight segment at mixed joints and preserving intended corners. It removes
redundant nodes within a quarter of the line width of the original path.
It runs only when commanded. For curved arrows it evens the
angle and length of a lopsided bow when the end angles differ by less than 20°. On arrows with
nodes ([Add nodes to an arrow](arrows.md#add-nodes-to-an-arrow)) it applies
the path rules to the interior nodes; the ends stay in place.

**Symmetrize** finds the best mirror axis, including tilted axes, and averages
both sides. Hold Shift when clicking it to use a vertical or horizontal axis;
the palette also has **Symmetrize upright**. Shapes too far from symmetric are
left unchanged with a reason in the status bar. Curved arrows, including
arrows with nodes, use the perpendicular bisector of their endpoints; Shift has
no effect on them. Arrow
endpoints, attachments and arrowheads stay in place.

### Edit points

1. Select a curve or path and choose **Edit curve points**.
2. Drag a node to move it with its direction controls, or drag a square
   control to change a direction. **Corner** nodes (diamonds) have independent
   directions. **Smooth** nodes (circles) keep directions in line, with separate
   lengths. **Symmetric** nodes (circles with a dot) keep equal, opposite
   directions. These constraints apply when both adjacent segments are curved.
   Dragged pen nodes start Symmetric; clicked nodes start Corner.
3. In **Properties**, choose the node under **Selected node**. **Next segment**
   switches the following segment between **Straight** and **Curved**.
   **Insert after** and **Delete node** add and remove nodes. **Close path** or
   **Open path** closes or opens the path, and **Continue drawing** extends an
   open path.
4. Double-click a node to cycle Corner → Smooth → Symmetric → Corner, or use
   **Corner**, **Smooth** and **Symmetric** in the context row for the selected
   node (including the owner of a selected direction control). Switching to
   Smooth or Symmetric aligns the directions. Option/Alt-drag a direction
   control to make its node Corner and move that direction independently.
   Each type change or drag is one Undo step.
5. Choose **Done** in the context row to finish.

### Put a figure on a panel

1. Select what the panel should hold, such as a scheme and its captions.
2. Choose **Wrap in panel** from the canvas menu, the command palette or
   **Properties → Grouping & frames**. A tinted, rounded panel appears behind
   the selection with 10 pt of padding, grouped with it so they move together.
   Click an object inside a second time to edit it alone. Drag it out of the
   frame and it leaves the panel.
3. **Properties** opens on the **Panel** card, above what the panel holds.
   Type a title and press Enter, pick a fill swatch, and turn **Outline** or
   **Shadow** on or off. **Title bar**
   puts it on a band in the outline color; **In border** sets it into the top
   edge. A title band grows the panel upward, so it never covers the contents.
4. **Split columns** and **Split rows** add a divider in the widest gap
   between the contents. Each column between vertical dividers takes its own
   title. Type a position in percent to move a divider; **×** removes it.
5. Choose **Rounded corners** or **Square corners**. Set the radius of rounded
   corners in **Graphic style → Corner (pt)**. Outline color, line width,
   dashes, gradients and a custom fill color are under **Graphic style**,
   which changes every selected shape: to change the panel alone,
   click its margin a second time first. **Fit to contents** refits the panel around what it is grouped with.

Each change is one Undo step. Panels are artwork: reaction detection, atom
mapping and analysis never read them.

### Keep rows, columns and grids

1. Select two or more molecules, groups or reactions.
2. Choose **Lasting layout…** in **Properties → Grouping & frames**, or
   **Row with even gutters**, **Column with even gutters** or **Grid with even
   gutters** in the **Distribute** menu. A whole reaction (its arrow,
   participants and conditions) moves as one item.
3. **Properties → Layout** switches between **Row**, **Column** and **Grid**,
   and sets **Gutter (pt)**, **Columns** and **Start**, **Center** or **End**
   alignment. Switching keeps the reading order.

Items only move: bond lengths, labels and reaction roles stay as drawn. When an
item changes size, for example after adding a bond, its neighbors move to keep
the gutters. Moving one item by itself leaves it where you put it.
**Ungroup** ends the layout.

### Draw a catalytic cycle

1. Draw the intermediates as structures or text, roughly in a circle.
2. Select them and choose **Make catalytic cycle** from the canvas menu or
   the command palette. The drawing shows them on a circle through them,
   ordered clockwise from the top, with an arc arrow from each to the next,
   and **Properties → Make catalytic cycle** lists the order. Choose
   **Reverse order** to run it the other way, then **Make catalytic cycle**
   or Enter to make it, or **Discard** or Esc to leave the drawing as it was.
   To start from an empty circle instead, choose **Catalytic cycle** in the
   command palette's insert commands.
3. To add a part, drag a structure or text that is not in a cycle. On the
   circle it becomes a new intermediate between its neighbors; beside the
   first half of a step's arc it becomes a reagent coming in, beside the
   second half a product going out. A dashed outline and a tag such as
   **Intermediate 3** or **Reagent of step 2** show the role before you let
   go. A part already in the cycle keeps its role when it moves.
4. Clicking any part selects the whole cycle. Click the part again to select
   it alone, then drag it: every arc and side arrow refits to it. Let go
   farther out than a reagent's place, and it leaves the cycle; the tag
   **Release to take it out of the cycle** and the arcs closing up without
   it show this before you let go. Option/Alt-drag does the same in one go.
   With the cycle selected, a click on its circle or an arc selects only the
   circle and its arrows, to restyle them. Right-click and choose **Select
   structures in cycle** to select every intermediate, reagent and product,
   for example to change their color or font.
5. In **Properties → Catalytic cycle**, choose **Clockwise** or
   **Counterclockwise**. Open **Layout** to set **Radius (pt)**, **Gap (pt)**
   and **Center** text. Each row shows the step name or the intermediates'
   names, formulas or text. Choose a step to see its reagents under **In:**
   and its products under **Out:**. Set the step's **Name**, turn on
   **Reversible** or **Hide arrow**, or adjust **Bow**, **Space in** and
   **out** in points. **Purpose and properties** opens the parts' purpose and
   molecular properties; **Graphic style** below restyles the circle and its
   arrows.
6. **Remove from cycle** also takes the selected part out. It stays in the
   drawing.

Each change is one Undo step. A reagent's side arrow joins the arc without a
head; a product's leaves it with one. Reaction detection never takes a
cycle's parts, so a scheme drawn beside the cycle is read on its own. A
structure or text that already has a role in an ordinary reaction leaves that
reaction when it joins a cycle, and the status line says so. Taking it out of
the cycle again does not bring the role back.

### Use a cycle's steps as reactions

Each step is a reaction: its reactants are the step's first intermediate and
its reagents, its products the next intermediate and its products. Texts are
its conditions.

1. Select the cycle and open the step in **Properties → Catalytic cycle**, or
   right-click the step's arc.
2. Choose an action under the open step or in the right-click menu.
   - **Copy step as reaction SMILES** copies that step's reaction.
   - **Check step balance** says whether the step keeps every atom,
     hydrogens included, and the charge, and what one side has more of.
   - **Auto-map step** proposes atom maps for the step. Review them under
     **Properties → Atom maps** and choose **Apply maps**. New map numbers
     stay clear of the cycle's other steps, and maps that would clash with a
     neighboring step are refused. As for any reaction, atoms inside an
     abbreviation such as PPh₃ get maps too, which expands the label.
   - **Color cycle by origin** colors each reactant and carries its color
     into the products along the atom maps, as for a scheme of several steps.
     Every step whose two sides share atom maps takes color, so an
     intermediate keeps its colors from one step into the next. Without maps
     it runs **Auto-map step** first, and applying the maps colors the cycle.
3. The same commands are in the command palette.

Atom maps and colors are one Undo step each and are saved with the drawing.

### Open a cycle in ChemDraw and back

1. Export the drawing as CDXML or CDX. ChemDraw gets each cycle as its arcs,
   side arrows and step names, grouped with its parts, and each step as a
   ChemDraw reaction step from one intermediate (and its reagents) to the
   next (and its products).
2. Open the file in ReShiki again. If nothing in it changed, every cycle
   comes back as a cycle, with its steps, names and settings.
3. If the file was edited and saved elsewhere, it opens as that program
   saved it: plain arrows, text and structures, and a message says so. Select
   the intermediates and choose **Make catalytic cycle** to make the cycle
   again, then drag the reagents and products back beside their steps.

### Place lab equipment

Lab equipment is a preview in this build. The catalogue has round-bottom
flasks (50 and 100 mL with 15/25 joints; 100, 300 and 500 mL with 29/42),
Liebig condensers, glass stoppers, rubber septa, a reducing and an
expansion adapter, a cow receiver adapter, a thermometer and its adapter,
a balloon on a needle, a lab stand, a three-finger clamp, a support ring
and joint clips for 15/25 and 29/42.

1. Click **Insert equipment…** (the flask in the last row of the tools palette), or
   choose **Insert equipment…** in the command palette. The side panel lists
   the pieces.
2. Search by name or keyword in English or Japanese (`round`, `Liebig`,
   `ナス`, `丸底`, `29/42`), or pick a category.
3. Click a size such as **100 mL · 15/25**. The piece follows the pointer;
   click to place it. With **Keep placing** on, each click adds another copy;
   press Esc to stop. **Done** returns to Properties.

A placed piece is one object. Drag it to move it, turn it with the rotation
handle, and scale it from a corner; it never stretches, so there are no side
handles. Copy, paste, Undo and saving work as for any shape. **Properties →
Equipment** shows its name and size. Equipment is always an illustration: it
is never part of a reaction.

Pieces are drawn in line style at 0.4 pt per mm with 0.6 pt outlines,
whatever their size: white glass, hatched ground-glass joints and an open
socket mouth. Flasks start empty.

### Connect equipment by its joints

Pieces snap together by their ground-glass joints.

1. Drag a piece, or a new piece from the Equipment panel, so that its joint
   comes near a free joint of another piece. A green ring marks the joint it
   will seat on, and a label says what releasing does, for example **Seat on
   flask neck · 15/25**. The piece turns and scales to fit, and the preview
   shows it seated. A dragged piece snaps when its joint points within 40° of
   the other; a new piece turns to any joint.
2. Release, or click to place. The cone is drawn inside the socket, cut at the
   mouth.

Joints that do not fit get a dashed amber ring. When an adapter in the
catalogue joins them, the label says **Insert Expansion adapter, 15/25 →
29/42 on flask neck** and releasing puts the adapter between the two pieces.
Two sockets, two cones, or sizes no adapter joins only show why they do not
seat.

Seated pieces form an assembly. A click selects the whole assembly and a drag
moves it; turning, scaling, copying and **Bring to front** act on all of it.
To take a piece out, click it again so it is selected alone, then drag it:
the pieces held only through it come along, and the rest stays. The tag
**Release to take it off** shows before you let go. Option-drag (Alt-drag on
Windows and Linux) does the same without the second click. **Properties →
Equipment** lists each joint of the selected piece, what is seated in it and
**Detach**, which pulls the pieces apart along the joint and leaves a gap.

Select a piece and press **J**, or choose **Connect…** in **Properties** or
the command palette. The drawing shows the nearest joint that fits, seated. The arrow
keys move through the others, Return connects and Esc cancels.

An assembly keeps one layer. A piece that seats onto an assembly behind the
structures goes behind them too, and the label says **Goes behind the
structures**. Undo reverses a snap, an inserted adapter and a pull-out in one
step.

### Hold equipment: stands, clamps and clips

Clamps and stands hold glass the way they do on the bench, with depth: a
clamp's back fingers pass behind the neck it holds and its front finger in
front, and the stand's rod runs behind the flask. Exports and ChemDraw files
keep that order.

1. Drag a clamp so its jaws come near a flask's neck. The label says **Hold
   the flask · Ø13 mm**; release, and the fingers close on the neck whatever
   its size.
2. Drag the clamp, or the flask it holds, so the clamp's boss comes near the
   rod of a stand. The label says **Mount the clamp on the stand rod**.
   Dragging the stand to a clamp works too: the stand stays where you put it
   and the clamp comes to its rod.
3. Drag the flask or the clamp to slide them up and down the rod; they stop
   at its ends. The stand stays put. Drag the stand to move everything, or
   Option-drag (Alt-drag) the clamp to take it and the flask off the rod.

A rod holds any number of clamps. **Properties → Equipment** lists each clamp
on the rod with its height along the rod, for example **Holds the clamp at
280 mm**, and **Detach**. The support ring mounts on a rod the same way.

Joint clips go on seated ground-glass joints. Select an assembly and click
**Add clips to all joints** in Properties, or pick a clip in the Equipment
panel and click near a joint of its size; the label says **Clip the flask
neck · 29/42**. A clip moves with its joint and goes when the joint is
detached; click it a second time to select it alone and delete it.

### Move parts and set liquid levels

The built-in **Schlenk flask, 100 mL** and **Schlenk tube, 15/25** have
three-way stopcocks. The **J. Young storage flask, 100 mL** and **J. Young
Schlenk tube** have J. Young valves. Round-bottom flasks, Schlenk ware and
J. Young ware can hold liquid. The **Cow receiver adapter, 15/25** turns
between receivers. The **Thermometer** slides through the **Thermometer
adapter, 15/25**, and the **Balloon** on its needle inflates and deflates.

1. Click a piece to select its whole assembly and show handles for movable
   parts. Hover over a handle to see its setting and value, for example
   **Valve: Closed · click to switch**. A second click and a drag, or
   Option-drag, still pulls a piece out of the assembly. Holding Option disables the handles.
2. Click a circle to cycle through the states of a valve or stopcock. A
   J. Young valve switches between **Closed** and **Open**.
   A stopcock cycles through **Closed**, **Arm to side barb**, **Arm to down
   barb** and **All open**.
3. Drag a diamond to tilt a clamp or move a liquid's surface. The label
   beside the pointer shows the value, for example **Tilt: 25°** or **Liquid
   level: 40 %**. The percentage measures only the share of the vessel's
   drawn area below the surface. The surface stays level on the page when
   you turn the vessel.
4. Click the handle on the cow adapter's bulb to turn to the next receiver.
   Attached receivers move with their outlets.
5. Drag the top of the thermometer to set how deep it sits in its adapter.
   The setup stays put.
6. Drag the top of the balloon to inflate or deflate it.
7. **Properties → Equipment → Movable parts** lists each selected piece's
   settings: buttons for states such as a clamp's **Jaws**, and sliders for
   **Tilt**, **Neck diameter** and **Liquid level**. Each change, and each
   slider drag, is one Undo step.

Tilting a clamp on a stand turns the flask it holds about the boss head.
The stand stays put. Off a stand, the flask stays put and only the clamp's
boss head turns; mount the clamp on a stand to tilt what it holds. Opening the jaws releases the neck. Closing them on a
neck takes hold again. If a move would pull the assembly apart, as when two
clamps on the same rod hold a flask, the nearest clamp lets go. The status
line names the released piece, for example **Tilt: 25° · releases the tube**.

### Keep and share equipment pieces

The Equipment panel shows pieces from three places: the built-in catalogue,
**My pieces**, and lab folders you link. The second menu under the search box
shows one library or all of them.

- **Libraries → Import…** adds `.reshiki-piece` files (one piece) and
  `.reshiki-pieces` files (a lab's set) to My pieces. Every piece in the file
  is checked first; one bad piece refuses the whole file. Importing the same
  file again adds nothing.
- **Export…** writes My pieces as one `.reshiki-pieces` file to send to
  someone.
- **Link lab folder…** reads the pieces in a folder, such as a shared drive or
  a git checkout, and its subfolders. ReShiki never writes there. A file it
  cannot use is listed under the library; hover to see why. **Reload** reads
  the libraries again after someone changed them, and **Unlink** stops using
  a folder.
- With a piece armed from My pieces, **Remove** takes it out of the library.
  Drawings that use it keep it. **Undo library change** puts back the last
  import or removal.

Every drawing carries the pieces it uses. When you open a drawing from
someone else, **Properties → Equipment** offers **Add to My pieces** for a
piece you do not have, and **Export this piece…** for any piece. In an
assembly, click the piece a second time first.

A piece is known by its ID, revision and content. When a library has a newer
revision of a piece in your drawing, Properties says so, for example
**Revision 3 is in Tanaka lab (this drawing uses 2)**, and lists what the
update would release or reset. **Update** changes the selected pieces and
**Update all in drawing** every copy; either is one Undo step. Nothing updates
by itself. Two libraries with different pieces under one ID both stay; the
browser adds the library's name to tell them apart.

To make a review image of a piece file, run
`reshiki --cli pieces render FILE --to svg` (or `--to png -o FILE.png`; add
`--dark` for the dark canvas).

### Start from a figure layout

**Templates → Figure layouts** has a single-row TOC with a title bar, previous
work beside this work, a scheme beside an evidence panel, and two matched
scheme rows, each listed by name. Choose one, then click **Place** and click
the canvas: a layout is placed once and never joins an atom. Its benzene rings
and arrows are placeholders with the **Illustration** purpose, so they never
form a reaction. Select a panel (in the matched rows, Option- or Alt-click
picks one row) and choose **Clear contents** in **Properties → Panel** to
delete them in one Undo step, keeping the frame and title, then
paste or draw your scheme: a paste lands in the middle of the selected empty
panel. A border title breaks only the panel's outline, so a page
gradient or border behind it stays whole, and it takes a color that reads on
both the panel and the page. To
see a TOC graphic at its printed size, choose **ACS TOC graphic · 8.25 × 4.45
cm**, **RSC graphical abstract · 8 × 4 cm** or **Elsevier graphical abstract ·
13 × 5 cm** in [Page setup](publication-pages.md).

## Limits

- Node editing and continued drawing work on one continuous path. Imported
  compound paths keep a simpler point editor.
- Closed and compound paths draw no arrowheads. Closing a path keeps its head
  settings, which reappear when you open it.
- CDXML does not keep the shape type. External saves may split or reorder
  objects and round colors and coordinates.
- A single-segment curve with solid heads returns from CDXML as a curved
  arrow.
- Dotted strokes cannot be exported to CDXML.
- ChemDraw draws waves only on straight lines without arrowheads. Other wavy
  lines, curves and arrows open in ChemDraw as solid strokes.
- ChemDraw has shadows only on upright rectangles and ovals. A shadow on a
  rotated rectangle, a single closed path or a panel is written to CDXML and
  CDX as a grey shape behind it, which opens as a separate shape. Other paths
  lose their shadow there.
- ChemDraw has no glows or discs. Highlights export as ChemDraw's flat
  highlight and glowing or disc ring fills as flat ring fills, with a notice.
- ChemDraw has no gradient or see-through fills. A gradient is written as the
  flat color halfway along it. A see-through fill in its outline's color is
  written as ChemDraw's faded fill, which covers what lies behind it; other
  see-through colors are written as they look over white paper. Export shows a
  notice. Ring interior effects are written flat in the same way.
- Shading paints each part of a compound path on its own, so a hole in it is
  shaded over. A path too detailed to shade keeps a flat fill.
- Freehand tracing and non-proportional resizing are not available.
- ChemDraw has no panel object. CDXML and CDX get a group of filled curves,
  lines and text, and export says so. ReShiki opens a CDXML file it wrote with
  the panels whole; a CDX file gives those shapes. Layouts are not written to
  ChemDraw files.
- A panel's title uses the drawing's font and size in bold. Title text is
  edited in **Properties**, not on the canvas.
- A catalytic cycle is a circle. ChemDraw has no cycle object: CDXML and CDX
  get its arrows, text and reaction steps. Only ReShiki reads the cycle back,
  and only from a file nothing else has changed.
- A step's tools need a structure as each of its two intermediates. A step
  next to a text intermediate has no reaction SMILES, atom maps or balance
  check.
- Color by origin on a cycle uses the default hues and has no legend; its
  settings are not kept. Undo removes it.
- The liquid level, a clamp's tilt and other parameters cannot be changed
  yet. Nothing sits in a support ring until pieces with a place for it come.
  ChemDraw gets each piece as a group of curves; opening that file again
  gives those curves, not a piece.
- A drawing with equipment saves as document version 40 and carries the
  definition of every piece it uses, so it opens the same way on another
  computer.
- Linked lab folders are read up to 4 folders deep and 5,000 files. Symbolic
  links and hidden folders are skipped. A piece file may be up to 1 MB and a
  library file up to 32 MB with 2,048 pieces. IDs starting with `reshiki/`
  belong to the built-in catalogue, so library files cannot use them.
- Rotating a whole layout changes its items' sizes, so the next size change
  lays them out upright again.
- Brackets are graphics. To define a repeating unit, see
  [generic structures](generic-structures.md).

## Implementation notes

Single-shape and orbital tools return to Select after committing. Chemical
symbol tools keep placing, attached or free, until Escape or another tool. Stroke swatches are the
palette's Ink and Strong row, and fill swatches its Tint row, where the
crossed-out swatch removes the fill; RGB hex entry is available. Brackets offer
both, left or right sides. Background graphics leave molecular atoms and bonds
selectable.

Pen paths: click the first node after three or more nodes to close. Closing
adds a straight segment from the last node to the first; opening removes that
closing segment. Choose the last node and **Curved** to reshape the closing
edge. Inserting a cubic node exactly splits the segment without reshaping it;
deletion joins its neighbors while keeping their outer tangent controls.
Undo/Redo keeps the selected object. Imported compound paths keep their generic
point editor and geometry, while node insertion/deletion and pen continuation
require a single connected path. Pen paths store an optional `node_types` entry
for each node. Older drawings detect types from their direction controls until
the path is next edited; saved types survive reopening, copying and pasting.

Document format version 4 adds graphics with an affine coordinate frame, so
transformations preserve editable shapes. Versions 1–3 still open with no
graphics by default. Cleanup and analysis preserve graphic objects. Undo of
graphic or typography changes keeps the current chemistry analysis. The tool
palette scrolls when the window is short. The scene used for canvas preview
also drives SVG, PDF and PNG export.

Panels are rectangles or rounded rectangles with a `panel` entry: titles,
title place, outline and dividers at fractions of the width or height. A lasting
layout is a `layout` entry on a group: flow, gutter, alignment and the order of its items. Its items
are the group's connected molecules, nested groups and whole reactions. After
each edit the layout is laid out again if an item changed size or number.
`graphics::panel::contents` lists what lies wholly inside a panel. Both entries
need document version 40. In CDXML a panel's group carries a `ReShikiPanel`
attribute: the panel as JSON, its frame in page points and its colors as
written. ReShiki reads that group back as the one panel; ChemDraw ignores the
attribute, and CDX has no place for it.

A catalytic cycle is an ellipse graphic with a `cycle` entry
(`crates/model/src/cycles.rs`): direction, gap, center text, intermediates
in order (each the atom IDs of one molecule or one text's ID) and one step
per intermediate with its name, reversible and hidden flags, bow, extra space
and reagent and product parts. Arcs, side arrows and names are computed on
every draw by `cycles::drawn` from the parts' ink, so they are never stored
and never enter the drawing's arrow list. Each arc starts and stops where it
leaves the ink box grown by the gap; a side arrow is a curve from its part's
ink that meets its step's arc along the arc's way, at least 1.2 bond lengths
along from the part. Step names try the inside and outside of the arc's
middle, then points along it and further from it, and take the place covering
the least ink without crossing the arc. After each edit the parts grow to whole
molecules, parts that went are dropped, and the steps on both sides of a
dropped intermediate join. Reaction preview, automatic reaction copy and the
arrow hints skip every cycle part, and a part that joins a cycle leaves its
ordinary reaction roles, condition lists and arrow labels. CDXML and
CDX export write the drawn arrows and text in the cycle's group
(`cycles::materialized`), and one ChemDraw `scheme` per cycle with a `step`
per step: reactants, products, arcs and texts above the arrow. A cycle needs
document version 40.

The cycle itself rides along privately (`exchange::cycles`): a `ReShikiCycles`
attribute on the CDXML root, or CDX property 0x5243 on the document object
(the range 0x4000–0x7FFF the CDX format leaves to other programs). Its value
is a SHA-256 fingerprint of the rest of the file and the data, a space and
base64 JSON of each cycle graphic in file coordinates. A drawing with a cycle
writes each molecule as a fragment of its own, grouped or not. A cycle whose
parts are not each exactly one fragment or text writes no data. Parts are named by the file's own
object IDs: a structure by the outermost `fragment` holding its first atom, a
text by its `t`; the drawn arrows, paths and names by theirs. The CDX writer
seals the binary file again with its own fingerprint. On import,
`take_cdxml`/`take_cdx` remove the data and compare the fingerprint. A match
restores each cycle that checks out (valid, every part a distinct imported
object): `CdxmlScene::file_objects` maps the file's object IDs to the imported
objects, the drawn objects are deleted, the cycle takes their place in their
groups (or joins the groups holding all its parts when it drew nothing) and the
parts grow to whole molecules. A restored drawing that fails validation is
dropped for the plain import. A mismatch, or a cycle that does not come back,
adds a warning. Converting a ReShiki CDXML file to CDX seals the data again
only if the CDXML file was unchanged. A file whose cycles came back
counts its schemes and steps as kept on save.

Steps as reactions (`cycles::steps`) are never stored. For a step, or every
step for the atom-map search and color by origin, a throwaway drawing holds
only the steps' parts, one forward arrow per step with an ID the drawing does
not use, and a `Reaction` per step; Reference and Illustration structures are
left out, and a step's tools need structures as both intermediates. Reaction
SMILES, the atom-map search and `reaction_colors::paint` run on that copy.
Applying maps writes only the participants' map numbers back
(`steps::copy_maps`); coloring first clears every default color-by-origin
hue from the cycle's participants, then writes label and bond colors,
highlights and ring fills back onto the same atom IDs, replacing a ring's
fill in place. Each is one Undo step. An atom-map proposal for a step
remembers its cycle and step, and the review compares it with a fresh copy.
The balance check counts the elements of each side from the chemistry
graph, with explicit and implicit hydrogens, and the formal charges.

Piece libraries (`crates/io/src/piece_library.rs`) are folders of files.
My pieces is `pieces/` in the data folder; the linked folders are listed in
`piece-libraries.json` beside it. Each piece file is named after its key
(`id@revision#hash`), so two windows saving the same piece write the same
bytes and no lock is needed, and an edited piece with an unchanged revision
lands beside the old one. Removing moves the file to `pieces/.deleted/<time>/`;
undoing an import moves the added files there too. The browser lists each
key once: built-in first, then My pieces, then the linked folders in order.
`equipment::revise::update` moves placed pieces to another revision: a
parameter value the new revision cannot take is dropped (its default
applies), and `solve::check` releases seats whose joints moved.

Equipment seats are stored in the drawing's `seats` list as pairs of
`{piece, joint}` (`crates/model/src/equipment/solve.rs`). Poses are stored,
never solved again: after every edit and when a drawing is read,
`solve::check` keeps a seat only while the male shoulder is within 0.05 mm of
its place, 0.06 L outside the female mouth, the axes are opposite within
0.2° and both pieces have one scale. A seat that fails is released and both
pieces stay where they are. Snapping (`solve::snap`) compares the moving
pieces' free joints with every other free joint, measuring from the socket's
contact point to the cone, shoulder to tip, so a cone caught by its tip counts
as close. Reach is 14 screen px or the joint's diameter, whichever is larger, and the score is 12 × the distance
divided by the reach plus 0.3 × the angle in degrees. A joint that fits
wins over a closer one that does not. The seat transform turns the moving
assembly so the axes oppose, scales it to the target's scale and puts the
male shoulder 0.06 L outside the female mouth, whichever piece moves; every other pair of free joints that then fits within
the tolerance is seated too. An inserted adapter is the first catalogue
adapter with one joint that mates each side. A merged assembly takes the
target's layer and follows it in the drawing order, so painting, hit testing
and export agree; a layer change on one piece moves its assembly with it. The
anchor that stays when a piece is pulled out is the biggest vessel (a piece
with a liquid region), else the biggest piece.

### Interchange boundary

CDXML exports solid/dashed graphics as editable Bézier paths. Separate fill and
stroke colors use grouped paths. It imports supported plain curves, curve-only
groups, basic rectangles/lines/bracket pairs, and ovals with explicit axes.

Shaded or shadowed rectangles, rounded rectangles and ellipses are written as
ChemDraw's own Rectangle and Oval graphics with their Shaded, Shadow or
Shadowed flags and ShadowSize (hundredths of a point), so ChemDraw can restyle
them. Upright rectangles only: a rotated one stays a path. ChemDraw paints a
shadowed outline's inside with the page color. ReShiki opens an unfilled one on
a colored page, such as a ChemDraw file with a black background, filled with
that color. A shaded path is a
ChemDraw shaded curve (CurveType 256, FillType Shaded). A line without heads
is ChemDraw's line graphic with LineType Solid, Dashed or Wavy, and opens again
as a line; a straight wavy arrow keeps LineType Wavy. ChemDraw stores what its
line and arc tools draw as arrows without heads. ReShiki opens such an arrow,
when no reaction step lists it and it has no fill, no-go or dipole mark, as a
graphic: a straight one as a line from its tail to its head, an arc as a path
of Bézier segments of at most a quarter turn each. On import, ChemDraw's Shaded paints the shape's one color
and Shadow keeps its ShadowSize. ChemDraw shades an open curve as if closed,
and draws a wavy curve or a wavy line with heads straight; ReShiki imports them
the same way. ReShiki draws the shading as flat bands, as ChemDraw's own
renders do, three bands per point of size (32 to 256), so canvas, PNG, SVG
and PDF match and the steps stay invisible when zoomed in. Drawings with shading,
shadows, wavy strokes, arrowheads on graphic lines or a set corner radius are
document version 40, as are gradient and see-through fills and ring interior
effects. ChemDraw's CornerRadius (hundredths of a point) becomes
**Corner (pt)** on import and is written back for native rounded rectangles.
Curves nested in molecular fragments are also kept on import.

ReShiki paints graphics behind the chemistry, then structures, then text and
arrows, then graphics in front. ChemDraw paints every object in its own Z
order. On import a graphic goes in front only when its Z is above every
visible fragment, atom, bond, text and arrow (a line or arc read as a graphic
does not count); every other graphic goes behind, in Z order. Text ChemDraw shows
over a shape therefore stays visible. The trade-off: a shape ChemDraw stacks
over part of a structure, under a later text, opens behind the structure.

ChemDraw paints the inside of a plain closed rectangle, oval or curve with its
color at FadePercent tenths of a percent over white (CDX property 0x0303,
INT16), measured with ChemDraw's own renderer. ReShiki reads that as a fill in
the stroke color with opacity FadePercent / 1000 and writes it back the same
way. Filled and shaded shapes, lines and arrows show no fade in ChemDraw, so
FadePercent is ignored there; a value that is not a number is refused. A
gradient is a `Gradient` scene primitive laid out in the unit square of the
shape's frame (its own axes for rectangles and ellipses, else its bounding
box). SVG writes it as a native `linearGradient` or `radialGradient` with a
`gradientTransform`, so PDF and PNG keep it exact, and the canvas draws it with
resvg from the same SVG, as it does 3D models. Its colors are the ones the flat
fill would show on the canvas. A see-through fill wraps the fill in an
`Opacity` primitive. A glow is a `Glow` primitive: the shape filled and stroked
`spread` wider, under an SVG `feGaussianBlur` whose filter region covers three
standard deviations; svg2pdf rasterizes the blur in PDF. Highlights glow with a
spread of 0.4 and a blur of 0.5 highlight radii; ring fills with 0.1 and 0.2
bond lengths. A highlight disc is 1.6 times the flat highlight's radius; a ring
disc reaches 1.15 times past its farthest atom. `Document::highlight_style`
stores the drawing's style (document version 40).

CDXML does not keep ReShiki's parametric shape type. External saves may split
grouped bracket strokes, change stacking values and quantize colors/coordinates.
A subsequent import keeps the supported visible paths but may have a different
number of graphic objects. `.rsk` files keep ReShiki's exact model.

`.rsk`/SVG/PDF/PNG keep dots. SVG/PDF/EMF/PNG draw path arrowheads, and
CDXML/CDX keep their editable head settings on open curves.
ReShiki has no doubled curve or ChemDraw `alpha` transparency yet, and draws
only some of ChemDraw's other objects. A ChemDraw file that uses them still
opens: see
[ChemDraw styles ReShiki cannot draw yet](#chemdraw-styles-reshiki-cannot-draw-yet).
A bracket becomes a polymer
repeating unit, with a movable repeat label, through **Define repeating unit**;
see [generic structures](generic-structures.md#define-a-repeating-unit).

### ChemDraw styles ReShiki cannot draw yet

Opening or pasting a CDX or CDXML file never fails because of one style or
object. ReShiki imports the nearest thing it draws and shows one message that
lists what changed and how many, such as "2 doubled curves as pairs of
grouped curves; 1 rotated caption upright".

| In the ChemDraw file                                                         | In ReShiki                                                       |
| ---------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| Doubled curve                                                                | Two grouped parallel curves; with a fill or arrowheads, one line |
| See-through object (alpha)                                                   | Opaque                                                           |
| Arc from an older ChemDraw file                                              | The same arc as a curve or curved arrow, without a message       |
| Arc arrow without its centre or axes                                         | A straight arrow                                                 |
| Curved arrow with a double shaft; longer curve with a no-go or dipole mark   | The curve without the mark                                       |
| Two upright single brackets of one style that face each other                | One bracket pair, without a message                              |
| Any other single bracket                                                     | A curve of the same shape                                        |
| Dagger, double dagger, lone pair bar; plain, shaded and filled orbitals      | The matching ReShiki symbol or orbital, without a message        |
| Rac, Abs and Rel stereo flags                                                | Text grouped with a rounded box                                  |
| Unbalanced equilibrium arrow whose short arrow is under a fifth of the other | The short arrow a fifth as long as the other                     |
| Rotated caption; outlined or shadowed text                                   | Upright; plain face                                              |
| Atom or abbreviation label with more than one font, size or weight           | The style of its first character; colours are kept               |
| Line, arrowhead, shadow or text size outside ReShiki's range                 | The nearest size in range                                        |
| BioDraw shape, gel plate, TLC plate, other unknown object                    | Left out and named; captions and drawings inside it are kept     |
| Table                                                                        | The cell contents without the frame                              |
| Plasmid map in a CDXML file                                                  | Its ring, regions as arcs, and labels                            |
| Picture without a PNG, TIFF, JPEG, GIF or BMP copy                           | Left out and named                                               |

A file with several pages, such as one of ChemDraw's template libraries, opens
with its pages in rows on one drawing, in page order, so **Fit** shows them
all. Page boundaries, paper settings and empty pages are not kept, and the
message says so.

In a CDX file, a table, plasmid map, gel plate or TLC plate is named as "other
ChemDraw object", and the captions and drawings inside it are kept. A CDX
property or value ReShiki does not know is skipped on a drawing object. On an
atom or bond, an unknown property is skipped and named in the message, and an
unknown value still refuses the file.

Saving such a file back in ChemDraw's format writes ReShiki's drawing, not the
original objects. The first **Save** of the opened file counts them: "Keep as
CDXML loses 3 objects ReShiki draws differently or leaves out."

A file is still refused when it is cut off or otherwise broken, exceeds a size
limit, has nothing ReShiki can draw, or holds chemistry ReShiki cannot read,
such as a biopolymer sequence. A CDX file that an earlier ReShiki version saved
with a hash no-go mark or a lone pair symbol now shows a cross or a lone pair
bar, because earlier versions wrote them with other values than ChemDraw.

Format references: the
[published CDXML DTD](https://static.chemistry.revvitycloud.com/cdxml/CDXML.dtd),
and the published format documentation for
[curves](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/Curve.htm),
[graphics](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/Graphic.htm) and
[curve flags](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/Curve_Type.htm).

Mixed-object and nested groups were added later; see
[selection and groups](selection-and-groups.md) for membership, integral groups,
fitted frames and remaining exchange limits. Version 5 extends the format with
this group model.

### Records

The [original graphics checks](changes/editable-graphics-20260920.md) record
the first build's desktop, file-exchange and test runs.
