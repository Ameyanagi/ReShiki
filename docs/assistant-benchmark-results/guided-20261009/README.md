# Native guided ethanol control · 9 October 2026

This is a separate first-use control on #93 commit
`43e3d80dde9c9f1da1be04e2525182aea390cc75`, using the fresh debug macOS arm64
bundle. It is **excluded from the unassisted baseline denominator**. The local
reference is visible to the user in this guided workflow.
The parent setup change is under review in [PR #280](https://github.com/Ameyanagi/ReShiki/pull/280).

One Send used explicitly selected GPT-6.1-Sol / Extra high. The connection was
ready, the example attached locally, and the canvas remained blank until manual
Apply even though the ordinary Auto apply preference was enabled. The completed
Assistant draft showed Apply and the visible review summary: the two-carbon
chain, two single bonds, terminal OH and no title matched the source; no visible
correction was requested. That model review statement is separate from graph
verification.

The exact [native save](drawing.rsk) independently scores as `CCO`, three heavy
atoms, two single bonds, neutral and one fragment, with no atom/hydrogen/bond/
stereo/fragment/abbreviation errors. See [score.json](score.json). One Undo
removed the whole graph and Redo restored it. Native SaveAs, clean quit and actual
UI reopening of the exact saved graph were observed. Reopened Properties show
`C2H6O`, three atoms, two bonds and canonical `CCO`; filename is clean and Undo is
disabled. The native file and independent score remain unchanged. Reopening used
250%, keyboard drawing off, JACS / ACS and Arial 10 pt; it closed without Save or
inference. The recovery banner was dismissed in memory only, with no file deletion.

![Exact saved guided ethanol reopened with formula, atom/bond counts and canonical CCO in Properties](../../images/assistant-setup/assistant-guided-reopened.jpg)

| Completed draft before manual Apply                                                                                                  | Saved graph restored by Redo                                                                                            |
| ------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| ![Completed ethanol draft with Apply available while canvas is blank](../../images/assistant-setup/assistant-draft-before-apply.jpg) | ![Saved native ethanol restored by Redo on the drawing canvas](../../images/assistant-setup/assistant-redo-desktop.jpg) |

Capture conditions differ deliberately: before Apply the blank canvas was at 100%
with F8 keyboard drawing on; after Apply F8 was off and the app fitted the graph
at 250%. The Redo image is unselected. These are exact JPEG captures; no image was
retouched. [receipt.json](receipt.json) pins native/evidence hashes and actions.

Send began at `2026-10-09T09:30:52.198Z`. Completion was observed within 181.712s;
the app's integer elapsed display was 62s for generation/review. The observation
interval includes polling delay and is an upper bound, **not an exact model
latency**. The single control stayed within its 300s allocation. Baseline plus
control used 14 of 16 allowed workflows; their bounded running intervals total at
most 1681.27s of 1800s, excluding idle time between the experiments. No extra request
or model fallback was used. Service tier was not recorded in these desktop
observations and is not inferred from the baseline.

The parent's [native setup review](../../changes/assistant-setup-review-2026-10-09.md)
also includes actual macOS arm64 UI checks with clearly labeled account-free
simulated sign-out, installation, broken/stalled handshake and cancellation
backends. The real account, installation and configuration stayed intact; no Send
or inference occurred in those fixture processes. Other-platform GUI remains
unverified. This successful known guided example does not increase the baseline's
six completed exact graphs or establish accuracy on unseen images.
