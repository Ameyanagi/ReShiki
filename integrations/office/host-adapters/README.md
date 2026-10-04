# ReShiki Office host adapters

These ESM modules persist an editable native ReShiki payload beside an Office preview. They use the shared `../protocol.js` envelope without parsing or rebuilding native drawing bytes. No installation, sideloading, certificate trust, Office UI automation or deployment is performed here.

## Contract

```js
const adapter = createHostAdapter({ Office, Word, Excel, PowerPoint });
const support = await adapter.checkSupport();
const inserted = await adapter.insert(envelope);
const selected = await adapter.readSelected(); // null only for an unowned selection
const current = await adapter.read(selected.target);
const updated = await adapter.update(current.target, editedEnvelope);
```

`checkSupport()` throws on an unsupported host/API floor. On success it returns `{ host, requirement, version, canEdit, editReason? }`. Excel can insert/read with ExcelApi 1.19 but returns `canEdit: false` without ExcelApiDesktop 1.1; the pane must disable native editing in that case. `update()` also enforces this edit-only requirement before any host mutation.

Every successful object operation returns `{ target, envelope }`. The target contains `host`, `objectId`, native SHA-256 `revision`, a factory `sessionId`, and exact Office identity: `contentControlId` for Word, `worksheetId`/`shapeId` for Excel, or `slideId`/`shapeId` for PowerPoint. `read` and `update` use this captured identity, never the user's later selection. Retain the returned target after an update: Excel replaces the physical shape ID while retaining the logical ReShiki object ID. A task-pane reload requires selecting the drawing again to obtain a new session token.

## Host carriers and API floors

| Host       | Carrier and preview update                                                                                                                                | Required runtime APIs                                                                                                                      |
| ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Word       | Dedicated rich-text content control around one inline picture; document custom XML; picture contents replaced within the same control                     | WordApi 1.4; Microsoft 365 Windows 2208 / Mac 16.64 or later                                                                               |
| Excel      | Image shape; short record locator in alt description; workbook custom XML; staged new image replaces old image only after complete data/geometry readback | ExcelApi 1.19 for selection/insertion (Windows 2504 / Mac 16.96); additionally ExcelApiDesktop 1.1 for editing (Windows 2509 / Mac 16.102) |
| PowerPoint | Image-filled rectangle; shape tag; shape-scoped custom XML; image fill changed on the same shape                                                          | PowerPointApi 1.8; Windows 2504 / Mac 16.96 or later                                                                                       |

