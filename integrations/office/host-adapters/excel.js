import {
  assertCanAdd,
  assertGeometry,
  assertReadback,
  assertRevision,
  encodeRecord,
  fail,
  findRecord,
  intrinsicSize,
  makeResult,
  newRecord,
  OBJECT_PREFIX,
  objectId,
  owner,
  readRecords,
  sameEnvelope,
  sameOwner,
  scaledSize,
  writeFailure,
  updatePreflight,
} from "./common.js";

const MARKER =
  /(?:\n)?\[ReShiki data:([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\]$/;
const SHAPE_PROPERTIES =
  "id,name,type,level,left,top,width,height,rotation,lockAspectRatio,placement,visible,zOrderPosition,altTextTitle,altTextDescription";
const GEOMETRY_FIELDS = [
  "left",
  "top",
  "width",
  "height",
  "rotation",
  "lockAspectRatio",
  "placement",
  "visible",
  "altTextTitle",
];
const IMAGE_PROPERTIES = "cropLeft,cropTop,cropRight,cropBottom,brightness,contrast,colorType";
const SHAPE_INSPECTION_LIMIT = 10000;

function descriptionWithMarker(description, recordId) {
  return `${description.replace(MARKER, "")}\n[ReShiki data:${recordId}]`;
}

async function shapeAt(context, identity, allowMissing = false) {
  const sheet = context.workbook.worksheets.getItemOrNullObject(identity.worksheetId);
  sheet.load("id");
  await context.sync();
  if (sheet.isNullObject)
    fail("TARGET_DELETED", "The worksheet containing this ReShiki drawing has been deleted.");
  const shape = sheet.shapes.getItemOrNullObject(identity.shapeId);
  shape.load(SHAPE_PROPERTIES);
  await context.sync();
  if (shape.isNullObject) {
    if (allowMissing) return { sheet, shape: null };
    fail(
      "TARGET_DELETED",
      "The Excel drawing that was opened for editing has been deleted or moved into a group.",
    );
  }
  return { sheet, shape };
}

async function readShape(context, identity, { allowCopy = false, allowUnowned = false } = {}) {
  const { sheet, shape } = await shapeAt(context, identity);
  const match = MARKER.exec(shape.altTextDescription);
  if (!match) {
    if (
      allowUnowned &&
      !shape.name.startsWith(OBJECT_PREFIX) &&
      !shape.altTextDescription.includes("[ReShiki data:")
    )
      return null;
    fail(
      "MISSING_PAYLOAD",
      "This Excel drawing has lost its ReShiki data identifier. No image-only edit will be attempted.",
    );
  }
  const identityNow = owner("Excel", { worksheetId: sheet.id, shapeId: shape.id });
  const records = await readRecords(context, context.workbook.customXmlParts);
  const record = findRecord(records, match[1], identityNow, allowCopy);
  shape.load(SHAPE_PROPERTIES);
  await context.sync();
  if (MARKER.exec(shape.altTextDescription)?.[1] !== record.recordId)
    fail("REVISION_CONFLICT", "The Excel drawing changed while its editable data was being read.");
  return { sheet, shape, record, records, identity: identityNow, geometry: loadedGeometry(shape) };
}

function loadedGeometry(shape) {
  if (shape.type !== "Image" || shape.level !== 0)
    fail("UNSUPPORTED_CONTAINER", "Ungroup this ReShiki image before opening it for editing.");
  const geometry = Object.fromEntries(
    [...GEOMETRY_FIELDS, "name", "zOrderPosition"].map((key) => [key, shape[key]]),
  );
  geometry.description = shape.altTextDescription.replace(MARKER, "");
  if (![geometry.width, geometry.height].every((value) => Number.isFinite(value) && value > 0))
    fail("INVALID_GEOMETRY", "Excel returned invalid drawing dimensions.");
  return geometry;
}

async function assertNoAttachedConnectors(context, sheet, shapeId) {
  let collections = [sheet.shapes];
  let inspected = 0;
  const lines = [];
  for (let depth = 0; collections.length; depth++) {
    if (depth > 32)
      fail(
        "UNSUPPORTED_CONTAINER",
        "The worksheet has too many nested shape groups to safely inspect connector attachments.",
      );
    for (const collection of collections) collection.load("items/id,items/type");
    await context.sync();
    const next = [];
    for (const collection of collections) {
      inspected += collection.items.length;
      if (inspected > SHAPE_INSPECTION_LIMIT)
        fail(
          "UNSUPPORTED_CONTAINER",
          "The worksheet has too many shapes to safely inspect connector attachments.",
        );
      for (const shape of collection.items) {
        if (shape.type === "Line") lines.push(shape.line);
        else if (shape.type === "Group") next.push(shape.group.shapes);
      }
    }
    collections = next;
  }
  if (!lines.length) return;
  for (const line of lines) line.load("isBeginConnected,isEndConnected");
  await context.sync();
  const endpoints = [];
  for (const line of lines) {
    if (line.isBeginConnected) endpoints.push(line.beginConnectedShape);
    if (line.isEndConnected) endpoints.push(line.endConnectedShape);
  }
  if (!endpoints.length) return;
  for (const endpoint of endpoints) endpoint.load("id");
  await context.sync();
  if (endpoints.some((endpoint) => endpoint.id === shapeId))
    fail(
      "ATTACHED_CONNECTOR",
      "An Excel connector is attached to this drawing. Detach it before editing with ReShiki so replacement does not break the connection.",
    );
}

async function checkEditablePicture(context, state, Office) {
  if (!Office.context.requirements.isSetSupported("ExcelApiDesktop", "1.1")) {
    fail(
      "UNSUPPORTED_HOST",
      "Safe Excel image replacement requires ExcelApiDesktop 1.1 (Microsoft 365 2509 on Windows or 16.102 on Mac). Update Excel before editing an existing drawing.",
    );
  }
  await assertNoAttachedConnectors(context, state.sheet, state.identity.shapeId);
  state.shape.image.load(IMAGE_PROPERTIES);
  state.shape.lineFormat.load("visible");
  state.shape.load(SHAPE_PROPERTIES);
  await context.sync();
  if (MARKER.exec(state.shape.altTextDescription)?.[1] !== state.record.recordId)
    fail("REVISION_CONFLICT", "The Excel drawing changed during the final edit check.");
  state.geometry = loadedGeometry(state.shape);
  const image = state.shape.image;
  const cropped = ["cropLeft", "cropTop", "cropRight", "cropBottom"].some(
    (key) => !Number.isFinite(image[key]) || Math.abs(image[key]) > 0.001,
  );
  if (
    cropped ||
    image.colorType !== "Automatic" ||
    Math.abs(image.brightness - 0.5) > 0.0001 ||
    Math.abs(image.contrast - 0.5) > 0.0001 ||
    state.shape.lineFormat.visible
  ) {
    fail(
      "UNSUPPORTED_TRANSFORM",
      "This Excel picture has cropping, color adjustments or an outline that cannot be safely preserved. Reset those picture effects before editing with ReShiki.",
    );
  }
}

function applyGeometry(
  shape,
  geometry,
  recordId,
  { name = geometry.name, visible = geometry.visible } = {},
) {
  shape.lockAspectRatio = false;
  for (const key of ["width", "height", "rotation", "left", "top", "placement", "altTextTitle"])
    shape[key] = geometry[key];
  shape.lockAspectRatio = geometry.lockAspectRatio;
  shape.name = name;
  shape.visible = visible;
  shape.altTextDescription = descriptionWithMarker(geometry.description, recordId);
  shape.lineFormat.visible = false;
}

function assertReplacement(state, record, geometry, overrides = {}) {
  assertReadback(state.record, record);
  const expected = { ...geometry, ...overrides };
  assertGeometry(state.geometry, expected, [
    ...GEOMETRY_FIELDS,
    "name",
    "description",
    ...(Object.hasOwn(overrides, "zOrderPosition") ? ["zOrderPosition"] : []),
  ]);
}

export function createExcelAdapter(Excel, { sessionId, Office }) {
  return {
    async insert(envelope) {
      return Excel.run(async (context) => {
        const records = await readRecords(context, context.workbook.customXmlParts);
        const record = newRecord(envelope);
        assertCanAdd(records, record);
        const sheet = context.workbook.worksheets.getActiveWorksheet();
        sheet.load("id");
        await context.sync();
        let identity;
        try {
          const shape = sheet.shapes.addImage(envelope.png);
          const geometry = {
            ...intrinsicSize(envelope),
            left: 36,
            top: 36,
            rotation: 0,
            lockAspectRatio: true,
            placement: "Absolute",
            visible: true,
            name: OBJECT_PREFIX + record.objectId,
            altTextTitle: "ReShiki chemical structure",
            description: "Editable chemical drawing. Select it and choose Edit in ReShiki.",
          };
          applyGeometry(shape, geometry, record.recordId);
          const part = context.workbook.customXmlParts.add(encodeRecord(record));
          shape.load("id");
          await context.sync();
          identity = owner("Excel", { worksheetId: sheet.id, shapeId: shape.id });
          record.owner = identity;
          part.setXml(encodeRecord(record));
          await context.sync();
          const written = await readShape(context, identity);
          assertReplacement(written, record, geometry);
          return makeResult(written.record, sessionId);
        } catch (error) {
          let recoveryError;
          try {
            if (!identity)
              fail(
                "UNKNOWN_INSERTION",
                "Office did not return the newly inserted shape identifier.",
              );
            const current = await shapeAt(context, identity);
            if (MARKER.exec(current.shape.altTextDescription)?.[1] !== record.recordId)
              fail("TARGET_CHANGED", "Another edit changed the incomplete insertion.");
            current.shape.delete();
            await context.sync();
          } catch (recovery) {
            recoveryError = recovery;
          }
          throw writeFailure(error, recoveryError, { operation: "insert", ...identity });
        }
      });
    },

    async readSelected() {
      return Excel.run(async (context) => {
        const shape = context.workbook.getActiveShapeOrNullObject();
        const sheet = context.workbook.worksheets.getActiveWorksheet();
        shape.load("id");
        sheet.load("id");
        await context.sync();
        if (shape.isNullObject) return null;
        const identity = owner("Excel", { worksheetId: sheet.id, shapeId: shape.id });
        let state = await readShape(context, identity, { allowCopy: true, allowUnowned: true });
        if (!state) return null;
        if (!sameOwner(state.record.owner, identity)) {
          const record = newRecord(state.record.envelope, identity);
          assertCanAdd(state.records, record);
          context.workbook.customXmlParts.add(encodeRecord(record));
          await context.sync();
          const fresh = await readShape(context, identity, { allowCopy: true });
          if (fresh.record.recordId !== state.record.recordId)
            fail("TARGET_CHANGED", "The copied drawing changed while it was being opened.");
          fresh.shape.altTextDescription = descriptionWithMarker(
            fresh.geometry.description,
            record.recordId,
          );
          await context.sync();
          state = await readShape(context, identity);
          assertReadback(state.record, record);
        }
        return makeResult(state.record, sessionId);
      });
    },

    async read(target) {
      return Excel.run(async (context) => {
        const state = await readShape(context, target);
        assertRevision(state.record, target);
        return makeResult(state.record, sessionId);
      });
    },

    async update(target, envelope) {
      return Excel.run(async (context) => {
        let { before, attempted } = await updatePreflight(async () => {
          const before = await readShape(context, target);
          assertRevision(before.record, target);
          await checkEditablePicture(context, before, Office);
          const attempted = newRecord(envelope, null, before.record.objectId);
          assertCanAdd(before.records, attempted);
          return { before, attempted };
        });
        const stagedName = `ReShiki_pending_${objectId()}`;
        const oldName = `ReShiki_previous_${objectId()}`;
        let replacementIdentity;
        let expectedGeometry;
        let deletionStarted = false;
        let originalRenamed = false;
        try {
          const shape = before.sheet.shapes.addImage(envelope.png);
          const stagedGeometry = {
            ...before.geometry,
            ...scaledSize(before.geometry, before.record.envelope, envelope),
          };
          applyGeometry(shape, stagedGeometry, attempted.recordId, {
            name: stagedName,
            visible: false,
          });
          shape.load("id");
          const part = context.workbook.customXmlParts.add(encodeRecord(attempted));
          await context.sync();
          replacementIdentity = owner("Excel", {
            worksheetId: before.identity.worksheetId,
            shapeId: shape.id,
          });
          attempted.owner = replacementIdentity;
          part.setXml(encodeRecord(attempted));
          await context.sync();
          const staged = await readShape(context, replacementIdentity);
          assertReplacement(staged, attempted, stagedGeometry, {
            name: stagedName,
            visible: false,
          });

          // Use the latest placement and user scale; never the selection after editing began.
          const fresh = await readShape(context, target);
          assertRevision(fresh.record, target);
          if (fresh.record.recordId !== before.record.recordId)
            fail(
              "REVISION_CONFLICT",
              "The drawing received another update while this edit was being applied.",
            );
          before = fresh;
          await checkEditablePicture(context, before, Office);
          expectedGeometry = {
            ...before.geometry,
            ...scaledSize(before.geometry, before.record.envelope, envelope),
          };
          originalRenamed = true;
          before.shape.name = oldName;
          applyGeometry(staged.shape, expectedGeometry, attempted.recordId);
          // Put the replacement immediately above the original. Removing the
          // original then leaves the replacement at the original stack index.
          staged.shape.setZOrder("SendToBack");
          for (let index = 0; index <= before.geometry.zOrderPosition; index++)
            staged.shape.setZOrder("BringForward");
          await context.sync();
          const ready = await readShape(context, replacementIdentity);
          assertReplacement(ready, attempted, expectedGeometry, {
            zOrderPosition: before.geometry.zOrderPosition + 1,
          });
          const original = await readShape(context, target);
          assertRevision(original.record, target);
          await checkEditablePicture(context, original, Office);
          if (
            original.record.recordId !== before.record.recordId ||
            original.shape.name !== oldName
          )
            fail("REVISION_CONFLICT", "The original drawing changed before replacement completed.");
          assertGeometry(original.geometry, before.geometry, GEOMETRY_FIELDS);
          // No original image or payload is deleted before complete readback.
          deletionStarted = true;
          original.shape.delete();
          await context.sync();
          const written = await readShape(context, replacementIdentity);
          assertReplacement(written, attempted, expectedGeometry, {
            zOrderPosition: before.geometry.zOrderPosition,
          });
          return makeResult(written.record, sessionId);
        } catch (error) {
          let recoveryError;
          try {
            const existing = await shapeAt(context, target, true);
            if (!existing.shape) {
              // A sync may fail after Excel applied the deletion. A full exact
              // readback can still establish success without repeating the write.
              if (deletionStarted && replacementIdentity && expectedGeometry) {
                const written = await readShape(context, replacementIdentity);
                assertReplacement(written, attempted, expectedGeometry, {
                  zOrderPosition: before.geometry.zOrderPosition,
                });
                return makeResult(written.record, sessionId);
              }
              fail(
                "TARGET_DELETED",
                "The original drawing is absent; automatic rollback cannot preserve its identity.",
              );
            }
            const original = await readShape(context, target);
            assertReadback(original.record, before.record);
            if (
              originalRenamed &&
              original.shape.name !== oldName &&
              original.shape.name !== before.geometry.name
            ) {
              fail(
                "REVISION_CONFLICT",
                "Another edit renamed the original; it was left untouched.",
              );
            }
            let stagedShape;
            if (replacementIdentity) {
              const staged = await shapeAt(context, replacementIdentity, true);
              if (staged.shape) {
                if (MARKER.exec(staged.shape.altTextDescription)?.[1] !== attempted.recordId)
                  fail(
                    "REVISION_CONFLICT",
                    "Another edit changed the replacement; it was left untouched.",
                  );
                const records = await readRecords(context, context.workbook.customXmlParts);
                const candidates = records.filter(
                  (record) => record.recordId === attempted.recordId,
                );
                if (
                  candidates.length !== 1 ||
                  candidates[0].objectId !== attempted.objectId ||
                  !sameEnvelope(candidates[0].envelope, attempted.envelope) ||
                  (candidates[0].owner && !sameOwner(candidates[0].owner, attempted.owner))
                ) {
                  fail(
                    "REVISION_CONFLICT",
                    "Another edit changed the replacement payload; it was left untouched.",
                  );
                }
                stagedShape = staged.shape;
              }
            } else {
              fail("UNKNOWN_INSERTION", "Office did not return the replacement shape identifier.");
            }
            original.shape.load("name,altTextDescription");
            if (stagedShape) stagedShape.load("altTextDescription");
            await context.sync();
            if (
              MARKER.exec(original.shape.altTextDescription)?.[1] !== before.record.recordId ||
              (originalRenamed &&
                original.shape.name !== oldName &&
                original.shape.name !== before.geometry.name) ||
              (stagedShape &&
                MARKER.exec(stagedShape.altTextDescription)?.[1] !== attempted.recordId)
            ) {
              fail(
                "REVISION_CONFLICT",
                "Another edit changed the drawing during rollback checks; it was left untouched.",
              );
            }
            // All rollback guards pass before queuing any cleanup mutation.
            if (stagedShape) stagedShape.delete();
            if (originalRenamed && original.shape.name === oldName) {
              original.shape.name = before.geometry.name;
            }
            await context.sync();
            const restored = await readShape(context, target);
            assertReadback(restored.record, before.record);
            assertGeometry(restored.geometry, before.geometry, [
              ...GEOMETRY_FIELDS,
              "name",
              "description",
              "zOrderPosition",
            ]);
          } catch (recovery) {
            recoveryError = recovery;
          }
          throw writeFailure(error, recoveryError, {
            operation: "update",
            target,
            replacementIdentity,
          });
        }
      });
    },
  };
}
