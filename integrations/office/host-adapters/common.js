import { createObjectId, LIMITS, validateEnvelope } from "../protocol.js";

export const XML_NAMESPACE = "urn:reshiki:office-object:1";
export const OBJECT_PREFIX = "ReShiki_";
export const WORD_TAG_PREFIX = "reshiki:";
export const PPT_OBJECT_TAG = "RESHIKI_OBJECT";
// ReShiki document limits, not Microsoft limits. Immutable records are retained
// because copied objects and Office undo can still reference older versions.
export const STORAGE_LIMITS = Object.freeze({ records: 256, xmlBytes: 64 * 1024 * 1024 });
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const REVISION = /^[0-9a-f]{64}$/;
const IDENTITY_KEYS = {
  Word: ["contentControlId"],
  Excel: ["worksheetId", "shapeId"],
  PowerPoint: ["slideId", "shapeId"],
};

export class HostAdapterError extends Error {
  constructor(code, message, details = {}, cause) {
    super(message, cause === undefined ? undefined : { cause });
    this.name = "HostAdapterError";
    this.code = code;
    this.details = details;
  }
}

export function fail(code, message, details) {
  throw new HostAdapterError(code, message, details);
}

export function objectId() {
  const id = createObjectId();
  if (!UUID.test(id)) fail("INVALID_OBJECT_ID", "Could not create a ReShiki object identifier.");
  return id;
}

export function newRecord(envelope, identity = null, logicalId = objectId()) {
  return { objectId: logicalId, recordId: objectId(), owner: identity, envelope };
}

export function markerId(value, prefix = OBJECT_PREFIX) {
  if (typeof value !== "string" || !value.startsWith(prefix)) return null;
  const id = value.slice(prefix.length, prefix.length + 36);
  return UUID.test(id) ? id : null;
}

export function owner(host, identity) {
  const keys = IDENTITY_KEYS[host];
  if (!keys) fail("INVALID_TARGET", "Unknown Office host.");
  const result = { host };
  for (const key of keys) {
    const value = identity[key];
    if (key === "contentControlId") {
      if (!Number.isSafeInteger(value) || value < 0)
        fail("INVALID_TARGET", "Invalid Word content-control identifier.");
    } else if (typeof value !== "string" || !value || value.length > 512) {
      fail("INVALID_TARGET", `Invalid Office ${key}.`);
    }
    result[key] = value;
  }
  return result;
}

export function sameOwner(a, b) {
  return (
    !!a && !!b && a.host === b.host && IDENTITY_KEYS[a.host]?.every((key) => a[key] === b[key])
  );
}

export function validateToken(token, host, sessionId) {
  if (
    !token ||
    typeof token !== "object" ||
    token.host !== host ||
    token.sessionId !== sessionId ||
    !UUID.test(token.objectId) ||
    !REVISION.test(token.revision)
  ) {
    fail(
      "INVALID_TARGET",
      "This edit belongs to another document or task-pane session. Select the drawing again.",
    );
  }
  return { ...owner(host, token), objectId: token.objectId, revision: token.revision, sessionId };
}

export function makeResult(record, sessionId) {
  if (!record.owner)
    fail(
      "INCOMPLETE_OBJECT",
      "This ReShiki object has an incomplete host update. Recover it before editing.",
    );
  return {
    target: {
      ...record.owner,
      objectId: record.objectId,
      revision: record.envelope.revision,
      sessionId,
    },
    envelope: { ...record.envelope, extent: [...record.envelope.extent] },
  };
}

function encodeOwner(value) {
  const bytes = new TextEncoder().encode(JSON.stringify(value));
  return btoa(Array.from(bytes, (byte) => String.fromCharCode(byte)).join(""));
}

function decodeOwner(value) {
  if (
    !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value) ||
    value.length > 4096
  ) {
    fail("INVALID_PAYLOAD", "The embedded ReShiki object identifier is invalid.");
  }
  try {
    const parsed = JSON.parse(
      new TextDecoder("utf-8", { fatal: true }).decode(
        Uint8Array.from(atob(value), (c) => c.charCodeAt(0)),
      ),
    );
    return parsed === null ? null : owner(parsed.host, parsed);
  } catch (error) {
    throw new HostAdapterError(
      "INVALID_PAYLOAD",
      "The embedded ReShiki object identifier is invalid.",
      {},
      error,
    );
  }
}

