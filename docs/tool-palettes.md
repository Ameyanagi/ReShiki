# Visual tool palettes and drawing depth

Click **C** in the main tools palette to open a periodic table containing all 118 elements. Choose a symbol, then click an existing atom to change its element or empty space to place it. Hover shows the symbol and atomic number. Existing element keyboard shortcuts remain available.

Click a **bond**, **ring**, or **arrow** tool to open a floating palette with visual previews. The palette is a compact grid with names on hover; it does not stretch across the canvas. Select the desired style and continue drawing. The bond palette contains all 17 supported presets; rings include sizes 3–8, benzene, chair projections, cyclopentadiene and [Haworth 5/6 projections](haworth-projections.md). Click outside the palette, its close button, or Escape to dismiss it. Keyboard shortcuts still select tools directly.

The arrow palette includes **Curved / electron pair**, **Single electron**, and **Bent / elbow**, alongside straight reaction arrows. Elbow arrows have two straight segments. Select an arrow and drag its middle handle to place the corner; drag either endpoint to resize it. Properties offers reverse direction, flip bend and straighten. Elbows retain an editable corner in native files and supported CDXML/CDX exchange.

At a crossing between unconnected bonds, ReShiki cuts a small gap in the lower bond. Shared atom junctions and parallel bonds stay continuous. To choose which bond passes over the other, click the middle of that bond with Select, open **Properties → Crossing bonds**, and choose **Bond in front** or **Bond behind**. These are display changes and are undoable. They do not add atoms or change molecular connectivity.

Canvas, SVG, PDF and PNG share the crossing geometry. Native document version 11 stores per-bond depth; versions 1–10 remain readable. Editable exchange records standard bond Z order and crossing references. Automated round trips cover the elbow and crossing depth; interactive verification in external drawing windows is still pending.

## 3D tilt and right-click commands

Choose the tilted-ring icon in the second row of the left tool palette. Click a molecule to select it, or select a region by dragging empty space, then drag the selection to tilt. Vertical motion rotates around X; horizontal motion rotates around Y. Hold Shift for 15° snapping. Each gesture previews without changing the document until release and creates one Undo step. Escape cancels; Done, Space or Escape returns to Select. X/Y ±15° and Front bonds remain available above the canvas.

Right-click a ring or selected objects for 3D tilt, Arrange & transform, Bond appearance and Attachment points when applicable. Clipboard commands form a separate section; Delete stays at the bottom. Empty canvas offers Undo/Redo, Paste, Select all and Fit drawing. Right-clicking an existing selection retains it. Aromatic and substituted ring interiors are recognized, while ring fusion keeps its original stricter requirements.

Tilt retains XYZ drawing geometry and keeps labels upright. It does not infer or assign chemical stereochemistry.
