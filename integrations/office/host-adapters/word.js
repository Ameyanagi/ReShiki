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
  markerId,
  mayRollback,
  newRecord,
  owner,
  readRecords,
  sameOwner,
  scaledSize,
  WORD_TAG_PREFIX,
  writeFailure,
  updatePreflight,
  retryableUpdateFailure,
} from "./common.js";
import { assertEditableWordPicture } from "./word-ooxml.js";

const CONTROL_PROPERTIES = "id,tag,type,subtype,cannotEdit,text";
const PICTURE_PROPERTIES =
  "items/width,items/height,items/lockAspectRatio,items/altTextTitle,items/altTextDescription,items/hyperlink";

async function controlAt(context, id) {
  const control = context.document.contentControls.getByIdOrNullObject(id);
  control.load(CONTROL_PROPERTIES);
  await context.sync();
  if (control.isNullObject)
    fail("TARGET_DELETED", "The Word drawing that was opened for editing has been deleted.");
  return control;
}

async function pictureIn(context, control) {
  control.load(CONTROL_PROPERTIES);
  control.inlinePictures.load(PICTURE_PROPERTIES);
  control.contentControls.load("items/id");
  await context.sync();
  return loadedPicture(control);
}

function loadedPicture(control) {
  if (
    control.inlinePictures.items.length !== 1 ||
    control.contentControls.items.length ||
    // oxlint-disable-next-line no-control-regex -- Word uses these sentinels for inline pictures and cell boundaries.
    control.text.replace(/[\s\u0001\u0007\uFFFC]/g, "") ||
    !["RichText", "RichTextInline", "RichTextParagraphs"].includes(control.type) ||
    ["RichTextTable", "RichTextTableRow", "RichTextTableCell"].includes(control.subtype)
  ) {
    fail(
      "UNSUPPORTED_CONTAINER",
      "The ReShiki content control must contain only its original inline drawing. Move added text or other content outside it before editing.",
    );
  }
  const picture = control.inlinePictures.items[0];
  const geometry = {
    width: picture.width,
    height: picture.height,
    lockAspectRatio: picture.lockAspectRatio,
    altTextTitle: picture.altTextTitle,
    altTextDescription: picture.altTextDescription,
    hyperlink: picture.hyperlink,
  };
  if (![geometry.width, geometry.height].every((n) => Number.isFinite(n) && n > 0))
    fail("INVALID_GEOMETRY", "Word returned invalid drawing dimensions.");
  return { picture, geometry };
}

async function readControl(context, id, { allowCopy = false, records, verifyPreview = true } = {}) {
  const control = await controlAt(context, id);
  const recordId = markerId(control.tag, WORD_TAG_PREFIX);
  if (!recordId || control.tag !== WORD_TAG_PREFIX + recordId)
    fail("TARGET_CHANGED", "The original Word content control is no longer a ReShiki drawing.");
  const identity = owner("Word", { contentControlId: control.id });
  const allRecords = records || (await readRecords(context, context.document.customXmlParts));
  const record = findRecord(allRecords, recordId, identity, allowCopy);
  const preview = verifyPreview ? await pictureIn(context, control) : {};
  if (!verifyPreview) {
    control.load(CONTROL_PROPERTIES);
    await context.sync();
  }
  if (control.tag !== WORD_TAG_PREFIX + recordId)
    fail("REVISION_CONFLICT", "The Word drawing changed while its editable data was being read.");
  return { control, record, identity, records: allRecords, ...preview };
}

async function checkEditablePicture(context, state) {
  if (state.control.cannotEdit)
    fail(
      "PROTECTED_OBJECT",
      "This Word drawing is locked against editing. Unlock it in Word before applying changes.",
    );
  const ooxml = state.control.getOoxml();
  state.control.load(CONTROL_PROPERTIES);
  state.control.inlinePictures.load(PICTURE_PROPERTIES);
  state.control.contentControls.load("items/id");
  await context.sync();
  if (state.control.tag !== WORD_TAG_PREFIX + state.record.recordId)
    fail("REVISION_CONFLICT", "The Word drawing changed during the final edit check.");
  if (state.control.cannotEdit)
    fail("PROTECTED_OBJECT", "This Word drawing was locked while the edit was being prepared.");
  Object.assign(state, loadedPicture(state.control));
  // These transformations cannot be restored through the Word 1.4 picture API.
  assertEditableWordPicture(ooxml.value, state.control);
}

function replacePicture(control, record, geometry) {
  const picture = control.insertInlinePictureFromBase64(record.envelope.png, "Replace");
  picture.lockAspectRatio = false;
  picture.width = geometry.width;
  picture.height = geometry.height;
  picture.lockAspectRatio = geometry.lockAspectRatio;
  picture.altTextTitle = geometry.altTextTitle;
  picture.altTextDescription = geometry.altTextDescription;
  picture.hyperlink = geometry.hyperlink;
  control.tag = WORD_TAG_PREFIX + record.recordId;
}

async function selectedControl(context) {
  const selection = context.document.getSelection();
  const children = selection.contentControls;
  const parent = selection.parentContentControlOrNullObject;
  children.load("items/id,items/tag");
  parent.load("id,tag");
  await context.sync();
  const candidates = new Map(
    children.items
      .filter((control) => control.tag.startsWith(WORD_TAG_PREFIX))
      .map((control) => [control.id, control]),
  );
  if (!parent.isNullObject && parent.tag.startsWith(WORD_TAG_PREFIX))
    candidates.set(parent.id, parent);
  if (candidates.size > 1)
    fail("MULTIPLE_SELECTION", "Select just one ReShiki drawing before opening the editor.");
  return candidates.values().next().value || null;
}

