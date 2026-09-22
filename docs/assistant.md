# Drawing with the assistant

Open **Assistant** in the toolbar and describe a molecule or chemical scheme. ReShiki uses your existing local Codex sign-in. The model menu lists the models available to your account; model, reasoning, service tier and Review/Accept-all preferences are saved. If needed, run `codex login` and reconnect. `RESHIKI_CODEX` can point to a specific executable.

## Activity and drafts

Sending a request immediately shows **Preparing your scheme…**, an elapsed timer and **Stop** in the conversation. A short composition outline appears before structures are prepared. Structure counts report completed local preparation work. Rendered drafts appear as panels become available, followed by the visual review stage. Quiet responses keep the timer and a waiting message visible; there are no estimated completion percentages.

The footer keeps the elapsed timer visible when you scroll through a large preview. The canvas and prompt remain usable throughout generation, local chemistry preparation and image review. Closing the panel lets the job continue; **Assistant · Working** remains in the toolbar. Draft previews stay in the conversation and do not repeatedly change the drawing. The preview caches its rendered geometry so typing, dragging and the activity timer do not rebuild it.

**Stop** cancels the job and retains any completed preview. Choose a panel, molecule, caption or arrow below the preview to inspect it at a readable size or move it; arrows can also be shortened. Editing an intermediate preview stops generation and retains that version. Edited or interrupted drafts require review. **Improve layout** checks the current draft again. With no pending draft, **Improve selected layout** or **Improve drawing layout** reviews existing objects without regenerating their chemistry. A selected reaction includes its complete chemical participants; connected reactions sharing a structure are reviewed together.

Cmd/Ctrl+Enter sends; plain Enter inserts a line break. **New conversation** cancels the job and clears its chat and previews. Chats and unapplied drafts are held in memory for the session. Saved drawing files contain applied objects.

## Composition

The assistant can request these arrangements:

- **Reaction rows:** aligned arrows and measured gaps between complete rows.
- **Central reaction:** a general reaction in the middle, with separately labeled examples above and below.
- **Labeled grid:** related reaction panels arranged in columns that fit the intended width.
- **Branching:** one shared central reactant, with separate arrows and products in several directions. Each branch retains its own reaction membership, coefficients and conditions. Directions may be specified or distributed automatically. This supports outward branches from a common reactant; arbitrary reaction networks are not yet a proposal format.

For example: “Draw the general saponification reaction in the center, with Triolein and Tributyrin examples around it. Use English names and compact long chains.” Or: “Put ethanol in the center, with oxidation to the right and dehydration upward; label each arrow with its conditions.”

The model specifies roles and relationships. ReShiki measures full molecular and caption bounds, calculates positions, aligns compound captions on a shared baseline within each reaction row and uses the active physical bond length and drawing style. Small global rotation offsets from molecular coordinates are removed before layout. Assistant generation and visual corrections allow 30-degree rotation steps, including the 30- or 60-degree changes needed to make conventional carbonyl groups upright. Rotations preserve the complete graph, stereochemistry, bond lengths and internal geometry. Irregular ring geometry is retained. Figure width defaults to 540 points unless specified in the request. An oversized figure is reported for correction instead of shrinking every molecule’s bond length to fit.

Long terminal carbon chains can use compact formula labels while retaining all underlying atoms, bonds and stereochemistry. The normal **Expand** command restores their original structures. Request full chain or stereochemical detail when it must remain visible; the proposal’s detail constraint disables automatic compaction. Mapped wildcard atoms appear as editable R-group labels. Identical repeated disconnected SMILES are normalized to one structure with a coefficient.

## Required image review

Every generated drawing is rendered for a separate review turn before automatic application. The reviewer receives the original request, editable object data, an overview and reaction close-ups. Close-ups retain the exact atom labels and chemical data shown in the overview. Rendering uses drawing data, never a desktop screenshot.

The reviewer checks spacing, alignment, captions, coefficients, large structures, clipped labels and possible chemistry issues. It can propose bounded editable changes: move an object or reaction panel, rotate a whole molecule, adjust arrow length, compact a chain or recompose panels. These operations cannot change atoms, bonds, reaction membership, coefficients or caption text. Chemistry problems are reported for correction instead of being disguised by a layout edit.

Local checks validate molecular graphs, detect overlaps and distant captions, check figure width, and compare element/charge totals using explicit reaction participants and coefficients. Consumed reagents should be included as reactants. Reagents written only as conditions may explain a balance warning, but are not assumed to be encoded participants. These checks do not establish that a reaction mechanism, identity or stereochemical assignment is correct.

At most two correction rounds are allowed, followed by a final verification when needed. Every changed draft is rendered again. Changes that increase the number of local problems are rejected. The exact final image must be reviewed before a draft qualifies for automatic application. Review failures, remaining problems and exhausted correction limits stay visible with the retained preview.

## Applying and undoing

**Review edits** is the initial mode. **Apply** inserts the editable proposal below existing content, or replaces its targeted objects. **Discard** removes only the draft.

**Accept all edits** applies a proposal automatically only after its exact final version passes visual review with no unresolved issues. Turning this preference on does not bypass warnings on an existing draft. A draft with warnings can still be inspected and applied explicitly. Every application is one Undo step; Redo restores it.

Additions use the current drawing, preserving edits made during generation. Replacements require the original drawing revision and cannot target objects outside an explicitly selected replacement scope. Switching documents invalidates either kind of result. Application waits for an active text edit or attachment operation to finish. Stopped jobs and late results cannot overwrite newer work.

## Data and limits

The request, conversation, prior proposal and drawing context are shared with Codex. The assistant can inspect the current canvas and prepare optional early previews. The required review also receives overview and close-up images plus editable drawing data. Images are temporary, bounded in pixel dimensions, and scoped to the drawing. Tools cannot control other apps or execute commands. Ordinary drawing and local chemistry do not require a connection.

Proposals support up to 32 molecules, eight reaction steps, 300 atoms per molecule and 1,500 atoms overall. Generation supports forward, equilibrium and retrosynthesis arrows. Explicit reaction roles remain available through **Properties → Reaction roles…** and reaction exchange; see [reaction workflow and exchange limits](reactions.md).

The integration follows the official [Codex app-server protocol](https://learn.chatgpt.com/docs/app-server), using structured output, public progress summaries, bounded drawing tools and local image inputs. `cargo run --example assistant_smoke -- --generate --prompt '…' --output artifacts/assistant-qa` exercises generation and mandatory review with the local sign-in. `--render drawing.reshiki --output artifacts/assistant-qa/render` renders an existing drawing and its close-ups without a model request.

## Model and navigation defaults

The assistant defaults to GPT-6 Sol (`gpt-6-sol`) when it is available in the connected Codex catalog, otherwise to the account default. Explicit model choices remain saved. Reasoning and service options come from the selected model’s catalog entry.

Opening model, effort or edit-mode menus preserves the conversation position. Closing and reopening the panel restores that position. Use **Jump to result** to return to a completed draft after reading earlier messages.
