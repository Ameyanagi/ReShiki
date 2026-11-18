# Keep drawings on GitHub

The desktop app keeps drawings in your GitHub repositories. **Save** keeps a
drawing on this computer, at once and offline. **Send** makes one commit of
every changed drawing in that repository, each with an SVG preview beside it,
so drawings show on GitHub.

## Use it

1. Choose the **GitHub** button at the left end of the status bar, then
   **Open from GitHub…**.
   Signed out, the picker shows the repositories known on this computer,
   greyed, and **Sign in…**. ReShiki shows a code, copies it and opens GitHub;
   paste the code there and allow ReShiki. The dialog closes by itself, the
   status bar says **Signed in as @you**, and the picker comes back. Click the
   code to copy it again.
2. If ReShiki may not use any repository yet, the picker says **No
   repositories yet**: choose **Choose repositories ↗** and pick them on GitHub.
3. The picker opens the only repository, or the one you used last. Pick
   another on the left; open folders and drawings with a double-click, Enter
   or **Open**. Backspace goes up a folder; the parents in **lab-notes /
   scheme** go back too. Folders come first, then files, each by name; who
   changed each file last and when (**You** for your own changes) fills in
   as GitHub answers, and nothing moves. With a repository open, the search
   field filters its files.
4. Edit and save as usual. The status bar chip shows the repository, the
   folder and what is left: **● lab-notes / scheme · 1 unsent** with **Send**.
   Click the name to open the picker in that folder.
5. Choose **Send**. A small box above the chip says what goes where, such as
   **ring.rsk → lab-notes**, with the message selected: type over it or keep
   it, then press Enter. ReShiki first saves every open drawing of that
   repository, as **Save** does: the chip says **Saving…**, then
   **Sending…**, then shows **● lab-notes ✓**.

Each repository keeps a color. Its tabs show a badge with its initials, such
as **LN** for lab-notes, and the tab in front is underlined in that color; the
window title names the repository. Right-click a GitHub tab and choose
**Color** for another one.

In the picker, **+ New repository** (the last row) makes a private
repository, **+ New drawing** makes an empty drawing in the folder open there,
and **+ New folder** opens a new folder, which reaches GitHub with the first
drawing sent from it. A new drawing shows **not sent yet** until you send it.
Neither shows in a repository you may only read.

### Every chemistry file, and Insert

The picker lists the files **File › Open** reads besides drawings: `.mol`,
`.sdf`, `.cdxml`, `.cdx`, `.rxn` and `.smi`, each with a small label such as
**MOL**. One opens in a read-only tab, as an old version does; **Save**
(Cmd+S) is **Save a copy…**, named as a drawing (`aspirin.rsk`), into a
repository of yours or **This computer…**. The file on GitHub is never
changed.

**Insert** (or Shift+Enter, or right-click a file and choose **Insert into
drawing**) puts a file's structures into the drawing in front instead, placed
as a paste is (a large one beside what is there), as one Undo step. An SD
file's records land side by side. If they do not all fit in one drawing, the
ones that fit open or are inserted, and the status bar says so in amber, such
as **Opened 120 of 150 structures · The rest don't fit**.

A folder shows its first 200 files by name, and says how many there are;
the search field finds the others.

### Tiles or Tree

The two buttons at the end of the folder path switch between **Tiles** and
**Tree**. The picker opens in the tree the first time, then in the view you
used last, in every repository.
The tree lists the whole repository: click a folder or press → and ← to open
and close it, ↑ and ↓ to move, Enter to open, Shift+Enter to insert. Each row
shows the name, the format of a file that is not a drawing, and who changed
it when. The pane on the right shows the selected file's preview, its path,
and **Read only: Open makes a local copy** when it opens read only. Searching
keeps the matches in the tree with their folders opened, across the whole
repository. **+ New drawing** and **+ New folder** are the last rows; they go
to the selected folder.

## Other people's repositories

Paste a github.com link into the picker's search field (**Search or paste a
GitHub link**). One row shows where it leads, such as **ligands / esters /
route.rsk**; press Enter to open it. A link to a repository or folder opens it
in the picker; a link to a drawing opens the drawing in a tab. The same works
when you paste a link with Cmd+V (Ctrl+V) while no text field has focus, drop
a `.webloc` or `.url` link file on the window, or type the link into the
command palette (one row, **Open from GitHub: ligands/esters/route**).

**+ Add public repository…**, the last row of the list, takes `owner/repo` or
a link. A repository opened by link stays in the list, tagged **shared**,
until you right-click it and choose **Remove from list**.

