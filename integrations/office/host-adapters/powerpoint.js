import {
  assertCanAdd,
  assertCurrentRecord,
  assertGeometry,
  assertReadback,
  assertRevision,
  discardUnpublishedRecord,
  encodeRecord,
  fail,
  findRecord,
  intrinsicSize,
  makeResult,
  markerId,
  mayRollback,
  newRecord,
  OBJECT_PREFIX,
  owner,
  PPT_OBJECT_TAG,
  readRecords,
  sameOwner,
  scaledSize,
  writeFailure,
  updatePreflight,
  retryableUpdateFailure,
} from "./common.js";

async function shapeAt(context, identity) {
  const slides = context.presentation.slides;
  const slide = slides.getItemOrNullObject(identity.slideId);
  slide.load("id");
  await context.sync();
  if (slide.isNullObject)
    fail("TARGET_DELETED", "The slide containing this ReShiki drawing has been deleted.");
  const shape = slide.shapes.getItemOrNullObject(identity.shapeId);
  shape.load("id,name,type,left,top,width,height,level");
  await context.sync();
  if (shape.isNullObject)
    fail(
      "TARGET_DELETED",
      "The PowerPoint drawing that was opened for editing has been deleted or moved into a group.",
    );
  return { slide, shape };
}

async function readShape(
  context,
  identity,
  { allowCopy = false, allowUnowned = false, verifyPreview = true } = {},
) {
  const { slide, shape } = await shapeAt(context, identity);
  shape.tags.load("items/key,items/value");
  shape.fill.load("type");
  await context.sync();
  const tag = shape.tags.items.find((item) => item.key.toUpperCase() === PPT_OBJECT_TAG);
  if (!tag) {
    if (allowUnowned && !shape.name.startsWith(OBJECT_PREFIX)) return null;
    fail(
      "MISSING_PAYLOAD",
      "This PowerPoint drawing has lost its ReShiki identifier. No image-only edit will be attempted.",
    );
  }
  const recordId = markerId(String(tag.value).toLowerCase(), "");
  if (!recordId || recordId !== String(tag.value).toLowerCase())
    fail("INVALID_PAYLOAD", "This PowerPoint drawing has an invalid ReShiki identifier.");
  const records = await readRecords(context, shape.customXmlParts);
  const currentOwner = owner("PowerPoint", { slideId: slide.id, shapeId: shape.id });
  const record = findRecord(records, recordId, currentOwner, allowCopy);
  shape.load("id,name,type,left,top,width,height,level");
  shape.tags.load("items/key,items/value");
  shape.fill.load("type");
  await context.sync();
  if (
    String(
      shape.tags.items.find((item) => item.key.toUpperCase() === PPT_OBJECT_TAG)?.value,
    ).toLowerCase() !== recordId
  ) {
    fail(
      "REVISION_CONFLICT",
      "The PowerPoint drawing changed while its editable data was being read.",
    );
  }
  if (
    verifyPreview &&
    (shape.type !== "GeometricShape" ||
      shape.fill.type !== "PictureAndTexture" ||
      shape.level !== 0)
  ) {
    fail(
      "UNSUPPORTED_CONTAINER",
      "Edit an ungrouped ReShiki image-filled shape. This object was grouped or its image fill was changed.",
    );
  }
  const geometry = { left: shape.left, top: shape.top, width: shape.width, height: shape.height };
  if (![geometry.width, geometry.height].every((value) => Number.isFinite(value) && value > 0))
    fail("INVALID_GEOMETRY", "PowerPoint returned invalid drawing dimensions.");
  return { slide, shape, record, identity: currentOwner, geometry, records };
}

function applyPreview(shape, record, geometry) {
  shape.fill.setImage(record.envelope.png);
  shape.width = geometry.width;
  shape.height = geometry.height;
  shape.tags.add(PPT_OBJECT_TAG, record.recordId);
}

