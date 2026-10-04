import test from "node:test";
import assert from "node:assert/strict";
import { createHostAdapter } from "../index.js";
import { createEnvelope } from "../../protocol.js";
import {
  decodeRecord,
  encodeRecord,
  newRecord,
  owner,
  STORAGE_LIMITS,
  XML_NAMESPACE,
} from "../common.js";
import { fakeOffice } from "./fake-office.js";
import { wordPictureXml } from "./word-xml.js";

const PNG =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aZ1sAAAAASUVORK5CYII=";
const SECOND_PNG =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
async function envelope(label = "original", extent = [2540, 1270]) {
  // Opaque bytes deliberately include unsupported chemistry fields and binary
  // data. Adapters must never parse/reconstruct the native drawing.
  return createEnvelope({
    version: 1,
    native: Buffer.from(
      `RSK\0${label}\0Cu(II)|Rh complex|isotope=13|charge=-1|font=Noto\n\xff`,
    ).toString("base64"),
    png: label === "original" ? PNG : SECOND_PNG,
    extent,
  });
}
const idFrom = (target) => target.contentControlId ?? target.shapeId;
const preview = (fake, target) => {
  const object = fake.object(idFrom(target));
  return fake.state.host === "Word" ? object.pictures[0] : object;
};
const code = (expected) => (error) => error.code === expected;

