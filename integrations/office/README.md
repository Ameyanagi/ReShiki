# ReShiki for Microsoft 365 desktop

This optional Office task pane stores an editable native ReShiki drawing and a PNG preview together in Word, Excel, and PowerPoint files on macOS and Windows. It uses a local HTTPS companion and an installed ReShiki executable. It is an MVP for sideloaded testing, not a published Marketplace add-in. Host mocks and protocol tests do not establish a real Office round trip: complete the acceptance table below on each target host before claiming support for that combination.

## Editing and saving

1. Start the companion, open the add-in in your Office file, and keep the pane open.
2. Draw in ReShiki and use Copy, then **Paste editable drawing** in the Office pane. Alternatively, insert a saved `.rsk` drawing file.
3. Select the inserted object and choose **Edit selected drawing**. ReShiki opens its native draft in a separate window.
4. Save in ReShiki. The companion prepares the new preview; the pane updates the exact ReShiki drawing and reads back its stored drawing. Only a matching acknowledgement lets ReShiki report that Office was updated. Selection changes do not redirect an edit. Excel replaces the physical picture shape, changing its host shape ID while preserving the logical ReShiki drawing and supported placement.
5. Save the Word, Excel, or PowerPoint document (or verify that AutoSave completed) to persist the embedded drawing in the Office file.
6. Close the ReShiki editor. Once its last save is acknowledged, observing the native editor process exit automatically releases that drawing for another edit. If an interrupted edit stays listed, **End edit session** explicitly detaches it. Ending a session never deletes its draft; a late save from an ended editor cannot update a newly opened session.

Double-click activation is not part of this shared task-pane workflow. Existing Windows OLE drawings keep their existing double-click/native-save path. A Windows OLE object is not automatically a Mac-editable task-pane object. Convert by opening its native drawing in ReShiki, copying it, and using the pane's **Paste editable drawing**.

Use the pane's **Copy editable drawing** and **Paste editable drawing** for transfer between Office documents/apps. Ordinary image copying may transfer only the preview. An image without native payload is reported as noneditable, never reconstructed as molecular atoms. Same-document duplicates become independent logical drawings when edited; document-owned payload versions remain available for copies and Office undo.

## Requirements

- Node.js 22 or later, no npm runtime dependencies.
- A ReShiki build containing `--office-addin-edit` and the bounded native preview/clipboard workers.
- WordApi 1.4, ExcelApi 1.19, or PowerPointApi 1.8, checked at runtime. Editing existing Excel pictures additionally requires ExcelApiDesktop 1.1 (Microsoft 365 2509 on Windows or 16.102 on Mac); older Excel 1.19 hosts can insert and read but cannot update. Each app has its own manifest to avoid requiring another app's API set.
- A trusted TLS certificate with `localhost` in its subject alternative names. The private key stays on this computer. Never share a generated private key or commit it to this repository.
- Desktop Office on macOS or Windows. Office on the web, mobile, legacy Office builds, arbitrary cross-document clipboard fidelity, and double-click hooks have not been implemented or validated by this MVP.

The task pane loads Microsoft's hosted Office.js library; native drawing bytes go only to the loopback companion and the current Office document. This does not bypass Office's own save or cloud synchronization behavior.

In Excel, keep these as plain pictures. User-added hyperlinks, assigned macros, shadows, and other picture effects are unsupported because the API cannot inspect and preserve every such association during picture replacement. Detectable unsupported crop/effect/connector states are rejected before mutation. Word similarly rejects picture transformations it cannot safely preserve. See the host-adapter documentation for the exact supported placement and transformation boundaries.

## Prepare and run

