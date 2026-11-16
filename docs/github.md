# Keep drawings on GitHub

The desktop app keeps drawings in your GitHub repositories. **Save** keeps a
drawing on this computer, at once and offline. **Send** makes one commit of
every changed drawing in that repository, each with an SVG preview beside it,
so drawings show on GitHub.

## Use it

1. Choose the **GitHub** button in the toolbar, then **Open from GitHub…**.
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
   scheme** go back too. Drawings show who changed them last and when (**You**
   for your own changes), newest first. With a repository open, the search
   field filters its files.
4. Edit and save as usual. The status bar chip shows the repository, the
   folder and what is left: **● lab-notes / scheme · 1 unsent** with **Send**.
   Click the name to open the picker in that folder.
5. Choose **Send**. A small box above the chip says what goes where, such as
   **ring.rsk → lab-notes**, with the message selected: type over it or keep
   it, then press Enter. ReShiki first saves every open drawing of that
   repository, as **Save** does. The chip shows **● lab-notes ✓**.

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

A folder shows its first 200 files, newest first, and says how many there
are; the search field finds the others.

### Tiles or Tree

The two buttons at the end of the folder path switch between **Tiles** and
**Tree**. The picker opens in the view you used last, in every repository.
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
ReShiki shows **GitHub** and **Yours** side by side. What differs is painted
with ReShiki's highlight on both. Choose:

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
drawing (**GitHub**), by day. It shows only for drawings on GitHub. Select
one, then choose **Compare** to see it beside the drawing now, **Open** to see
it in a new tab or **Restore** to make it the drawing again. Restore is one
undo step and makes a new save.

The GitHub button shows your initial while you are signed in; its tooltip
says **Signed in as @you** or **Not signed in**. Its menu ends with your
account: **@you**, **Add repositories… ↗** and **Sign out**. The command
palette offers the GitHub commands that apply to the drawing in front, such as
**Send 1 drawing**, **History**, **Show in Finder**, **Copy GitHub link** and
**Open on GitHub ↗**;
typing a whole word such as `send`, `history`, `versions` or `repo` puts them
first.

## Limits

- Desktop only. The browser editor has no GitHub sign-in yet.
- ReShiki sends to each repository's default branch, and only `.rsk` drawings
  and their `.svg` previews. Other branches and commits, and other chemistry
  files, open read only.
- Newest first is as far as known: only files on screen are asked for their
  last change, so in a folder of more than 200 the rest follow by name.
- A repository too big for GitHub to list whole shows part of it in the tree,
  with **Only part of this repository is listed**; **+ New** rows are then
  left out. Tiles list each folder in full.
- A link names a branch by its first folder: `tree/feature/x/...` reads the
  branch `feature`.
- Only `.webloc` and `.url` files carry a dropped link; a link dragged from a
  browser's address bar is not received.
- A change to the drawing's style or settings is not painted. The compare
  says **Style or settings changed** instead.
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
- On window focus, at most every five minutes and only while GitHub drawings
  are open, ReShiki asks each open repository's branch head for the open
  drawings' blobs: the repositories you may send to whether a drawing changed
  since its base, read-only ones whether you may send to them now. A changed
  drawing costs one more request for its last commit and one for its bytes.
- Send reads the repository by id, builds one tree on the branch head and
  moves the branch only forward. If the head moved, it retries against the new
  head.
- The compare matches atoms, text, arrows and graphics by id (ids survive
  saving) and bonds by their two atoms, and compares each as saved. A bond
  whose atom moved counts as changed. Everything else in the file is compared
  as a whole for **Style or settings changed**. Atoms and bonds that differ
  get the persistent highlight paint in one diff colour, on copies made for
  the compare; text and graphics get a soft fill of that colour behind them.
- Tiles list one folder at a time, at most 200 matching the search. They
  render the file itself, the way an opened file renders (an SD file's first
  record); no preview file from the repository is parsed. Each file's last
  commit and preview are one request each, made only once its tile is on
  screen. The first screenful shows once all of its dates are known and sorts
  newest first, once; tiles scrolled to later never move the others.
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
