import { randomUUID, createHash } from "node:crypto";
import { spawn } from "node:child_process";
import { mkdir, readFile, writeFile, rename, lstat } from "node:fs/promises";
import path from "node:path";
import { LIMITS, validateEnvelope, decodeBase64 } from "../protocol.js";

export function sameTarget(a, b) {
  return JSON.stringify(a) === JSON.stringify(b);
}
export function validateTarget(target, revision) {
  if (
    !target ||
    Object.getPrototypeOf(target) !== Object.prototype ||
    !["Word", "Excel", "PowerPoint"].includes(target.host) ||
    typeof target.sessionId !== "string" ||
    !target.sessionId.length ||
    target.sessionId.length > 128 ||
    !/^[0-9a-f-]{36}$/i.test(target.objectId) ||
    target.revision !== revision
  )
    throw new Error("Invalid exact-object target");
  const required =
    target.host === "Word"
      ? ["contentControlId"]
      : target.host === "Excel"
        ? ["worksheetId", "shapeId"]
        : ["slideId", "shapeId"];
  if (
    !required.every(
      (key) =>
        ["number", "string"].includes(typeof target[key]) &&
        String(target[key]).length > 0 &&
        String(target[key]).length <= 512,
    ) ||
    JSON.stringify(target).length > 2048
  )
    throw new Error("Invalid Office object identity");
  return structuredClone(target);
}
async function atomicJson(file, value) {
  const temporary = `${file}.${randomUUID()}.tmp`;
  await writeFile(temporary, JSON.stringify(value), { mode: 0o600, flag: "wx", flush: true });
  await rename(temporary, file);
}
async function boundedRead(file, max) {
  const info = await lstat(file);
  if (!info.isFile() || info.isSymbolicLink() || info.size > max)
    throw new Error("Invalid or oversized recovery file");
  const bytes = await readFile(file);
  if (bytes.length > max) throw new Error("Oversized recovery file");
  return bytes;
}
export class Sessions {
  constructor({ recoveryDir, executable, worker, launch, now = Date.now }) {
    this.root = recoveryDir;
    this.executable = executable;
    this.worker = worker;
    this.now = now;
    this.sessions = new Map();
    this.clients = new Map();
    this.finished = new Map();
    this.transition = Promise.resolve();
    this.launch =
      launch ??
      ((file, onExit) =>
        new Promise((resolve, reject) => {
          const child = spawn(executable, ["--office-addin-edit", "--open", file], {
            detached: true,
            shell: false,
            stdio: "ignore",
            windowsHide: false,
          });
          child.once("exit", () => onExit());
          child.once("error", reject);
          child.once("spawn", () => {
            child.unref();
            resolve();
          });
        }));
  }
  serialized(operation) {
    const result = this.transition.then(operation);
    this.transition = result.catch(() => {});
    return result;
  }
  createClient(host) {
    if (!["Word", "Excel", "PowerPoint"].includes(host)) throw new Error("Unsupported Office host");
    if (this.clients.size >= 32)
      throw new Error("Too many task panes; restart the companion after closing unused panes");
    const id = randomUUID();
    this.clients.set(id, { host, seen: this.now() });
    return id;
  }
  touch(clientId) {
    const client = this.clients.get(clientId);
    if (!client || this.now() - client.seen > 30000)
      throw new Error(
        "Task pane session expired. Reopen the pane; saved drafts remain in recovery.",
      );
    client.seen = this.now();
    return client;
  }
  own(id, clientId) {
    this.touch(clientId);
    const session = this.sessions.get(id);
    if (!session || session.clientId !== clientId) throw new Error("Unknown edit session");
    return session;
  }
  start(clientId, target, supplied) {
    return this.serialized(() => this.startLocked(clientId, target, supplied));
  }
  async startLocked(clientId, target, supplied) {
    const client = this.touch(clientId),
      envelope = await validateEnvelope(supplied);
    target = validateTarget(target, envelope.revision);
    if (target.host !== client.host) throw new Error("Host mismatch");
    if (this.sessions.size >= 32)
      throw new Error("Too many edit sessions; restart the companion after finishing edits");
    if (
      [...this.sessions.values()].some(
        (s) => s.clientId === clientId && s.target.objectId === target.objectId,
      )
    )
      throw new Error("This drawing already has an open edit session");
    // Validate native semantics with the installed trusted worker before launching.
    const checked = await this.worker(
      "--libreoffice-preview",
      Buffer.from(decodeBase64(envelope.native, LIMITS.nativeBytes)),
    );
    if (checked.revision !== envelope.revision)
      throw new Error("Worker changed the native drawing");
    const id = randomUUID(),
      directory = path.join(this.root, id),
      file = path.join(directory, "drawing.rsk");
    await mkdir(directory, { recursive: true, mode: 0o700 });
    await writeFile(file, decodeBase64(envelope.native, LIMITS.nativeBytes), {
      mode: 0o600,
      flag: "wx",
      flush: true,
    });
    await atomicJson(path.join(directory, "session.json"), {
      version: 1,
      sessionId: id,
      target,
      created: new Date(this.now()).toISOString(),
    });
    const session = {
      id,
      clientId,
      target,
      directory,
      file,
      pending: null,
      handledRequest: null,
      error: null,
    };
    this.sessions.set(id, session);
    try {
      await this.launch(file, () => {
        this.nativeExited(id).catch((error) => {
          session.error = `Editor closed; edit session retained: ${error.message}`;
        });
      });
    } catch (error) {
      this.sessions.delete(id);
      throw new Error(`Could not launch ReShiki. Draft retained at ${file}: ${error.message}`);
    }
    return { sessionId: id, recoveryFile: file };
  }
  poll(id, clientId) {
    return this.serialized(() => this.pollLocked(id, clientId));
  }
  async pollLocked(id, clientId) {
    this.touch(clientId);
    const finished = this.finished.get(id);
    if (finished?.clientId === clientId)
      return { pending: null, finished: true, recoveryFile: finished.file };
    const session = this.own(id, clientId);
    if (session.pending) return { pending: session.pending, error: session.error };
    let request;
    try {
      request = JSON.parse(await boundedRead(path.join(session.directory, "request.json"), 4096));
    } catch (error) {
      if (error.code === "ENOENT") return { pending: null, error: session.error };
      throw error;
    }
    if (
      request.version !== 1 ||
      request.sessionId !== id ||
      typeof request.requestId !== "string" ||
      !/^[a-zA-Z0-9_-]{1,128}$/.test(request.requestId) ||
      !/^[a-f0-9]{64}$/.test(request.revision)
    )
      throw new Error("Invalid native save request");
    if (request.requestId === session.handledRequest)
      return { pending: null, error: session.error };
    const bytes = await boundedRead(session.file, LIMITS.nativeBytes);
    if (createHash("sha256").update(bytes).digest("hex") !== request.revision)
      return { pending: null, error: "Waiting for complete native save" };
    const envelope = await this.worker("--libreoffice-preview", bytes);
    if (envelope.revision !== request.revision) throw new Error("Worker revision mismatch");
    session.pending = {
      requestId: request.requestId,
      target: structuredClone(session.target),
      envelope,
    };
    session.error = null;
    return { pending: session.pending, error: null };
  }
  acknowledge(id, clientId, body) {
    return this.serialized(() => this.acknowledgeLocked(id, clientId, body));
  }
  async acknowledgeLocked(id, clientId, body) {
    this.touch(clientId);
    const finished = this.finished.get(id);
    if (finished?.clientId === clientId && finished.lastAck && sameTarget(finished.lastAck, body))
      return { accepted: true };
    const session = this.own(id, clientId),
      pending = session.pending;
    if (session.lastAck && JSON.stringify(session.lastAck) === JSON.stringify(body))
      return { accepted: true };
    if (
      !pending ||
      pending.requestId !== body.requestId ||
      !sameTarget(pending.target, body.previousTarget)
    )
      throw new Error("Stale Office acknowledgement");
    const target = validateTarget(body.target, pending.envelope.revision);
    if (
      target.host !== pending.target.host ||
      target.objectId !== pending.target.objectId ||
      target.sessionId !== pending.target.sessionId
    )
      throw new Error("Office acknowledgement targets another object");
    const stableKeys =
      target.host === "Word"
        ? ["contentControlId"]
        : target.host === "Excel"
          ? ["worksheetId"]
          : ["slideId", "shapeId"];
    if (!stableKeys.every((key) => target[key] === pending.target[key]))
      throw new Error("Office acknowledgement changed object identity");
    // The browser only calls this after the adapter read-back verifies native + PNG.
    await atomicJson(path.join(session.directory, "session.json"), {
      version: 1,
      sessionId: id,
      target,
      acceptedRevision: target.revision,
    });
    await atomicJson(path.join(session.directory, "ack.json"), {
      version: 1,
      sessionId: id,
      requestId: pending.requestId,
      revision: pending.envelope.revision,
      accepted: true,
    });
    session.target = target;
    session.handledRequest = pending.requestId;
    session.pending = null;
    session.error = null;
    session.lastAck = structuredClone(body);
    if (session.nativeExited) await this.retireIfReadyLocked(session);
    return { accepted: true };
  }
  fail(id, clientId, body) {
    return this.serialized(() => this.failLocked(id, clientId, body));
  }
  failLocked(id, clientId, body) {
    const session = this.own(id, clientId);
    if (session.pending?.requestId !== body.requestId)
      throw new Error("Stale failed-update report");
    session.error = String(body.error).slice(0, 1024);
    return { retained: true, recoveryFile: session.file };
  }
  finish(id, clientId) {
    return this.serialized(async () => {
      this.touch(clientId);
      const finished = this.finished.get(id);
      if (finished?.clientId === clientId) return { finished: true, recoveryFile: finished.file };
      const session = this.own(id, clientId);
      return this.retireLocked(session);
    });
  }
  nativeExited(id) {
    return this.serialized(async () => {
      const session = this.sessions.get(id);
      if (!session) return;
      session.nativeExited = true;
      return this.retireIfReadyLocked(session);
    });
  }
  async retireIfReadyLocked(session) {
    if (session.pending) return;
    try {
      const request = JSON.parse(
        await boundedRead(path.join(session.directory, "request.json"), 4096),
      );
      // A save might be on disk before the task pane has polled it. Preserve
      // authorization until that exact request is read back and acknowledged.
      if (request.requestId !== session.handledRequest) return;
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
    return this.retireLocked(session);
  }
  async retireLocked(session) {
    const { id, clientId } = session;
    await atomicJson(path.join(session.directory, "session.json"), {
      version: 1,
      sessionId: id,
      target: session.target,
      closed: true,
    });
    this.sessions.delete(id);
    this.finished.set(id, { clientId, file: session.file, lastAck: session.lastAck });
    if (this.finished.size > 128) this.finished.delete(this.finished.keys().next().value);
    return { finished: true, recoveryFile: session.file };
  }
}