for (const host of ["Word", "Excel", "PowerPoint"]) {
  test(`${host}: exact native bytes survive insertion and a new task-pane session`, async () => {
    const fake = fakeOffice(host);
    const adapter = createHostAdapter(fake.dependencies);
    const original = await envelope();
    const inserted = await adapter.insert(original);
    assert.deepEqual(inserted.envelope, original);
    assert.deepEqual((await adapter.read(inserted.target)).envelope, original);
    const reopened = createHostAdapter(fake.dependencies);
    assert.deepEqual((await reopened.readSelected()).envelope, original);
    await assert.rejects(reopened.read(inserted.target), code("INVALID_TARGET"));
    assert.equal(preview(fake, inserted.target).png, original.png);
  });

  test(`${host}: two edits preserve current anisotropic user scale`, async () => {
    const fake = fakeOffice(host);
    const adapter = createHostAdapter(fake.dependencies);
    const original = await envelope();
    const inserted = await adapter.insert(original);
    const image = preview(fake, inserted.target);
    image.width *= 2;
    image.height *= 3;
    if (host !== "Word") {
      image.left = 101;
      image.top = 63;
      image.rotation = 17;
    }
    const changed = await envelope("first", [5080, 1905]);
    const result = await adapter.update(inserted.target, changed);
    assert.equal(result.target.objectId, inserted.target.objectId);
    assert.equal(preview(fake, result.target).width, 288);
    assert.equal(preview(fake, result.target).height, 162);
    if (host !== "Word") {
      assert.equal(preview(fake, result.target).left, 101);
      assert.equal(preview(fake, result.target).top, 63);
      assert.equal(preview(fake, result.target).rotation, 17);
    }
    const changedAgain = await envelope("second", [1270, 2540]);
    const second = await adapter.update(result.target, changedAgain);
    assert.equal(second.target.objectId, inserted.target.objectId);
    assert.equal(preview(fake, second.target).width, 72);
    assert.equal(preview(fake, second.target).height, 216);
    assert.deepEqual((await adapter.read(second.target)).envelope, changedAgain);
    assert.equal(fake.state.objects.filter((object) => !object.deleted).length, 1);
    if (host === "Excel") assert.notEqual(second.target.shapeId, inserted.target.shapeId);
    else assert.equal(idFrom(second.target), idFrom(inserted.target));
  });

  test(`${host}: copied drawing retains old payload after original update and forks independently`, async () => {
    const fake = fakeOffice(host);
    const adapter = createHostAdapter(fake.dependencies);
    const original = await envelope();
    const inserted = await adapter.insert(original);
    const copy = fake.copy(idFrom(inserted.target));
    const changedOriginal = await adapter.update(
      inserted.target,
      await envelope("original changed"),
    );
    fake.state.activeId = copy.id;
    const copied = await adapter.readSelected();
    assert.equal(copied.envelope.native, original.native);
    assert.notEqual(copied.target.objectId, inserted.target.objectId);
    const copyChange = await envelope("copy changed");
    const edited = await adapter.update(copied.target, copyChange);
    assert.equal((await adapter.read(edited.target)).envelope.native, copyChange.native);
    assert.deepEqual(
      (await adapter.read(changedOriginal.target)).envelope,
      changedOriginal.envelope,
    );
    assert.equal(fake.state.objects.filter((object) => !object.deleted).length, 2);
  });

  test(`${host}: selected ordinary objects return null; owned objects missing payload fail`, async () => {
    const fake = fakeOffice(host);
    const adapter = createHostAdapter(fake.dependencies);
    assert.equal(await adapter.readSelected(), null);
    const inserted = await adapter.insert(await envelope());
    if (host === "PowerPoint") fake.object(idFrom(inserted.target)).parts.length = 0;
    else fake.state.parts.length = 0;
    await assert.rejects(adapter.readSelected(), code("MISSING_PAYLOAD"));
  });

  test(`${host}: stale token never falls back to the newly selected drawing`, async () => {
    const fake = fakeOffice(host);
    const adapter = createHostAdapter(fake.dependencies);
    const first = await adapter.insert(await envelope("first"));
    fake.state.activeId = null;
    const second = await adapter.insert(await envelope("second"));
    fake.object(idFrom(first.target)).deleted = true;
    const mutations = fake.state.mutations.length;
    await assert.rejects(
      adapter.update(first.target, await envelope("new")),
      code("TARGET_DELETED"),
    );
    assert.equal(fake.state.mutations.length, mutations);
    assert.equal((await adapter.read(second.target)).envelope.native, second.envelope.native);
  });

  test(`${host}: requirement failure and invalid payload write nothing`, async () => {
    const fake = fakeOffice(host);
    const adapter = createHostAdapter(fake.dependencies);
    fake.state.supported = false;
    await assert.rejects(adapter.insert(await envelope()), code("UNSUPPORTED_HOST"));
    fake.state.supported = true;
    const invalid = { ...(await envelope()), revision: "0".repeat(64) };
    await assert.rejects(adapter.insert(invalid));
    assert.equal(fake.state.mutations.length, 0);
  });

  test(`${host}: a marker changed while XML is read causes a conflict`, async () => {
    const fake = fakeOffice(host);
    const adapter = createHostAdapter(fake.dependencies);
    const inserted = await adapter.insert(await envelope());
    const object = fake.object(idFrom(inserted.target));
    fake.afterBatch(
      (labels) => labels.includes("part.getXml"),
      () => {
        if (host === "Word") object.tag = "reshiki:00000000-0000-0000-0000-000000000000";
        else if (host === "Excel")
          object.altTextDescription = object.altTextDescription.replace(
            /data:[^\]]+/,
            "data:00000000-0000-0000-0000-000000000000",
          );
        else object.tags[0].value = "00000000-0000-0000-0000-000000000000";
      },
    );
    await assert.rejects(adapter.read(inserted.target), code("REVISION_CONFLICT"));
  });

  test(`${host}: only one operation per adapter can mutate Office at a time`, async () => {
    const fake = fakeOffice(host);
    const adapter = createHostAdapter(fake.dependencies);
    const payload = await envelope();
    const first = adapter.insert(payload);
    await assert.rejects(adapter.insert(payload), code("BUSY"));
    await first;
    assert.equal(fake.state.objects.filter((object) => !object.deleted).length, 1);
  });
}

