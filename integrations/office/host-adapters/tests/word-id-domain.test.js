import test from "node:test";
import assert from "node:assert/strict";
import { createHostAdapter } from "../index.js";
import { createEnvelope } from "../../protocol.js";
import { decodeRecord, owner, sameOwner } from "../common.js";
import { assertEditableWordPicture } from "../word-ooxml.js";
import { fakeOffice } from "./fake-office.js";
import { wordPictureXml } from "./word-xml.js";

const PNG =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aZ1sAAAAASUVORK5CYII=";
const envelope = (label) =>
  createEnvelope({
    version: 1,
    native: Buffer.from(`RSK\0${label}`).toString("base64"),
    png: PNG,
    extent: [2540, 1270],
  });

test("Word ID domain: signed IDs retain their exact numeric identity", () => {
  for (const id of [-2147483648, -854810030, -1, 0, 2147483647]) {
    assert.equal(owner("Word", { contentControlId: id }).contentControlId, id);
  }
  // Retain previously accepted positive values without claiming an API mapping.
  for (const id of [3440157266, Number.MAX_SAFE_INTEGER]) {
    assert.equal(owner("Word", { contentControlId: id }).contentControlId, id);
  }
  for (const id of [-2147483649, -Number.MAX_SAFE_INTEGER, -1.5, NaN, Infinity, "-1", null]) {
    assert.throws(() => owner("Word", { contentControlId: id }), { code: "INVALID_TARGET" });
  }
  assert.equal(
    sameOwner(
      owner("Word", { contentControlId: -1 }),
      owner("Word", { contentControlId: 4294967295 }),
    ),
    false,
  );
});

for (const id of [-2147483648, -854810030, -1]) {
  test(`Word ID domain: ${id} survives insertion, record reload and editing`, async () => {
    // Synthetic host response: this does not establish Office JS's native ID sign.
    const fake = fakeOffice("Word", { contentControlIds: [id] });
    const adapter = createHostAdapter(fake.dependencies);
    const original = await envelope("original");
    const inserted = await adapter.insert(original);
    assert.equal(inserted.target.contentControlId, id);
    assert.equal((await decodeRecord(fake.state.parts[0].xml)).owner.contentControlId, id);
    assert.deepEqual((await adapter.read(inserted.target)).envelope, original);

    const reopened = createHostAdapter(fake.dependencies);
    const selected = await reopened.readSelected();
    assert.equal(selected.target.contentControlId, id);
    const changed = await envelope("changed");
    const updated = await reopened.update(selected.target, changed);
    assert.equal(updated.target.contentControlId, id);
    assert.deepEqual((await reopened.read(updated.target)).envelope, changed);
    assert.equal(fake.state.objects.filter((object) => !object.deleted).length, 1);
  });
}

test("Word ID domain: signed target tokens never alias another numeric ID or selection", async () => {
  const fake = fakeOffice("Word", { contentControlIds: [-1, -2] });
  const adapter = createHostAdapter(fake.dependencies);
  const first = await adapter.insert(await envelope("first"));
  fake.state.activeId = null;
  const second = await adapter.insert(await envelope("second"));
  const changed = await envelope("first changed");
  const count = fake.state.mutations.length;
  await assert.rejects(adapter.update({ ...first.target, contentControlId: 4294967295 }, changed), {
    code: "TARGET_DELETED",
  });
  await assert.rejects(adapter.update({ ...first.target, contentControlId: -2 }, changed), {
    code: "TARGET_CHANGED",
  });
  assert.equal(fake.state.mutations.length, count);
  // The active selection is still the second control; update must use the first ID.
  const updated = await adapter.update(first.target, changed);
  assert.equal(updated.target.contentControlId, -1);
  assert.deepEqual((await adapter.read(second.target)).envelope, second.envelope);
});

test("Word ID domain: OOXML binding stays exact and does not reinterpret unsigned aliases", () => {
  const control = { id: -1, tag: "reshiki:fixture" };
  const xml = wordPictureXml(control);
  assert.doesNotThrow(() => assertEditableWordPicture(xml, control));
  for (const id of [-2, 4294967295]) {
    assert.throws(() => assertEditableWordPicture(xml, { ...control, id }), {
      code: "UNSUPPORTED_CONTAINER",
    });
  }
  assert.throws(
    () => assertEditableWordPicture(xml.replace('w:val="-1"', 'w:val="-01"'), control),
    { code: "UNSUPPORTED_CONTAINER" },
  );
});
