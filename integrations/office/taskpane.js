import { createHostAdapter } from "./host-adapters/index.js";
import { LIMITS, validateEnvelope } from "./protocol.js";
import { OperationGate, applyPending, acknowledgeApplied } from "./edit-flow.js";

const element = (id) => document.getElementById(id);
let token,
  clientId,
  adapter,
  stopped = false,
  editSupported = true,
  editReason = "";
const gate = new OperationGate();
const edits = new Map();
async function api(route, body = {}) {
  const response = await fetch(`/api/${route}`, {
    method: "POST",
    headers: { "Content-Type": "application/json", "X-ReShiki-Token": token },
    body: JSON.stringify({ ...body, clientId }),
    credentials: "omit",
    cache: "no-store",
  });
  const value = await response.json();
  if (!response.ok) throw new Error(value.error || `Companion returned ${response.status}`);
  return value;
}
function status(message) {
  element("status").textContent = message;
}
async function action(fn) {
  if (stopped) return;
  await gate.run(async () => {
    controls(true);
    try {
      await fn();
    } catch (error) {
      status(error.message);
    } finally {
      controls(stopped);
    }
  });
}
function controls(disabled) {
  document.querySelectorAll("button,input").forEach((node) => {
    node.disabled = disabled || (node.id === "edit" && !editSupported);
  });
}
function finishEdit(edit) {
  edits.delete(edit.sessionId);
  edit.node.textContent = `Edit finished. Select the drawing to reopen it.\nRecovery: ${edit.recoveryFile}`;
  edit.finish.remove();
}
async function selected() {
  const current = await adapter.readSelected();
  if (!current)
    throw new Error(
      "Select an editable ReShiki drawing. A plain image does not contain editable drawing data.",
    );
  current.envelope = await validateEnvelope(current.envelope);
  return current;
}
async function insert(envelope) {
  await adapter.insert(await validateEnvelope(envelope));
  status("Editable drawing inserted. Save your Office file to keep it.");
}
element("paste").addEventListener("click", () =>
  action(async () => insert((await api("clipboard/read")).envelope)),
);
element("copy").addEventListener("click", () =>
  action(async () => {
    const current = await selected();
    await api("clipboard/write", { envelope: current.envelope });
    status("Editable drawing copied. Use Paste editable drawing in the destination Office file.");
  }),
);
element("file").addEventListener("change", () =>
  action(async () => {
    const file = element("file").files[0];
    if (!file) return;
    try {
      if (file.size > LIMITS.nativeBytes)
        throw new Error("Drawing file exceeds the 16 MiB native limit");
      const bytes = new Uint8Array(await file.arrayBuffer());
      let binary = "";
      for (let i = 0; i < bytes.length; i += 8192)
        binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
      await insert((await api("preview", { native: btoa(binary) })).envelope);
    } finally {
      element("file").value = "";
    }
  }),
);
element("edit").addEventListener("click", () =>
  action(async () => {
    if (!editSupported) throw new Error(editReason || "Update Office before editing this drawing.");
    const current = await selected();
    const result = await api("edit", current);
    const row = document.createElement("div");
    row.className = "edit-status";
    const node = document.createElement("p");
    row.append(node);
    const finish = document.createElement("button");
    finish.textContent = "End edit session";
    row.append(finish);
    element("sessions").append(row);
    const edit = { ...result, node, finish, applied: null, blocked: false };
    edits.set(result.sessionId, edit);
    finish.addEventListener("click", () =>
      action(async () => {
        await api("finish", { sessionId: edit.sessionId });
        finishEdit(edit);
        status("Edit session finished. You can select and reopen the drawing.");
      }),
    );
    node.textContent = `Editing in ReShiki. Save there to update this drawing.\nRecovery: ${result.recoveryFile}`;
    status("ReShiki opened. Keep this pane open until the save is confirmed.");
  }),
);
async function poll() {
  if (stopped) return;
  try {
    await api("heartbeat");
    await gate.run(async () => {
      controls(true);
      try {
        for (const edit of edits.values()) {
          if (edit.blocked) continue;
          if (!edit.applied) {
            const { pending, finished } = await api("poll", { sessionId: edit.sessionId });
            if (finished) {
              finishEdit(edit);
              continue;
            }
            if (!pending) continue;
            try {
              await applyPending(edit, pending, adapter);
            } catch (error) {
              // Never retry a partially committed host operation automatically.
              edit.blocked = true;
              edit.node.textContent = `Update not confirmed: ${error.message}\nYour native draft is retained: ${edit.recoveryFile}\nReopen this pane and import the recovery file to recover it as a new drawing.`;
              await api("fail", {
                sessionId: edit.sessionId,
                requestId: pending.requestId,
                error: error.message,
              }).catch(() => {});
              continue;
            }
          }
          try {
            await acknowledgeApplied(edit, (applied) =>
              api("ack", { sessionId: edit.sessionId, ...applied }),
            );
            edit.node.textContent = `Drawing updated in Office. Save your Office file to keep it.\nRecovery: ${edit.recoveryFile}`;
          } catch (error) {
            edit.node.textContent = `Office read-back succeeded; retrying save acknowledgement: ${error.message}\nRecovery: ${edit.recoveryFile}`;
          }
        }
      } finally {
        controls(stopped);
      }
    });
  } catch (error) {
    status(
      `Companion connection interrupted: ${error.message}. Native drafts remain in the recovery folder.`,
    );
  } finally {
    if (!stopped) setTimeout(poll, 500);
  }
}
window.addEventListener("pagehide", () => {
  stopped = true;
});
try {
  await Office.onReady();
  adapter = createHostAdapter({
    Office,
    Word: window.Word,
    Excel: window.Excel,
    PowerPoint: window.PowerPoint,
  });
  const support = await adapter.checkSupport();
  if (support === false || support?.supported === false)
    throw new Error(
      support?.reason || "This Office version does not support editable ReShiki drawings.",
    );
  editSupported = support?.canEdit !== false;
  editReason = support?.editReason || "";
  token = (
    await (
      await fetch("/bootstrap.json", {
        headers: { "X-ReShiki-Bootstrap": "1" },
        cache: "no-store",
        credentials: "omit",
      })
    ).json()
  ).token;
  clientId = (await api("client", { host: adapter.host })).clientId;
  element("support").textContent = editSupported
    ? `Connected to ${adapter.host}`
    : `Connected to ${adapter.host}. ${support.editReason || "This Office version supports insertion but must be updated before editing drawings."}`;
  element("actions").hidden = false;
  element("excel-note").hidden = adapter.host !== "Excel";
  controls(false);
  poll();
} catch (error) {
  stopped = true;
  element("support").textContent = error.message;
}