for (const [host, failureLabel] of [
  ["Word", "word.replace"],
  ["PowerPoint", "ppt.setImage"],
]) {
  test(`${host}: partially applied preview failure restores original preview and native record`, async () => {
    const fake = fakeOffice(host);
    const adapter = createHostAdapter(fake.dependencies);
    const original = await envelope();
    const inserted = await adapter.insert(original);
    const before = structuredClone(preview(fake, inserted.target));
    fake.failAfter(failureLabel);
    await assert.rejects(
      adapter.update(inserted.target, await envelope("failed", [6000, 4000])),
      code("HOST_WRITE_FAILED"),
    );
    assert.deepEqual((await adapter.read(inserted.target)).envelope, original);
    assert.equal(preview(fake, inserted.target).png, before.png);
    assert.equal(preview(fake, inserted.target).width, before.width);
    assert.equal(preview(fake, inserted.target).height, before.height);
  });

  test(`${host}: rollback never overwrites a newer coauthor record`, async () => {
    const fake = fakeOffice(host);
    const adapter = createHostAdapter(fake.dependencies);
    const inserted = await adapter.insert(await envelope());
    const object = fake.object(idFrom(inserted.target));
    const coauthor = newRecord(
      await envelope("coauthor"),
      owner(host, inserted.target),
      inserted.target.objectId,
    );
    fake.failAfter(failureLabel, () => {
      (host === "Word" ? fake.state.parts : object.parts).push({
        id: "coauthor-part",
        xml: encodeRecord(coauthor),
      });
      if (host === "Word") {
        object.tag = `reshiki:${coauthor.recordId}`;
        object.pictures[0].png = coauthor.envelope.png;
      } else {
        object.tags[0].value = coauthor.recordId;
        object.png = coauthor.envelope.png;
      }
    });
    await assert.rejects(
      adapter.update(inserted.target, await envelope("failed")),
      code("RECOVERY_REQUIRED"),
    );
    assert.deepEqual((await adapter.readSelected()).envelope, coauthor.envelope);
    assert.equal(preview(fake, inserted.target).png, coauthor.envelope.png);
  });
}

test("Excel: pre-deletion failure keeps the original shape, name, payload and order", async () => {
  const fake = fakeOffice("Excel");
  const adapter = createHostAdapter(fake.dependencies);
  const inserted = await adapter.insert(await envelope());
  const before = structuredClone(fake.object(idFrom(inserted.target)));
  fake.failAfter("shape.setZOrder");
  await assert.rejects(
    adapter.update(inserted.target, await envelope("failed")),
    code("HOST_WRITE_FAILED"),
  );
  assert.deepEqual(fake.object(idFrom(inserted.target)), before);
  assert.deepEqual((await adapter.read(inserted.target)).envelope, inserted.envelope);
  assert.equal(fake.state.objects.filter((object) => !object.deleted).length, 1);
});

test("Excel: an applied deletion with a failed sync succeeds only after complete readback", async () => {
  const fake = fakeOffice("Excel");
  const adapter = createHostAdapter(fake.dependencies);
  const inserted = await adapter.insert(await envelope());
  const changed = await envelope("updated");
  fake.failAfter("Excel.delete");
  const updated = await adapter.update(inserted.target, changed);
  assert.deepEqual(updated.envelope, changed);
  assert.equal(fake.state.objects.filter((object) => !object.deleted).length, 1);
  assert.notEqual(updated.target.shapeId, inserted.target.shapeId);
});

test("Excel: crop, color effects, outline, group and absent desktop API fail before mutation", async () => {
  for (const mutate of [
    (fake, object) => {
      object.image.cropLeft = 1;
    },
    (fake, object) => {
      object.image.brightness = 0.8;
    },
    (fake, object) => {
      object.lineFormat.visible = true;
    },
    (fake, object) => {
      object.level = 1;
    },
    (fake) => {
      fake.state.desktop = false;
    },
  ]) {
    const fake = fakeOffice("Excel");
    const adapter = createHostAdapter(fake.dependencies);
    const inserted = await adapter.insert(await envelope());
    mutate(fake, fake.object(idFrom(inserted.target)));
    const count = fake.state.mutations.length;
    await assert.rejects(adapter.update(inserted.target, await envelope("new")));
    assert.equal(fake.state.mutations.length, count);
  }
});