Each repository row ends in **↗** while the pointer is on it or it is
selected; it opens the repository on GitHub in your browser. Right-click a
row for **Open on GitHub ↗** too.

Public repositories open signed out too. Signed out, the picker shows names
only and says **Sign in to see who changed each file and a preview.** GitHub
allows 60 signed-out requests an hour; past that, the picker says **GitHub's
limit for signed-out use is reached. Sign in to continue.** with **Sign in…**.

If GitHub will not show a repository, the picker says **Can't open
owner/repo**: it is private, or the link is wrong. **Copy request for the
owner** copies a message asking the owner to add ReShiki.

A repository you may not send to is **Read only**: its row shows an eye, its
tabs an outlined badge, and the chip **● ligands · Read only · 1 change ▾**.
Edit and save as usual; the saves stay on this computer. The chip's menu has
**Save a copy…** (choose a repository of yours, or **This computer…**),
**Copy request for the owner** and **Open on GitHub ↗**. When the owner adds
ReShiki or gives you access, the label goes away the next time you sign in or
come back to the window (checked at most every five minutes), and your saves
count as unsent.

A link to another branch or to a commit opens that version in its own tab,
apart from the drawing you work on. Its chip says **Old version** with **Open
latest**. You can edit it; **Save** (Cmd+S) saves a copy, and nothing of it is
ever sent.