export function createPowerPointAdapter(PowerPoint, { sessionId }) {
  return {
    async insert(envelope) {
      return PowerPoint.run(async (context) => {
        const selected = context.presentation.getSelectedSlides();
        selected.load("items/id");
        await context.sync();
        if (!selected.items.length)
          fail("NO_SLIDE", "Select a slide before inserting a ReShiki drawing.");
        if (selected.items.length !== 1)
          fail("MULTIPLE_SELECTION", "Select one slide before inserting a ReShiki drawing.");
        const slide = selected.items[0];
        const record = newRecord(envelope);
        assertCanAdd([], record);
        let identity;
        try {
          const shape = slide.shapes.addGeometricShape("Rectangle", {
            left: 36,
            top: 36,
            ...intrinsicSize(envelope),
          });
          shape.name = OBJECT_PREFIX + record.objectId;
          shape.lineFormat.visible = false;
          shape.fill.setImage(envelope.png);
          shape.tags.add(PPT_OBJECT_TAG, record.recordId);
          const part = shape.customXmlParts.add(encodeRecord(record));
          shape.load("id");
          await context.sync();
          identity = owner("PowerPoint", { slideId: slide.id, shapeId: shape.id });
          record.owner = identity;
          part.setXml(encodeRecord(record));
          await context.sync();
          const written = await readShape(context, identity);
          assertReadback(written.record, record);
          assertGeometry(written.geometry, intrinsicSize(envelope));
          return makeResult(written.record, sessionId);
        } catch (error) {
          let recoveryError;
          try {
            if (!identity)
              fail("UNKNOWN_INSERTION", "Office did not return the inserted shape identifier.");
            const current = await shapeAt(context, identity);
            current.shape.tags.load("items/key,items/value");
            await context.sync();
            const marker = current.shape.tags.items.find(
              (item) => item.key.toUpperCase() === PPT_OBJECT_TAG,
            );
            if (String(marker?.value).toLowerCase() !== record.recordId)
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
      return PowerPoint.run(async (context) => {
        const shapes = context.presentation.getSelectedShapes();
        const slides = context.presentation.getSelectedSlides();
        shapes.load("items/id");
        slides.load("items/id");
        await context.sync();
        if (!shapes.items.length) return null;
        if (shapes.items.length !== 1 || slides.items.length !== 1)
          fail(
            "MULTIPLE_SELECTION",
            "Select just one ReShiki drawing on one slide before opening the editor.",
          );
        const identity = owner("PowerPoint", {
          slideId: slides.items[0].id,
          shapeId: shapes.items[0].id,
        });
        let state = await readShape(context, identity, { allowCopy: true, allowUnowned: true });
        if (!state) return null;
        if (!sameOwner(state.record.owner, identity)) {
          const record = newRecord(state.record.envelope, identity);
          assertCanAdd(state.records, record);
          state.shape.customXmlParts.add(encodeRecord(record));
          await context.sync();
          const fresh = await readShape(context, identity, { allowCopy: true });
          if (fresh.record.recordId !== state.record.recordId)
            fail("TARGET_CHANGED", "The copied drawing changed while it was being opened.");
          fresh.shape.tags.add(PPT_OBJECT_TAG, record.recordId);
          await context.sync();
          state = await readShape(context, identity);
          assertReadback(state.record, record);
        }
        return makeResult(state.record, sessionId);
      });
    },

    async read(target) {
      return PowerPoint.run(async (context) => {
        const state = await readShape(context, target);
        assertRevision(state.record, target);
        return makeResult(state.record, sessionId);
      });
    },

    async update(target, envelope) {
      return PowerPoint.run(async (context) => {
        let { before, attempted } = await updatePreflight(async () => {
          const before = await readShape(context, target);
          assertRevision(before.record, target);
          const attempted = newRecord(envelope, before.identity, before.record.objectId);
          assertCanAdd(before.records, attempted);
          return { before, attempted };
        });
        let writingPreview = false;
        try {
          before.shape.customXmlParts.add(encodeRecord(attempted));
          await context.sync();
          const fresh = await readShape(context, target);
          assertCurrentRecord(fresh.record, before.record, target);
          before = fresh;
          const geometry = {
            ...before.geometry,
            ...scaledSize(before.geometry, before.record.envelope, envelope),
          };
          writingPreview = true;
          applyPreview(before.shape, attempted, geometry);
          await context.sync();
          const written = await readShape(context, target);
          assertReadback(written.record, attempted);
          assertGeometry(written.geometry, geometry, ["left", "top", "width", "height"]);
          return makeResult(written.record, sessionId);
        } catch (error) {
          if (!writingPreview) {
            try {
              const current = await shapeAt(context, target);
              await discardUnpublishedRecord(
                context,
                current.shape.customXmlParts,
                attempted,
                async () => {
                  // Copies carry their own shape-scoped parts. Only this exact
                  // shape can reference the staging part being removed here.
                  current.shape.tags.load("items/key,items/value");
                  await context.sync();
                  return current.shape.tags.items.some(
                    (tag) =>
                      tag.key.toUpperCase() === PPT_OBJECT_TAG &&
                      String(tag.value).toLowerCase() === attempted.recordId,
                  );
                },
              );
            } catch (recovery) {
              throw writeFailure(error, recovery, { operation: "update", target });
            }
            throw retryableUpdateFailure(error);
          }
          let recoveryError;
          try {
            const current = await readShape(context, target, { verifyPreview: false });
            if (!mayRollback(current.record, before.record, attempted))
              fail(
                "REVISION_CONFLICT",
                "Another edit superseded the failed write; it was left untouched.",
              );
            applyPreview(current.shape, before.record, before.geometry);
            await context.sync();
            const restored = await readShape(context, target);
            assertReadback(restored.record, before.record);
            assertGeometry(restored.geometry, before.geometry, ["left", "top", "width", "height"]);
          } catch (recovery) {
            recoveryError = recovery;
          }
          throw writeFailure(error, recoveryError, { operation: "update", target });
        }
      });
    },
  };
}
