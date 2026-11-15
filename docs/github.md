# Keep drawings on GitHub

The desktop app keeps drawings in your GitHub repositories. **Save** keeps a
drawing on this computer, at once and offline. **Send to GitHub** makes one
commit of every changed drawing in that repository, each with an SVG preview
beside it, so drawings show on GitHub.

## Use it

1. Choose the **GitHub** button in the toolbar, then **GitHub account…** →
   **Sign in with GitHub**. ReShiki shows a code, copies it and opens GitHub.
   Paste the code there and allow ReShiki.
2. If ReShiki may not use any repository yet, choose **Choose repositories ↗**,
   pick them on GitHub, then **Check again**.
3. Choose **GitHub → Open from GitHub…**. Pick a repository, open folders, select
   a drawing and choose **Open**.
4. Edit and save as usual. The status bar shows **1 not on GitHub**.
5. Choose **Send** in the status bar. Change the message if you like, then
   choose **Send**. The status bar shows **On GitHub**.

To put a drawing that is not on GitHub yet into a repository, choose
**GitHub → Send to GitHub**. Pick a repository and folder, name the drawing,
then choose **Save here**. **New private repository…** makes a new private
repository for it.

Closing a drawing whose saves are not on GitHub asks **Send to GitHub** or
**Later**. Later keeps the saves on this computer; the status bar shows them
the next time you open the drawing.

If a drawing changed on GitHub since you opened it, Send leaves it out and
shows **GitHub’s** and **Yours** side by side. What differs is highlighted:
atoms, bonds, text and graphics. Choose one:

- **Keep GitHub’s**: your drawing becomes GitHub's. Undo brings yours back,
  and yours stays in Versions.
- **Keep mine**: yours goes to GitHub over theirs. Theirs stays in the
  repository's history.
- **Keep both**: yours is saved beside it as `<name> (2).rsk` and sent.

Escape decides later. The status bar shows **1 conflict** with **Review**.

**GitHub → Versions** lists the saves on this computer since the last send
(**On this Mac**, or **On this computer** on Windows and Linux) and the commits
on GitHub that changed the drawing (**On GitHub**). Select one, then choose
**Compare with current** to see it beside the drawing now, **Open** to see it
in a new tab or **Restore** to make it the drawing again. Restore is one undo
step and makes a new save.

The same commands are in the command palette: **Open from GitHub…**,
**Send to GitHub**, **Versions** and **GitHub account…**.

## Limits

- Desktop only. The browser editor has no GitHub sign-in yet.
- ReShiki uses each repository's default branch, and only `.rsk` drawings and
  their `.svg` previews.
- A change to the drawing's style or settings is not highlighted. The compare
  says **Style or settings changed** instead.
- Send sends the saved files. It saves the drawing in front first; drawings in
  other tabs are sent as last saved.
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
  as a whole for **Style or settings changed**. Changes are boxes drawn under
  the drawings, so a highlight colour shows as it is.
- The picker lists one folder at a time. Its tiles render the drawing itself,
  the way an opened file renders; no preview file from the repository is
  parsed.
- Code: `src/github/` (sign-in, API, working copies, Send) and
  `src/app/github.rs` (the window). Tests run against a local mock of GitHub;
  `cargo test --lib github::mock::serve_github_mock -- --ignored` serves it for
  a manual run with `RESHIKI_GITHUB_CLIENT_ID=mock`,
  `RESHIKI_GITHUB_API=http://127.0.0.1:47123/api`,
  `RESHIKI_GITHUB_WEB=http://127.0.0.1:47123` and a fresh `RESHIKI_DATA_DIR`.
