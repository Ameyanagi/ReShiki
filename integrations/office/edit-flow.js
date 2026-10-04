import { validateEnvelope } from "./protocol.js";
import { sameEnvelope } from "./host-adapters/common.js";

// Acquire before the first await so a click and a polling response cannot own
// Office.run concurrently. The pane disables its controls for the same period.
export class OperationGate {
  busy = false;
  async run(operation) {
    if (this.busy) return false;
    this.busy = true;
    try {
      await operation();
      return true;
    } finally {
      this.busy = false;
    }
  }
}
export async function applyPending(edit, pending, adapter) {
  if (edit.applied) return;
  const updated = await adapter.update(pending.target, await validateEnvelope(pending.envelope));
  const readback = await adapter.read(updated.target);
  const actual = readback && (await validateEnvelope(readback.envelope));
  if (!actual || !sameEnvelope(actual, await validateEnvelope(pending.envelope)))
    throw new Error("Office did not read back the expected drawing and preview");
  edit.applied = {
    requestId: pending.requestId,
    previousTarget: pending.target,
    target: readback.target,
  };
}
export async function acknowledgeApplied(edit, acknowledge) {
  if (!edit.applied) return;
  await acknowledge(edit.applied);
  edit.applied = null;
}