export function createWordAdapter(Word, { sessionId }) {
  return {
    async insert(envelope) {
      return Word.run(async (context) => {
        if (await selectedControl(context))
          fail(
            "INVALID_INSERTION_POINT",
            "Move the Word cursor outside the existing ReShiki drawing before inserting another drawing.",
          );
        const records = await readRecords(context, context.document.customXmlParts);
        const record = newRecord(envelope);
        assertCanAdd(records, record);
        let control;
        let id;
        try {
          // End preserves a selected text range instead of replacing user text.
          const picture = context.document
            .getSelection()
            .insertInlinePictureFromBase64(envelope.png, "End");
          picture.lockAspectRatio = false;
          Object.assign(picture, intrinsicSize(envelope));
          picture.lockAspectRatio = true;
          picture.altTextTitle = "ReShiki chemical structure";
          picture.altTextDescription =
            "Editable chemical drawing. Select it and choose Edit in ReShiki.";
          control = picture.insertContentControl();
          control.tag = WORD_TAG_PREFIX + record.recordId;
          control.title = "ReShiki chemical structure";
          control.removeWhenEdited = false;
          control.load("id");
          const part = context.document.customXmlParts.add(encodeRecord(record));
          await context.sync();
          id = control.id;
          record.owner = owner("Word", { contentControlId: id });
          part.setXml(encodeRecord(record));
          await context.sync();
          const written = await readControl(context, id);
          assertReadback(written.record, record);
          assertGeometry(written.geometry, intrinsicSize(envelope));
          return makeResult(written.record, sessionId);
        } catch (error) {
          let recoveryError;
          try {
            if (id === undefined)
              fail(
                "UNKNOWN_INSERTION",
                "Office did not return the newly inserted content-control ID.",
              );
            const current = await controlAt(context, id);
            if (current.tag !== WORD_TAG_PREFIX + record.recordId)
              fail("TARGET_CHANGED", "The incomplete insertion was changed by another editor.");
            current.delete(false);
            await context.sync();
          } catch (recovery) {
            recoveryError = recovery;
          }
          throw writeFailure(error, recoveryError, { operation: "insert", contentControlId: id });
        }
      });
    },

    async readSelected() {
      return Word.run(async (context) => {
        const selected = await selectedControl(context);
        if (!selected) return null;
        let state = await readControl(context, selected.id, { allowCopy: true });
        if (!sameOwner(state.record.owner, state.identity)) {
          if (state.control.cannotEdit)
            fail(
              "PROTECTED_OBJECT",
              "Unlock this copied drawing in Word before opening it for editing.",
            );
          // Fork an immutable version: later edits to either copy stay independent.
          const record = newRecord(state.record.envelope, state.identity);
          assertCanAdd(state.records, record);
          context.document.customXmlParts.add(encodeRecord(record));
          await context.sync();
          const current = await controlAt(context, selected.id);
          if (current.tag !== WORD_TAG_PREFIX + state.record.recordId)
            fail("TARGET_CHANGED", "The copied drawing changed while it was being opened.");
          current.tag = WORD_TAG_PREFIX + record.recordId;
          await context.sync();
          state = await readControl(context, selected.id);
          assertReadback(state.record, record);
        }
        return makeResult(state.record, sessionId);
      });
    },

    async read(target) {
      return Word.run(async (context) => {
        const state = await readControl(context, target.contentControlId);
        assertRevision(state.record, target);
        return makeResult(state.record, sessionId);
      });
    },

    async update(target, envelope) {
      return Word.run(async (context) => {
        let { before, attempted } = await updatePreflight(async () => {
          const before = await readControl(context, target.contentControlId);
          assertRevision(before.record, target);
          await checkEditablePicture(context, before);
          const attempted = newRecord(envelope, before.identity, before.record.objectId);
          assertCanAdd(before.records, attempted);
          return { before, attempted };
        });
        let writingPicture = false;
        try {
          // Save the complete native data before any preview replacement.
          context.document.customXmlParts.add(encodeRecord(attempted));
          await context.sync();
          const fresh = await readControl(context, target.contentControlId);
          assertRevision(fresh.record, target);
          if (fresh.record.recordId !== before.record.recordId)
            fail(
              "REVISION_CONFLICT",
              "The drawing received another update while this edit was being applied.",
            );
          before = fresh;
          await checkEditablePicture(context, before);
          const geometry = {
            ...before.geometry,
            ...scaledSize(before.geometry, before.record.envelope, envelope),
          };
          writingPicture = true;
          replacePicture(before.control, attempted, geometry);
          await context.sync();
          const written = await readControl(context, target.contentControlId);
          assertReadback(written.record, attempted);
          assertGeometry(written.geometry, geometry, [
            "width",
            "height",
            "lockAspectRatio",
            "altTextTitle",
            "altTextDescription",
            "hyperlink",
          ]);
          return makeResult(written.record, sessionId);
        } catch (error) {
          if (!writingPicture) throw retryableUpdateFailure(error);
          let recoveryError;
          if (writingPicture) {
            try {
              const current = await readControl(context, target.contentControlId, {
                verifyPreview: false,
              });
              if (!mayRollback(current.record, before.record, attempted))
                fail(
                  "REVISION_CONFLICT",
                  "Another edit superseded the failed write; it was left untouched.",
                );
              replacePicture(current.control, before.record, before.geometry);
              await context.sync();
              const restored = await readControl(context, target.contentControlId);
              assertReadback(restored.record, before.record);
              assertGeometry(restored.geometry, before.geometry, [
                "width",
                "height",
                "lockAspectRatio",
                "altTextTitle",
                "altTextDescription",
                "hyperlink",
              ]);
            } catch (recovery) {
              recoveryError = recovery;
            }
          }
          throw writeFailure(error, recoveryError, { operation: "update", target });
        }
      });
    },
  };
}