Use your organization's trusted localhost development certificate or generate one using your reviewed development-certificate tooling. Microsoft's [Office Add-ins tooling](https://github.com/OfficeDev/Office-Addin-Scripts/tree/master/packages/office-addin-dev-certs) provides development certificates. Certificate generation/trust installation and Office sideloading are separate, explicit machine configuration steps. `setup.js` does neither.

From this directory, run:

```sh
node setup.js --executable /absolute/path/to/reshiki --cert /absolute/path/to/localhost.crt --key /absolute/path/to/localhost.key --directory /absolute/path/to/office-local
node companion/server.js --config /absolute/path/to/office-local/config.json
```

Use quoted absolute Windows paths on Windows. For a macOS app bundle, the executable is typically `ReShiki.app/Contents/MacOS/reshiki`. Choose the exact installed executable yourself; paths are never accepted from an Office document. The default port is 43127. If you change `--port`, use the generated manifests for the same port.

Setup validates certificate/key matching, hostname, validity, and executable presence, then exclusively creates a config and three manifests. It refuses to overwrite an existing config. The server binds only `127.0.0.1`; confirm that `https://localhost:43127/taskpane.html` has no certificate error before sideloading. A regular browser reports that Office is unavailable; the pane itself runs inside Office.

For macOS, follow Microsoft's [Mac sideload instructions](https://learn.microsoft.com/en-us/office/dev/add-ins/testing/sideload-an-office-add-in-on-mac): copy only the relevant generated manifest into that app's `wef` directory and restart that Office application. For Windows, use Microsoft's [development sideload tooling](https://github.com/OfficeDev/Office-Addin-Scripts/tree/master/packages/office-addin-debugging) or the documented [shared-folder catalog](https://learn.microsoft.com/en-us/office/dev/add-ins/testing/create-a-network-shared-folder-catalog-for-task-pane-and-content-add-ins) testing route. Do not treat a trusted folder catalog as production deployment. Managed environments may require the administrator's approved deployment procedure.

## Recovery and limits

Each native edit gets a private UUID directory below the configured recovery directory. `drawing.rsk` is the durable draft; `session.json`, `request.json`, and `ack.json` identify its target and exact save. Native Save waits up to 20 seconds for the matching acknowledgement. Missing pane, stale/deleted target, wrong revision, host errors, oversized data, or a closed Office document leave the native editor dirty and retain the draft. Save As can preserve a separate `.rsk` file.

Closing a task pane or restarting the companion abandons its live authorization; it never silently reconnects an old edit to a different Office document. Reopen the pane and explicitly insert the recovery `.rsk` file as a new object. Do not delete recovery files until the Office document has been saved and its drawing verified. The companion does not delete drafts automatically and does not override Office's close behavior. Inspect the recovery directory periodically; document-version payloads and retained drafts increase storage usage.

Limits are 16 MiB native drawing, 8 MiB PNG, 32 megapixels, 32 task panes/sessions per companion process, 15 seconds per worker, and 30 seconds without a pane heartbeat. These are local ReShiki safeguards, not Microsoft limits. API requests require exact localhost Host and Origin, a runtime token, a per-pane capability, JSON content type, and bounded bodies. No permissive CORS, document-supplied commands, or arbitrary filesystem routes are exposed. Keep the config, key, and recovery directory private to your operating-system account (Windows ACLs, or Unix modes 0700/0600).

## Verification

```sh
node --test
```

The automated suite covers envelope integrity, bounded worker errors, origin/token isolation, session/object/revision matching, concurrent requests, failed-save draft retention, and lost-ACK retries. Native Rust receipt tests cover missing, stale, wrong, and matching acknowledgements. Adapter tests exercise host-specific copy and update contracts with mocks.

Record the exact Office build, OS, ReShiki commit, manifest, and evidence for every real-host run:

| Acceptance case                                                              | Word Mac/Windows | Excel Mac/Windows | PowerPoint Mac/Windows |
| ---------------------------------------------------------------------------- | ---------------- | ----------------- | ---------------------- |
| Insert → native edit → confirmed Save                                        | Pending          | Pending           | Pending                |
| Save Office → close → reopen → edit same native drawing                      | Pending          | Pending           | Pending                |
| Duplicate object → edit one → other stays unchanged                          | Pending          | Pending           | Pending                |
| Move/resize/rotate (where host supports it) → edit preserves placement       | Pending          | Pending           | Pending                |
| Native edit open → select another object → Save updates original             | Pending          | Pending           | Pending                |
| Delete target / close pane / close Office → draft recovery, no false success | Pending          | Pending           | Pending                |
| Pane Copy/Paste across apps and across Mac/Windows Office files              | Pending          | Pending           | Pending                |
| Protected document or failed host mutation → retained recovery               | Pending          | Pending           | Pending                |
