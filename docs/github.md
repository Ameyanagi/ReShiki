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
   scheme** go back too. Drawings show who changed them last and when, newest
   first.
4. Edit and save as usual. The status bar chip shows the repository and what
   is left: **● lab-notes · 1 unsent** with **Send**.
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

**GitHub → Versions** lists the saves on this computer since the last send
(**This computer**) and the commits on GitHub that changed the drawing
(**GitHub**), by day. Select one, then choose **Compare** to see it beside the
drawing now, **Open** to see it in a new tab or **Restore** to make it the
drawing again. Restore is one undo step and makes a new save. × or Escape
closes Versions.

The GitHub menu ends with your account: **@you**, **Add repositories… ↗** and
**Sign out**. The command palette offers the GitHub commands that apply to
the drawing in front, such as **Send 1 drawing**, **Versions**, **Copy GitHub
link** and **Open on GitHub ↗**.

## Limits

- Desktop only. The browser editor has no GitHub sign-in yet.
- ReShiki uses each repository's default branch, and only `.rsk` drawings and
  their `.svg` previews.
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
- Send reads the repository by id, builds one tree on the branch head and
  moves the branch only forward. If the head moved, it retries against the new
  head.
- The compare matches atoms, text, arrows and graphics by id (ids survive
  saving) and bonds by their two atoms, and compares each as saved. A bond
  whose atom moved counts as changed. Everything else in the file is compared
  as a whole for **Style or settings changed**. Atoms and bonds that differ
  get the persistent highlight paint in one diff colour, on copies made for
  the compare; text and graphics get a soft fill of that colour behind them.
- The picker lists one folder at a time. Its tiles render the drawing itself,
  the way an opened file renders; no preview file from the repository is
  parsed. Each drawing's last commit is one request; drawings show once all
  are known, so nothing moves.
- Repository colors and the repository last used are kept in
  `<data>/github/prefs.json`, by repository id. A new repository gets the hue
  its lowercase `owner/repo` hashes to (FNV-1a), or the next one no other kept
  repository uses.
- Code: `src/github/` (sign-in, API, working copies, Send) and
  `src/app/github.rs` (the window). Tests run against a local mock of GitHub;
  `cargo test --lib github::mock::serve_github_mock -- --ignored` serves it for
  a manual run with `RESHIKI_GITHUB_CLIENT_ID=mock`,
  `RESHIKI_GITHUB_API=http://127.0.0.1:47123/api`,
  `RESHIKI_GITHUB_WEB=http://127.0.0.1:47123` and a fresh `RESHIKI_DATA_DIR`.