// All variable XML text is ASCII base64, UUIDs, numeric extents or hexadecimal hashes.
// No general XML entity expansion or document-provided code is evaluated.
export function encodeRecord(record) {
  const { envelope, objectId: id } = record;
  if (!UUID.test(id) || !UUID.test(record.recordId))
    fail("INVALID_PAYLOAD", "Invalid ReShiki object identifier.");
  return `<reshiki-object xmlns="${XML_NAMESPACE}" object-id="${id}" record-id="${record.recordId}" version="1" revision="${envelope.revision}" width="${envelope.extent[0]}" height="${envelope.extent[1]}" owner="${encodeOwner(record.owner)}"><native>${envelope.native}</native><png>${envelope.png}</png></reshiki-object>`;
}

export async function decodeRecord(xml) {
  const maxLength =
    Math.ceil(LIMITS.nativeBytes / 3) * 4 + Math.ceil(LIMITS.pngBytes / 3) * 4 + 8192;
  if (typeof xml !== "string" || xml.length > maxLength)
    fail("INVALID_PAYLOAD", "The embedded ReShiki object exceeds the supported payload size.");
  // Accept the XML declaration and namespace prefixes a host serializer may add.
  const match =
    /^\s*(?:<\?xml\s+[^?]*\?>\s*)?<((?:[A-Za-z_][\w.-]*:)?reshiki-object)\s+([^<>]+)>\s*<((?:[A-Za-z_][\w.-]*:)?native)>\s*([A-Za-z0-9+/=]*)\s*<\/\3>\s*<((?:[A-Za-z_][\w.-]*:)?png)>\s*([A-Za-z0-9+/=]*)\s*<\/\5>\s*<\/\1>\s*$/.exec(
      xml,
    );
  if (!match)
    fail("INVALID_PAYLOAD", "The embedded ReShiki XML is damaged or uses an unsupported format.");
  const attributes = {};
  let remaining = match[2];
  const attributePattern = /^\s*([A-Za-z_][\w:.-]*)\s*=\s*(["'])([^"'<>]*)\2/;
  while (remaining.trim()) {
    const attribute = attributePattern.exec(remaining);
    if (!attribute || Object.hasOwn(attributes, attribute[1]))
      fail("INVALID_PAYLOAD", "The embedded ReShiki XML attributes are invalid.");
    attributes[attribute[1]] = attribute[3];
    remaining = remaining.slice(attribute[0].length);
  }
  const prefix = match[1].includes(":") ? match[1].split(":")[0] : "";
  const namespaceKey = prefix ? `xmlns:${prefix}` : "xmlns";
  const required = [
    namespaceKey,
    "object-id",
    "record-id",
    "version",
    "revision",
    "width",
    "height",
    "owner",
  ];
  if (
    Object.keys(attributes).length !== required.length ||
    required.some((key) => !Object.hasOwn(attributes, key)) ||
    attributes[namespaceKey] !== XML_NAMESPACE ||
    attributes.version !== "1" ||
    !UUID.test(attributes["object-id"]) ||
    !UUID.test(attributes["record-id"]) ||
    !/^[1-9][0-9]*$/.test(attributes.width) ||
    !/^[1-9][0-9]*$/.test(attributes.height)
  ) {
    fail("INVALID_PAYLOAD", "The embedded ReShiki XML schema is unsupported.");
  }
  const expectedNativeName = prefix ? `${prefix}:native` : "native";
  const expectedPngName = prefix ? `${prefix}:png` : "png";
  if (match[3] !== expectedNativeName || match[5] !== expectedPngName)
    fail("INVALID_PAYLOAD", "The embedded ReShiki XML namespaces do not match.");
  const envelope = await validateEnvelope({
    version: 1,
    native: match[4],
    png: match[6],
    extent: [Number(attributes.width), Number(attributes.height)],
    revision: attributes.revision,
  });
  return {
    objectId: attributes["object-id"],
    recordId: attributes["record-id"],
    owner: decodeOwner(attributes.owner),
    envelope,
  };
}

export async function readRecords(context, collection) {
  const parts = collection.getByNamespace(XML_NAMESPACE);
  parts.load("items/id");
  await context.sync();
  if (parts.items.length > STORAGE_LIMITS.records)
    fail(
      "STORAGE_LIMIT",
      "This document contains too many ReShiki data records. Use a new document or explicit editable export; no records were removed.",
    );
  const records = [];
  let total = 0;
  // Office cannot report a part's size before getXml. Read one bounded record
  // at a time so a hostile document cannot make us fetch all parts up front.
  for (const part of parts.items) {
    const xml = part.getXml();
    await context.sync();
    if (typeof xml.value !== "string")
      fail("INVALID_PAYLOAD", "Office returned invalid embedded ReShiki XML.");
    total += xml.value.length;
    if (total > STORAGE_LIMITS.xmlBytes)
      fail(
        "STORAGE_LIMIT",
        "The embedded ReShiki data exceeds the document storage limit. No records were removed.",
      );
    records.push({ ...(await decodeRecord(xml.value)), part, xml: xml.value });
  }
  return records;
}

export function assertCanAdd(records, record) {
  if (
    records.length + 1 > STORAGE_LIMITS.records ||
    records.reduce((sum, item) => sum + item.xml.length, 0) + encodeRecord(record).length + 8192 >
      STORAGE_LIMITS.xmlBytes
  ) {
    fail(
      "STORAGE_LIMIT",
      "This document has reached ReShiki's embedded-data limit. Use a new document or explicit editable export. Existing records are retained for copies and undo.",
    );
  }
}

export function findRecord(records, id, identity, allowCopy = false) {
  const candidates = records.filter((record) => record.recordId === id);
  const exact = candidates.filter((record) => sameOwner(record.owner, identity));
  if (exact.length === 1) return exact[0];
  if (exact.length > 1 || (allowCopy && candidates.length > 1))
    fail(
      "AMBIGUOUS_OBJECT",
      "This drawing has conflicting embedded ReShiki data. No object was changed.",
    );
  if (allowCopy && candidates.length === 1 && candidates[0].owner) return candidates[0];
  if (candidates.some((record) => !record.owner))
    fail(
      "INCOMPLETE_OBJECT",
      "An earlier ReShiki insertion did not finish. No image-only edit will be attempted.",
    );
  fail(
    "MISSING_PAYLOAD",
    "This drawing has no matching editable ReShiki data. Use Copy Editable and Paste Editable to transfer it.",
  );
}

export function assertRevision(record, target) {
  if (!sameOwner(record.owner, target) || record.objectId !== target.objectId)
    fail(
      "TARGET_CHANGED",
      "The original ReShiki drawing no longer matches this edit. Select it again.",
    );
  if (record.envelope.revision !== target.revision)
    fail(
      "REVISION_CONFLICT",
      "This ReShiki drawing changed after editing began. Reopen it before applying your changes.",
    );
}

export function sameEnvelope(a, b) {
  return (
    a.version === b.version &&
    a.native === b.native &&
    a.png === b.png &&
    a.revision === b.revision &&
    a.extent[0] === b.extent[0] &&
    a.extent[1] === b.extent[1]
  );
}

export function assertReadback(record, expected) {
  if (
    record.objectId !== expected.objectId ||
    record.recordId !== expected.recordId ||
    !sameOwner(record.owner, expected.owner) ||
    !sameEnvelope(record.envelope, expected.envelope)
  ) {
    fail(
      "READBACK_FAILED",
      "Office did not return the complete ReShiki data that was written. The edit has not been acknowledged.",
    );
  }
}

export function intrinsicSize(envelope) {
  return { width: (envelope.extent[0] * 72) / 2540, height: (envelope.extent[1] * 72) / 2540 };
}

export function scaledSize(geometry, before, after) {
  const result = {
    width: (geometry.width * after.extent[0]) / before.extent[0],
    height: (geometry.height * after.extent[1]) / before.extent[1],
  };
  if (![result.width, result.height].every((n) => Number.isFinite(n) && n > 0))
    fail("INVALID_GEOMETRY", "The drawing dimensions cannot be represented by Office.");
  return result;
}

export function assertGeometry(actual, expected, fields = ["width", "height"]) {
  for (const key of fields) {
    if (typeof expected[key] === "number") {
      if (!Number.isFinite(actual[key]) || Math.abs(actual[key] - expected[key]) > 0.1)
        fail("READBACK_FAILED", `Office did not preserve the drawing's ${key}.`);
    } else if (actual[key] !== expected[key])
      fail("READBACK_FAILED", `Office did not preserve the drawing's ${key}.`);
  }
}

export function mayRollback(record, before, attempted) {
  return (
    record.objectId === before.objectId &&
    sameOwner(record.owner, before.owner) &&
    ((record.recordId === before.recordId && sameEnvelope(record.envelope, before.envelope)) ||
      (record.recordId === attempted.recordId && sameEnvelope(record.envelope, attempted.envelope)))
  );
}

export function writeFailure(cause, recoveryError, details = {}) {
  return new HostAdapterError(
    recoveryError ? "RECOVERY_REQUIRED" : "HOST_WRITE_FAILED",
    recoveryError
      ? "Office could not complete or safely roll back this edit. The edit was not acknowledged. Keep the document open and recover the ReShiki object before retrying."
      : "Office could not complete this edit. The original object was restored; the edit was not acknowledged.",
    {
      ...details,
      recoveryRequired: !!recoveryError,
      ...(recoveryError ? { recoveryError: String(recoveryError.message || recoveryError) } : {}),
    },
    cause,
  );
}