Right-click a GitHub tab and choose **Copy GitHub link** for the drawing's
address on GitHub, or **Show in Finder** (**Show in Explorer** on Windows,
**Open containing folder** on Linux) for its working copy on this computer,
in the [data folder](https://reshiki.com/guide/install/#your-data-folder). The command palette also lists every drawing and
repository known on this computer, such as **Open from GitHub: lab-notes/ring**,
without asking GitHub.

## Send a drawing that is not on GitHub yet

To put a drawing that is not on GitHub yet into a repository, choose
**GitHub → Send to GitHub…**. Pick a repository and folder, type a **Name**,
then choose **Send**.

Closing a drawing whose saves are not on GitHub asks **Send** or **Later**.
Later keeps the saves on this computer; the chip shows them the next time you
open the drawing. The GitHub button has a dot while anything is not sent.

If a drawing changed on GitHub since you opened it, Send leaves it out, the
status bar says so in amber (**Not sent · ring.rsk changed on GitHub**), and
ReShiki shows **GitHub** and **Yours** side by side, compared as in
[Compare](#compare-two-versions). Choose:

- **Keep GitHub’s** (under GitHub's): your drawing becomes GitHub's. Undo
  brings yours back, and yours stays in Versions.
- **Keep mine** (under yours): yours goes to GitHub over theirs. Theirs stays
  in the repository's history.
- **Keep both**: yours is saved beside it as `<name> (2).rsk` and sent, and
  GitHub's opens in a tab next to it.

**Later** (or Escape) decides later; the chip shows **Review** in amber.

You also hear of a change before you send. When you come back to the window
(at most every five minutes), ReShiki asks GitHub about the open drawings. If
someone changed one there and you have no changes to it here, the chip shows
**● lab-notes · Newer on GitHub** with **Update**, and the tab's tooltip says
**Newer on GitHub**. **Update** (or **Update from GitHub** in the command
palette) shows GitHub's drawing in one undo step; the status bar says who
changed it, such as **Updated · @alice: Add an ethyl group** (**You** for a
change you sent from another computer). If you have changed the drawing here,
the chip shows **Review** instead, with the same choices as above.

**History**, the clock button just after the inspector's tabs (or **GitHub →
History**, or the chip's state), lists the saves on this computer since the
last send (**This computer**) and the commits on GitHub that changed the
drawing (**GitHub**), by day. It shows only for drawings on GitHub. GitHub's
list shows as soon as it arrives; each version's picture fills in as its row
comes into view. Select one, then choose **Compare** to see it beside the
drawing now, **Open** to see it in a new tab or **Restore** to make it the
drawing again. Restore is one undo step and makes a new save.

The GitHub button sits at the left end of the status bar, just before the
chip, signed in or not. It shows your initial while you are signed in; its
tooltip says **Signed in as @you** or **Not signed in**. Its menu opens
upward and ends with your account: **@you**, **Add repositories… ↗** and
**Sign out**. The theme editor hides the status bar; there, use the command
palette for GitHub, and **Send** opens as a dialog. The command
palette offers the GitHub commands that apply to the drawing in front, such as
**Send 1 drawing**, **History**, **Show in Finder**, **Copy GitHub link** and
**Open on GitHub ↗**;
typing a whole word such as `send`, `history`, `versions` or `repo` puts them
first.

### Compare two versions

Compare, and the changed-on-GitHub dialog, show both drawings with what
changed marked by kind, and a list of the changes beside them. The version
you compare with is on the left, in a dashed grey frame marked **COMPARED**
(**Version · saved 14:02**, or **On GitHub · @alice · 14:05**); the drawing
you have open is on the right, in a green frame marked **CURRENT** (**Your
drawing · open now**, or **· not sent**). Changes read from left to right. A
drawing much wider than it is tall, such as a reaction scheme, stacks the two
instead, the compared version on top, when that shows it at least 10% larger;
the footer then reads top to bottom. A badge beside the title says
**Chemically identical**, **2 chemical changes** or **Couldn't verify all
chemistry**.

- **Chemistry**, **Layout** and **Everything** (the default) choose which
  changes show, each with its count. Chemistry is atoms, bonds, charges,
  isotopes, hydrogens, stereo and reaction roles. Layout is moves, redraws,
  style and text.
- A molecule drawn as aromatic or Kekulé, a wedge moved to another bond with
  the same configuration, a collapsed label and refreshed R/S labels are not
  chemical changes. Chemistry that cannot be checked, such as an R-group, is
  **Couldn't verify** with the reason, never "the same".
- A moved or turned molecule gets a blue dashed outline and one **MOVED**
  line; when most of the drawing moved together, that is one line, **Whole
  scheme moved**. A molecule or shape flipped is **MIRRORED** (**Molecule
  flipped left–right**, **flipped top–bottom** or **mirrored**), and one drawn
  larger or smaller is **SCALED** (**Molecule scaled to 120%**); a whole
  scheme flipped or scaled is still one line, a straight arrow included. A
  molecule whose shape changed, as Clean Up does, is **Molecule redrawn**.
- The Flip tool swaps wedges and hashes, so a flipped chiral molecule keeps
  its configuration: **Wedges swapped; same configuration**, no chemical
  change. Mirrored without that swap, every stereocenter is inverted, and the
  chemical line says **Mirror image: every stereocenter inverted**.
- A chemical line says what changed, with the drawing's own atom numbers
  where it has them: **OH added at C4**, **Cl → Br at C2**, **NO₂ removed**,
  **C1–C2 single → double**, **inverted at C3**, the first two then **+N
  more**. A bare number such as `4`, `4a` or `1′` is read with its element
  (**C4**); other text, such as `N1′`, as written. A number on one side only
  says so (**C4 (current)**), and one the sides number differently reads
  **C5 → C4**. Without numbers there is no position: the paint shows where.
- **Line up moved molecules** (on by default) shows each moved object of the
  right side where the left side has it, so real differences stand out. Turn
  it off to see where they are.
- Each line has a tag that reads without colour: **CHEM**, **MOVED**,
  **STYLE**, **TEXT**, **RXN**, **ADDED**, **REMOVED**, **SAME** and **?**.
  The legend reads **Moved, mirrored, scaled**.
  A chemical change shows its SMILES before, then → after on a line of its
  own; a long SMILES wraps between atoms and bonds, never inside `[NH4+]`,
  `Cl` or a ring bond. Click a line to point at it in both drawings: an
  outline marks a chemical change's changed atoms and bonds, and both
  drawings zoom, at one scale, to show it with its whole molecule around
  it, at most twice as large as the full view. Click it again to let go.
- The comparison runs in the background (**Comparing…**), so a big drawing
  never stops the window.

## Limits

- Desktop only. The browser editor has no GitHub sign-in yet.
- ReShiki sends to each repository's default branch, and only `.rsk` drawings
  and their `.svg` previews. Other branches and commits, and other chemistry
  files, open read only.
- A repository too big for GitHub to list whole shows part of it in the tree,
  with **Only part of this repository is listed**; **+ New** rows are then
  left out. Tiles list each folder in full.
- A link names a branch by its first folder: `tree/feature/x/...` reads the
  branch `feature`.
- Only `.webloc` and `.url` files carry a dropped link; a link dragged from a
  browser's address bar is not received.
- A change to the drawing's style or settings is not painted. The compare
  lists **Style or settings changed** instead.
- Compare matches molecules by ReShiki's own ids, then by SMILES. A molecule
  both changed and given new ids, as after importing it again, is matched by
  its structure when that is clear, and its chemical line says **matched by
  structure (ids changed)**. Otherwise, such as a molecule rewritten beyond
  half its bonds, a chain made a ring, molecules sharing fewer than three
  bonds, two equally good candidates, or more than 32 such molecules a side,
  it shows as removed and added. A shape flipped or scaled is told from a
  redraw only for molecules and graphics of three points or more off one
  line; an arrow drawn longer is redrawn. Atom numbers come from the
  drawing, never generated.
- Send sends saved files. It first saves every open drawing of that
  repository; a drawing that is not open is sent as last saved.
- On Linux without a keyring, the sign-in lasts until ReShiki quits:
  **Sign-in won't be remembered on this computer.**

## Implementation notes

- GitHub App "ReShiki" with device-flow sign-in. There is no client secret or
  private key anywhere. Tokens expire after 8 hours and refresh without a
  secret. They are kept only in the system credential store: Keychain on macOS,
  Credential Manager on Windows, Secret Service on Linux.
- Working copies live in `<data>/github/<owner>/<repo>/files/`, with
  `state.json` (what GitHub had for each drawing) and `versions/` (up to 50
  saves per drawing since its last send). A drawing is unsent when its saved
  file differs from the bytes last synced, so nothing else needs to be stored.
- Files are read and written off the window's thread. Whether drawings are
  unsent is read in the background when a drawing opens and on window focus;
  after a save, only that drawing is read back. Opening from the picker,
  **Update** and a conflict choice write the working copy in the background:
  **Send** and closing the window wait for a write still running, and Update
  never writes over a drawing that changed here meanwhile.
- History lists GitHub's versions from the branch in one request (20 a page).
  A version's drawing is read when its row first comes into view, once per
  commit, and kept with its list for the session, so a drawing's History
  shows again at once; signing out forgets them.
- On window focus, at most every five minutes and only while GitHub drawings
  are open, ReShiki asks each open repository's branch head for the open
  drawings' blobs: the repositories you may send to whether a drawing changed
  since its base, read-only ones whether you may send to them now. A changed
  drawing costs one more request for its last commit and one for its bytes.
- Send reads the repository by id, builds one tree on the branch head and
  moves the branch only forward. If the head moved, it retries against the new
  head.
- The compare (`reshiki::compare`) matches molecules (connected atoms) by
  shared ids, which must keep, between atoms of the same elements, at least
  half the bonds of the smaller; then by canonical isomeric SMILES,
  duplicates by where they sit once the common move is removed; then by
  SMILES without stereo or isotopes; then by structure. Shared ids keeping
  exactly half the smaller molecule's bonds, as when a fresh import gives
  ethanol ids an ester had and one of its two bonds lines up, count only for
  molecules nothing else matched. Text, arrows and graphics match by id,
  then by content and place. Structure matching pairs the atoms with the
  reaction mapper's bounded search, under Compare's own rules: atoms pair by
  element, and a bond counts as kept only with the same order and ring
  membership. Kept heavy-atom bonds over the bonds of either molecule (the
  Tanimoto similarity) must be at least 0.5, with at least three kept, and
  every ring system must pair with exactly one on the other side through
  ring bonds kept between them. A pair must be each molecule's clear best:
  similarities are compared in steps of 0.05, an equal rival must be at
  least twice as far, and a rival of unknown worth that could be as good
  leaves both unmatched: one the budget skipped or cut short, one over 128
  atoms, or one whose best pairing splits its ring systems. The search
  counts work, not time (20,000 states and 10,000,000 work a pair,
  40,000,000 a comparison), so the same two drawings always give the same
  report. Of equally good atom pairings, the nearest once the two molecules
  are laid over each other wins, so a ring substituted again is marked at
  the same atom on both sides; the fit leaves out matched atoms whose bonds
  to other matched atoms changed. Each matched molecule is compared as
  chemistry over its prepared, sanitized structure, and fitted onto its old
  self with a proper 2D rigid fit (Kabsch): within 0.01 bond lengths RMS and
  0.03 at most it moved. Failing that, a molecule or graphic of three points
  off one line is fitted reflected, then scaled (by at least 1%), then both,
  at the same tolerances; beyond that, with the same chemistry, it was
  redrawn. A move at least two objects and three in four share is **Whole
  scheme moved**, refitted with its reflection and scale; an object it
  carries within the tolerances, such as a straight arrow under a scale, has
  no line of its own. Lined up, a reflection's display is undone with the
  Flip tool's own transform (wedges, double-bond sides, hydrogen sides and
  number offsets), and style is compared on those lined-up values.
  Everything else in the file is compared as a whole for **Style or settings
  changed**. The marks are drawn as a layer over the drawings; neither
  drawing is changed.
- Tiles list one folder at a time, at most 200 matching the search. They
  render the file itself, the way an opened file renders (an SD file's first
  record); no preview file from the repository is parsed. A folder's names
  show as soon as it is listed, folders first, then by name. Each file's last
  commit (one, as of the listed commit) and preview are one request each,
  made only once its tile is on screen, and fill in without moving anything.
  A folder opened again in the same repository asks for neither again.
- Opening a file from the picker reads it by the blob the listing named, one
  request, or none once its preview was read; its working copy is based on
  the listed commit. If someone changed it on GitHub since the listing, the
  check on window focus offers **Update**, and Send finds the change in the
  way as a conflict: it never goes over it.
- What GitHub never changes is kept while ReShiki runs, apart for each
  sign-in and for signed-out reading: folders and files by their object
  names, commits by name, and a file's last commit as of a commit. Up to
  64 MB, the least recently used going first. Choosing a repository still
  asks for its branch head each time, so new commits show; going up, into a
  folder again or choosing the repository again asks GitHub for nothing
  else.
- The same replies are also kept on disk, so they last past a restart, in
  `cache/github/<login>/` in the data folder (`~/.reshiki/cache/github/` on
  macOS and Linux, `%LOCALAPPDATA%\reshiki\ReShiki\cache\github\` on
  Windows); signed-out reading keeps its own in `cache/github/_public/`, a
  name no GitHub login takes. One
  folder per account keeps one account's private repositories from another,
  so signing out clears nothing. Replies that can change (a branch head, a
  repository, the account's repositories, a drawing's History) are kept with
  GitHub's ETag and asked again each time with `If-None-Match`; GitHub's
  304 (unchanged) costs no download. After a restart, choosing the same
  repository asks only for its branch head (a 304 when nothing moved), and
  opening the same folder or drawing asks for nothing.
- The disk cache holds up to 200 MB for all accounts together. Past that,
  the entries used least recently go until 180 MB are left. Each entry is
  written to a temporary file and renamed into place, with a SHA-256 of the
  reply; an entry that is cut short or unreadable is removed and read from
  GitHub again, never shown as an error. Reads and writes run off the
  window's thread. Delete `cache/github/` (or the whole `cache/`) whenever
  you like, with ReShiki closed or not; it is rebuilt as you browse. With
  `RESHIKI_GITHUB_API` or `RESHIKI_GITHUB_WEB` set, replies go to
  `cache/github-<hash of the API address>/` instead, so a test server's
  never pass for GitHub's or another server's.
- Requests go over HTTP/2 where GitHub offers it, one connection for the
  previews, dates and uploads running at once; signed-in, signed-out and
  sign-in requests share the connections. Send uploads each drawing and its
  preview four at a time, then makes the tree, the commit and the branch
  move in that order.
- The tree reads the repository whole with one recursive tree request when
  the repository opens, kept until it opens again. Only its rows on screen are
  built; their last commits, and the selected file's preview, are asked for as
  they come on screen. Its folders and files sort by name.
- A file other than a drawing is kept like an old version, in `old/<commit>/`,
  and read the way Insert reads it. Save on its tab is Save a copy.
- Signed out, requests carry no token and reach public repositories only.
  Opened by link, a repository's id, names, default branch and whether you
  may send to it are kept in `prefs.json` (`shared`); a working copy's
  `state.json` says `read_only` while you may not. Old versions live in
  `<data>/github/<owner>/<repo>/old/<commit>/`, outside `files/`, so nothing
  there is ever sent.
- Repository colors, the repository last used and those opened by link are kept in
  `<data>/github/prefs.json`, by repository id. A new repository gets the hue
  its lowercase `owner/repo` hashes to (FNV-1a), or the next one no other kept
  repository uses.
- Code: `src/github/` (sign-in, API, working copies, Send) and
  `src/app/github.rs` (the window). Tests run against a local mock of GitHub;
  `cargo test --lib github::mock::serve_github_mock -- --ignored` serves it for
  a manual run with `RESHIKI_GITHUB_CLIENT_ID=mock`,
  `RESHIKI_GITHUB_API=http://127.0.0.1:47123/api`,
  `RESHIKI_GITHUB_WEB=http://127.0.0.1:47123` and a fresh `RESHIKI_DATA_DIR`.
  It also serves `alice/ligands`, someone else's public repository with a
  `draft` branch, every other chemistry file and a `screening` folder of 240,
  and `alice/notes`, private; `octo-test/spectra` is too big to list whole.
  `POST /_mock/limit` with `{"left": 0}` spends the signed-out limit.
  `POST /_mock/commit` with
  `{"repo", "path", "content", "author": "alice", "message"}` changes a
  drawing as someone else.