Runtime `isSetSupported` checks are authoritative. Build tables do not establish that a particular installed host behaves correctly. PowerPoint uses stable `ShapeFill.setImage`, not the preview-only `addPicture` API. References: [Word API requirements](https://learn.microsoft.com/en-us/javascript/api/requirement-sets/word/word-api-requirement-sets), [Excel API requirements](https://learn.microsoft.com/en-us/javascript/api/requirement-sets/excel/excel-api-requirement-sets), [Excel desktop requirements](https://learn.microsoft.com/en-us/javascript/api/requirement-sets/excel/excel-api-desktop-1-1-requirement-set), [PowerPoint requirements](https://learn.microsoft.com/en-us/javascript/api/requirement-sets/powerpoint/powerpoint-api-requirement-sets), [PowerPoint image fill](https://learn.microsoft.com/en-us/javascript/api/powerpoint/powerpoint.shapefill).

## Persistence and identity

Custom XML namespace `urn:reshiki:office-object:1` contains the complete validated envelope: opaque native bytes, PNG, native extent in hundredths of a millimetre, and native SHA-256 revision. Each version has a fresh `record-id`; updates retain the logical `object-id`. The control/tag/alt-description locator references an immutable record. The owner identifies the exact Office object that created that record.

This versioning matters for copies: a copy made before an original is edited continues to reference its earlier payload. When a selected copy has a different Office owner, `readSelected()` forks it to a new logical object and record before returning. Old records remain available for copies and undo. Automatic garbage collection is deliberately absent. A failed Word/PowerPoint update removes only its newly staged record when no preview/marker write was queued, the record still matches exactly, and no current control/shape references it. Removal must be confirmed before another save can retry. Once a preview write begins, that record is retained even after rollback because undo may still reference it. Ordinary clipboard transfer across documents/apps is not promised to preserve custom XML; use the application's explicit Copy Editable/Paste Editable path when the payload does not accompany the preview.

ReShiki imposes 256 records and 64 MiB XML per storage collection (Word document, Excel workbook, or PowerPoint shape). New records reserve owner-metadata space and fail before changing the preview at the cap. XML is fetched and validated one part at a time; the aggregate is checked before fetching the next part. A single host `getXml()` response cannot be size-bounded before it arrives. Individual envelope limits come from `protocol.js` (16 MiB native / 8 MiB PNG). These are application limits, not claims about Microsoft's storage capacity.

## Geometry and supported objects

Native extents convert to Office points with `extent * 72 / 2540`. An edit preserves the current independent width/height scale by multiplying the latest displayed dimensions by `newExtent / oldExtent`. This handles 2×, 3× and unequal-axis scaling even when native content bounds change on a later edit.

- Word preserves the content-control identity, inline placement, picture dimensions, aspect-ratio lock, alt text and hyperlink. Additional text/nested controls, floating pictures, table/row/cell controls, locked controls, detected crops/rotation/flips and recognized picture effects are rejected. Inserting another drawing while inside an existing ReShiki control is rejected to prevent nesting or damaging it.
- Excel supports **plain ReShiki image shapes**. Replacement preserves position, dimensions, rotation, aspect-ratio lock, name, human alt text, visibility, cell placement and stacking order. Grouped targets, detectable crops/color adjustments/outlines and attached connectors (including connectors inside groups) are rejected. Connector inspection stops at 10,000 shapes or over 32 nested group levels. Current stable APIs do not expose all shape-associated features: externally attached hyperlinks, assigned macros, shadows/other effects, flips and other features outside the fields listed above are unsupported and may not be detectable. Do not use this replacement path for such decorated/augmented pictures; physical shape identity changes on every successful edit.
- PowerPoint updates the fill on the existing ungrouped geometric shape; identity, position, rotation and stack membership remain on that shape. Dimensions scale with native extent. A changed fill type or grouped target is rejected. General preservation of manually added fill-cropping/tiling and other picture effects is not established by these adapters.

Word's OOXML preflight resolves the main document part through its Flat OPC package relationship, then locates the content control by its exact ID and tag. Only that control's single inline picture is checked for transforms; theme defaults and other drawings are outside the check. Malformed XML, DTDs and ambiguous identities or picture containers are rejected before replacement. The webview's native `DOMParser` performs XML parsing.

## Acknowledgement and recovery

The adapters validate input, preserve complete native payloads before replacing a preview, recheck exact object markers after multi-batch reads, and read back the complete stored envelope and supported geometry before returning success. Word/PowerPoint try to restore the previous preview and locator if a partially applied edit fails. Excel retains the original image until the staged replacement has passed readback; it deletes the original as the final replacement mutation. A failed final-delete sync is treated as success only if a subsequent complete readback proves the intended replacement exists in the expected state.

Rollback checks the exact old/attempted record and owner and refuses to overwrite a newer coauthor record. Office.js does not supply a transaction or compare-and-swap primitive for these operations; the checks do not eliminate the final read/write race. `RECOVERY_REQUIRED` means success was not acknowledged and automatic recovery could not establish a safe original state. Keep the document and native editor open for explicit recovery; do not retry against an unrelated selection. Failed staging can leave unreferenced XML records, which remain counted toward the cap.

A successful adapter result means Office returned the written document state. It does not mean the document was saved to disk. The user/Office must still save the DOCX/XLSX/PPTX, and closed-document/native-host reopen tests remain necessary. See Microsoft's [persisting add-in state guidance](https://learn.microsoft.com/en-us/office/dev/add-ins/develop/persisting-add-in-state-and-settings).

## Verification

Run from the repository root:

```sh
bun install --frozen-lockfile
node --test integrations/office/host-adapters/tests/*.test.js
```

The tests use queued Office doubles with explicit `load`/`sync`, enum validation, partial sync failures and concurrent marker changes. They cover exact opaque native-byte roundtrips, new pane sessions, captured targets, copied-object independence, repeated anisotropic scaling, safe rollback, connector/crop/group rejection and size bounds. They do not establish native Office rendering, custom-XML survival on save/close/reopen, native clipboard behavior, undo transaction boundaries, coauthor atomicity or cross-OS interoperability.

Node tests use the pinned development-only `@xmldom/xmldom` parser with strict error handling. The Word theme regression fixture is reduced Flat OPC reconstructed from a saved Mac Word DOCX; its provenance file records the source and retained parts. It is not a captured live `getOoxml()` response or evidence of a successful native edit.

Before claiming desktop acceptance, test Word, Excel and PowerPoint on both Mac and Windows: insert a real chemistry fixture; resize uniformly and by unequal axes; save/close the Office file; reopen and edit twice; copy and edit independently; then perform Windows → Mac → Windows file roundtrips. Verify native hashes and chemistry content, preview changes and displayed scale. Include an unsupported crop/connector case and an induced replacement failure. Those actual-host acceptance results are pending.
