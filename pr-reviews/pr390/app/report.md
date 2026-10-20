Local UI verification, 2026-10-20 (Asia/Tokyo). **Part A completed. Part B is blocked by the computer-use browser URL policy, so an A2–B2 rendering comparison cannot yet be made.**

The branch build was launched with `open -n` from the requested `ReShiki Verification.app` bundle. Its PID was **49245**, executable `/Users/shiranui/dev/ReShiki/.claude/worktrees/wf_9a402735-96b-2/target/debug/reshiki`. All three captures came from its sole window, **window ID 6908**, titled `Untitled • — ReShiki`. Before each saved capture, read-only macOS process/window metadata verified the bundle, executable, owner PID and window ID. See [launch.txt](launch.txt), the per-image `.pid.json` and `.ax.txt` files, and [capture-manifest.json](capture-manifest.json).

Caffeine was inserted through Import → Structure using `CN1C=NC2=C1C(=O)N(C(=O)N2C)C`. The Recovery draft banner was dismissed. Properties → Structure → 3D produced the model; the status read “Shown as 3D model,” with no Generating message. The molecule was deselected for all saved screenshots. Before switching to Both, the Select tool and a bond were clicked to select the molecule again.

| Requested screenshot | Result and visible content |
| --- | --- |
| [A1-3d-fit.png](A1-3d-fit.png) | Full verification-app window, 2560 × 1704 pixels. Caffeine in 3D ball-and-stick view, deselected, after Fit. The View control reads **201%**. The complete model is visible on the white canvas. Properties show C8H10N4O2 and 14 atoms / 15 bonds. |
| [A2-3d-large.png](A2-3d-large.png) | Full verification-app window, 2560 × 1704 pixels. Same deselected 3D model after one View `+` increment; zoom reads **251%**. All visible extremities remain inside the canvas. This is the intended large branch-rendering reference. |
| [A3-both.png](A3-both.png) | Full verification-app window, 2560 × 1704 pixels. Both mode, deselected, after Fit, at **134%**. The 2D structural formula is on the left and the 3D ball-and-stick model is on the right, with a clear gap between them. Status reads “Shown as 2D and 3D.” |
| `B1-prototype-3d.png` | **Not captured.** Opening the supplied local HTML URL was rejected before the prototype loaded. |
| `B2-prototype-large.png` | **Not captured.** Prototype controls and a comparable enlarged view could not be reached. |

Concrete observations from A2, pending comparison with B2:

- **Ball shading:** gray carbon, blue nitrogen and red oxygen spheres have a pale highlight toward the upper left and a darker lower-right edge. Multiple closely spaced shading bands are visible around the highlights, especially on the central gray spheres and the upper blue nitrogen; the shading does not appear perfectly continuous.
- **Outlines:** every atom has a conspicuous dark gray circular rim. White hydrogens also have an inner gray shaded edge, making them stand out against the white canvas. There is no selection bounding box or atom-selection halo.
- **Stick widths:** the main ring bonds are approximately one quarter of a heavy-atom sphere’s diameter. Hydrogen bonds are similarly narrow. Light central strips and darker side edges give the sticks a cylindrical appearance.
- **Stick–ball joins:** most sticks end behind the atom silhouettes, without obvious white gaps. At the upper methyl carbon, a small gray crescent is visible beyond its upper-right edge near the hydrogen connection; the screenshot alone does not establish whether this is an occluded atom or a join artifact.
- **Overlaps:** the lower-right methyl group has a foreground white hydrogen overlapping the lower-right face of its gray carbon. Its outline stays visible across that overlap. Other sticks are hidden by their endpoint balls. The central ring remains legible, with no obvious unintended bond crossing.
- **Jagged edges:** contours are broadly smooth at the saved scale, but some diagonal stick edges and circular rims have mild stepping/softness. The capture transport initially supplied JPEG bytes, which were converted to PNG without resizing or cropping; individual-pixel edge defects cannot be attributed confidently to the app from these captures alone.
- **Colours:** carbons are medium gray, nitrogens saturated blue, oxygens warm red, and hydrogens near white. Bonds change colour in distinct segments: for example, the left oxygen–carbon stick changes from red to gray, and nitrogen–carbon sticks change from blue to gray. These are visibly discrete transitions, rather than continuous colour blends.

No visible difference between A2 and B2 is claimed because B2 does not exist. The prototype’s Ball & stick, Flat look and 3D settings remain unverified.

The Part B tool rejection stated that `file://` is not an allowed protocol, that only HTTP and HTTPS are permitted, and that alternate browser surfaces or indirect workarounds must not be used to reach the same blocked page. The full message is preserved in [B-prototype-blocker.txt](B-prototype-blocker.txt). An approved HTTP/HTTPS prototype URL is needed to continue. The rejected attempt created an `about:blank` task tab, ID 387416898; that tab was closed through computer use.

Cleanup: verification PID 49245 was quit. A post-quit CUA observation automatically reopened that same verification bundle as PID 51237; this second process was also quit, and no screenshots were taken from it. Both PIDs were confirmed absent afterward. The pre-existing installed ReShiki, PID **51217**, was still running and was never selected or operated. No repository files or system settings were changed, and no pre-existing app/browser windows were closed or operated.