test("Excel: edit capability is advertised before native editing without blocking insertion", async () => {
  const fake = fakeOffice("Excel");
  fake.state.desktop = false;
  const adapter = createHostAdapter(fake.dependencies);
  const support = await adapter.checkSupport();
  assert.equal(support.canEdit, false);
  assert.match(support.editReason, /ExcelApiDesktop 1\.1/);
  const inserted = await adapter.insert(await envelope());
  assert.deepEqual((await adapter.read(inserted.target)).envelope, inserted.envelope);
  const count = fake.state.mutations.length;
  await assert.rejects(
    adapter.update(inserted.target, await envelope("updated")),
    code("UNSUPPORTED_HOST"),
  );
  assert.equal(fake.state.mutations.length, count);
});

test("Excel: attached connectors, including grouped connectors, fail before mutation", async () => {
  for (const grouped of [false, true]) {
    const fake = fakeOffice("Excel");
    const adapter = createHostAdapter(fake.dependencies);
    const inserted = await adapter.insert(await envelope());
    const connector = {
      id: "connector",
      name: "Connector",
      type: "Line",
      line: {
        isBeginConnected: true,
        isEndConnected: false,
        beginConnectedShape: { id: inserted.target.shapeId },
      },
    };
    fake.state.objects.push(
      grouped ? { id: "group", type: "Group", children: [connector] } : connector,
    );
    const count = fake.state.mutations.length;
    await assert.rejects(
      adapter.update(inserted.target, await envelope("updated")),
      code("ATTACHED_CONNECTOR"),
    );
    assert.equal(fake.state.mutations.length, count);
    assert.deepEqual((await adapter.read(inserted.target)).envelope, inserted.envelope);
  }
});

test("Excel: unrelated connectors are preserved while replacing a drawing", async () => {
  const fake = fakeOffice("Excel");
  const adapter = createHostAdapter(fake.dependencies);
  const inserted = await adapter.insert(await envelope());
  const connector = {
    id: "connector",
    name: "Connector",
    type: "Line",
    line: {
      isBeginConnected: true,
      isEndConnected: false,
      beginConnectedShape: { id: "another-shape" },
    },
  };
  fake.state.objects.push(connector);
  const updated = await adapter.update(inserted.target, await envelope("updated"));
  assert.ok(fake.state.objects.includes(connector));
  assert.deepEqual((await adapter.read(updated.target)).envelope, updated.envelope);
});

test("Word: cropped/rotated content and added text fail before mutation", async () => {
  for (const mutate of [
    (object) => {
      object.ooxml = wordPictureXml(object, '<a:srcRect l="2000"/>');
    },
    (object) => {
      object.ooxml = wordPictureXml(object, '<a:xfrm rot="3600"/>');
    },
    (object) => {
      object.text = "user text";
    },
  ]) {
    const fake = fakeOffice("Word");
    const adapter = createHostAdapter(fake.dependencies);
    const inserted = await adapter.insert(await envelope());
    mutate(fake.object(idFrom(inserted.target)));
    const count = fake.state.mutations.length;
    await assert.rejects(adapter.update(inserted.target, await envelope("new")));
    assert.equal(fake.state.mutations.length, count);
  }
});

test("Word: final OOXML preflight rechecks the marker before replacing the image", async () => {
  const fake = fakeOffice("Word");
  const adapter = createHostAdapter(fake.dependencies);
  const inserted = await adapter.insert(await envelope());
  const object = fake.object(idFrom(inserted.target));
  let checks = 0;
  fake.beforeBatch(
    (labels) => labels.includes("word.getOoxml") && ++checks === 2,
    () => {
      object.tag = "reshiki:00000000-0000-0000-0000-000000000000";
    },
  );
  await assert.rejects(
    adapter.update(inserted.target, await envelope("failed")),
    code("REVISION_CONFLICT"),
  );
  assert.equal(fake.state.mutations.filter((label) => label === "word.replace").length, 0);
});

test("Word: inserting inside an owned drawing fails without modifying it", async () => {
  const fake = fakeOffice("Word");
  const adapter = createHostAdapter(fake.dependencies);
  const inserted = await adapter.insert(await envelope());
  const count = fake.state.mutations.length;
  await assert.rejects(adapter.insert(await envelope("nested")), code("INVALID_INSERTION_POINT"));
  assert.equal(fake.state.mutations.length, count);
  assert.deepEqual((await adapter.read(inserted.target)).envelope, inserted.envelope);
});

test("PowerPoint: grouped image-filled shapes fail before mutation", async () => {
  const fake = fakeOffice("PowerPoint");
  const adapter = createHostAdapter(fake.dependencies);
  const inserted = await adapter.insert(await envelope());
  fake.object(idFrom(inserted.target)).level = 1;
  const count = fake.state.mutations.length;
  await assert.rejects(
    adapter.update(inserted.target, await envelope("new")),
    code("UNSUPPORTED_CONTAINER"),
  );
  assert.equal(fake.state.mutations.length, count);
});

test("XML: validates exact bytes/hash/schema and rejects external entities", async () => {
  const record = newRecord(await envelope(), owner("Word", { contentControlId: 2 }));
  assert.deepEqual(await decodeRecord(encodeRecord(record)), record);
  await assert.rejects(decodeRecord(encodeRecord(record).replace("<native>", "<native>&xxe;")));
  await assert.rejects(
    decodeRecord(
      '<!DOCTYPE x [<!ENTITY xxe SYSTEM "file:///private/data">]>' + encodeRecord(record),
    ),
  );
  await assert.rejects(decodeRecord(encodeRecord(record).replace('version="1"', 'version="2"')));
  await assert.rejects(
    decodeRecord(encodeRecord(record).replace(record.envelope.revision, "0".repeat(64))),
  );
});

test("XML: document record limit stops insert without deleting history", async () => {
  const fake = fakeOffice("Word");
  const original = await envelope();
  const record = newRecord(original, owner("Word", { contentControlId: 1 }));
  for (let index = 0; index < STORAGE_LIMITS.records; index++)
    fake.state.parts.push({
      id: `existing-${index}`,
      xml: encodeRecord({ ...record, recordId: crypto.randomUUID() }),
    });
  const adapter = createHostAdapter(fake.dependencies);
  await assert.rejects(adapter.insert(original), code("STORAGE_LIMIT"));
  assert.equal(fake.state.parts.length, STORAGE_LIMITS.records);
  assert.equal(fake.state.mutations.length, 0);
});

test("XML: oversized first part stops before fetching remaining parts", async () => {
  const fake = fakeOffice("Word");
  fake.state.parts.push({
    id: "large",
    xml: `<x xmlns="${XML_NAMESPACE}">${"x".repeat(34 * 1024 * 1024)}</x>`,
  });
  fake.state.parts.push({ id: "never-fetch", xml: "invalid " + XML_NAMESPACE });
  const adapter = createHostAdapter(fake.dependencies);
  await assert.rejects(adapter.insert(await envelope()), code("INVALID_PAYLOAD"));
  assert.equal(fake.state.batches.flat().filter((label) => label === "part.getXml").length, 1);
});

test("XML: maximum native payload roundtrips without regex stack overflow", async () => {
  const native = Buffer.alloc(16 * 1024 * 1024, 0xa5).toString("base64");
  const payload = await createEnvelope({ version: 1, native, png: PNG, extent: [2540, 1270] });
  const record = newRecord(payload, owner("Word", { contentControlId: 1 }));
  const result = await decodeRecord(encodeRecord(record));
  assert.equal(result.envelope.native, native);
  assert.equal(result.envelope.revision, payload.revision);
});
