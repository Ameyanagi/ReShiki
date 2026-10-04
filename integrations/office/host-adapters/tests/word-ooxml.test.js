import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createHostAdapter } from "../index.js";
import { createEnvelope } from "../../protocol.js";
import { fakeOffice } from "./fake-office.js";
import { wordPictureXml } from "./word-xml.js";
import { assertEditableWordPicture } from "../word-ooxml.js";

const savedPackage = readFileSync(
  new URL("./fixtures/word-plain-picture-theme.xml", import.meta.url),
  "utf8",
);
const PNG =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aZ1sAAAAASUVORK5CYII=";
const envelope = (label) =>
  createEnvelope({
    version: 1,
    native: Buffer.from(`RSK\0${label}`).toString("base64"),
    png: PNG,
    extent: [2540, 1270],
  });

const documentXml = (...content) =>
  `<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>${content.join("")}</w:body></w:document>`;
const packageXml = (main, path = "/word/document.xml") =>
  `<pkg:package xmlns:pkg="http://schemas.microsoft.com/office/2006/xmlPackage">
    <pkg:part pkg:name="/_rels/.rels"><pkg:xmlData>
      <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="${path.slice(1)}"/>
      </Relationships>
    </pkg:xmlData></pkg:part>
    <pkg:part pkg:name="${path}"><pkg:xmlData>${main}</pkg:xmlData></pkg:part>
  </pkg:package>`;

async function drawing() {
  const fake = fakeOffice("Word");
  const adapter = createHostAdapter(fake.dependencies);
  const inserted = await adapter.insert(await envelope("original"));
  return { fake, adapter, inserted, control: fake.object(inserted.target.contentControlId) };
}

test("Word: saved-package theme defaults do not block a plain owned picture", async () => {
  const { fake, adapter, inserted, control } = await drawing();
  // This is reconstructed Flat OPC from a saved DOCX, not a live getOoxml capture.
  control.ooxml = savedPackage
    .replace('w:val="399796563"', `w:val="${control.id}"`)
    .replace("reshiki:23a276c1-0c12-4c2b-a7a8-f81510c05cb8", control.tag);
  const changed = await envelope("changed");
  const updated = await adapter.update(inserted.target, changed);
  assert.equal(updated.target.contentControlId, inserted.target.contentControlId);
  assert.deepEqual((await adapter.read(updated.target)).envelope, changed);
  assert.equal(fake.state.objects.filter((object) => !object.deleted).length, 1);
});

for (const [label, xml] of [
  [
    "namespace aliases",
    (control) =>
      wordPictureXml(control)
        .replaceAll("w:", "word:")
        .replaceAll("xmlns:w=", "xmlns:word=")
        .replaceAll("a:", "draw:")
        .replaceAll("xmlns:a=", "xmlns:draw="),
  ],
  [
    "default drawing namespace",
    (control) =>
      wordPictureXml(
        control,
        '<xfrm xmlns="http://schemas.openxmlformats.org/drawingml/2006/main" rot="0" flipH="false"/>',
      ),
  ],
  ["document fragment", (control) => documentXml(wordPictureXml(control))],
  [
    "relationship-selected main part",
    (control) => packageXml(documentXml(wordPictureXml(control)), "/custom/main.xml"),
  ],
  [
    "unrelated transformed drawing",
    (control) =>
      documentXml(
        wordPictureXml({ id: 912, tag: control.tag }, '<a:xfrm rot="90000"/><a:outerShdw/>'),
        wordPictureXml(control),
      ),
  ],
  [
    "namespace lookalike outside DrawingML",
    (control) => wordPictureXml(control, '<other:shade xmlns:other="urn:unrelated"/>'),
  ],
  [
    "explicit neutral transforms",
    (control) =>
      wordPictureXml(
        control,
        '<a:xfrm rot="-0" flipV="0"/><a:srcRect l="0" t="0.0%" r="+0" b="-0"/><a:effectLst/><a:effectRef idx="0"/>',
      ),
  ],
]) {
  test(`Word OOXML: accepts ${label}`, async () => {
    const { fake, adapter, inserted, control } = await drawing();
    control.ooxml = xml(control);
    const changed = await envelope(label);
    const updated = await adapter.update(inserted.target, changed);
    assert.deepEqual((await adapter.read(updated.target)).envelope, changed);
    assert.equal(fake.state.mutations.filter((mutation) => mutation === "word.replace").length, 1);
  });
}

for (const markup of [
  '<a:srcRect l="2000"/>',
  '<a:srcRect r="-0.1%"/>',
  '<a:xfrm rot="60000"/>',
  '<a:xfrm flipH="1"/>',
  '<a:xfrm flipV="true"/>',
  '<a:srcRect b="invalid"/>',
  '<a:xfrm rot="NaN"/>',
  '<a:xfrm flipH="invalid"/>',
  "<a:outerShdw/>",
  '<a:shade val="50000"/>',
  "<a:duotone/>",
  "<a:effectLst><a:blur/></a:effectLst>",
  "<a:effectDag><a:cont/></a:effectDag>",
  '<a:effectRef idx="1"/>',
  "<a:effectRef/>",
  '<a:blip><a:alphaModFix amt="50000"/></a:blip>',
  '<xfrm xmlns="http://schemas.openxmlformats.org/drawingml/2006/main" rot="60000"/>',
]) {
  test(`Word OOXML: rejects picture transform ${markup} before mutation`, async () => {
    const { fake, adapter, inserted, control } = await drawing();
    control.ooxml = wordPictureXml(control, markup);
    const count = fake.state.mutations.length;
    await assert.rejects(adapter.update(inserted.target, await envelope("changed")), {
      code: "UNSUPPORTED_TRANSFORM",
    });
    assert.equal(fake.state.mutations.length, count);
    assert.deepEqual((await adapter.read(inserted.target)).envelope, inserted.envelope);
  });
}

for (const [label, xml] of [
  ["missing target ID", (control) => wordPictureXml(control).replace(/<w:id[^>]+\/>/, "")],
  ["wrong target ID", (control) => wordPictureXml({ ...control, id: 999 })],
  ["missing tag", (control) => wordPictureXml(control).replace(/<w:tag[^>]+\/>/, "")],
  ["wrong tag", (control) => wordPictureXml({ ...control, tag: "reshiki:other" })],
  [
    "duplicate target controls",
    (control) => documentXml(wordPictureXml(control), wordPictureXml(control)),
  ],
  [
    "duplicate ID properties",
    (control) =>
      wordPictureXml(control).replace("</w:sdtPr>", `<w:id w:val="${control.id}"/></w:sdtPr>`),
  ],
  ["missing picture", (control) => wordPictureXml(control).replace(/<pic:pic>.*?<\/pic:pic>/s, "")],
  [
    "duplicate pictures",
    (control) => wordPictureXml(control).replace("</a:graphicData>", "<pic:pic/></a:graphicData>"),
  ],
  ["floating picture", (control) => wordPictureXml(control).replaceAll("wp:inline", "wp:anchor")],
  [
    "nested content control",
    (control) => wordPictureXml(control).replace("</w:sdtContent>", "<w:sdt/></w:sdtContent>"),
  ],
  ["malformed XML", (control) => wordPictureXml(control).replace("</w:sdt>", "")],
  ["undeclared namespace", (control) => wordPictureXml(control).replace(/xmlns:pic="[^"]+"/, "")],
  ["invalid entity", (control) => wordPictureXml(control).replace("</w:sdt>", "&unknown;</w:sdt>")],
  [
    "internal DTD",
    (control) => '<!DOCTYPE sdt [<!ENTITY entity "data">]>' + wordPictureXml(control),
  ],
  [
    "external DTD",
    (control) => '<!DOCTYPE sdt SYSTEM "file:///private/data">' + wordPictureXml(control),
  ],
  [
    "browser parser error",
    (control) =>
      wordPictureXml(control).replace(
        "</w:sdt>",
        '<parsererror xmlns="http://www.mozilla.org/newlayout/xml/parsererror.xml">error</parsererror></w:sdt>',
      ),
  ],
  ["unqualified parser error", () => "<parsererror>error</parsererror>"],
  [
    "wrong document relationship",
    (control) =>
      packageXml(documentXml(wordPictureXml(control))).replace(
        'Target="word/document.xml"',
        'Target="missing.xml"',
      ),
  ],
  [
    "external main relationship",
    (control) =>
      packageXml(documentXml(wordPictureXml(control))).replace(
        'Target="word/document.xml"',
        'TargetMode="External" Target="https://example.test/document.xml"',
      ),
  ],
  [
    "duplicate main relationship",
    (control) =>
      packageXml(documentXml(wordPictureXml(control))).replace(
        /(<Relationship Id[^>]+\/>)/,
        "$1$1",
      ),
  ],
  [
    "duplicate part names",
    (control) =>
      packageXml(documentXml(wordPictureXml(control))).replace(
        "</pkg:package>",
        '<pkg:part pkg:name="/word/document.xml"/></pkg:package>',
      ),
  ],
  [
    "missing root relationships",
    (control) =>
      packageXml(documentXml(wordPictureXml(control))).replace(
        'pkg:name="/_rels/.rels"',
        'pkg:name="/other.rels"',
      ),
  ],
  [
    "only a decoy document part",
    (control) =>
      packageXml(documentXml(wordPictureXml(control))).replace(
        'Target="word/document.xml"',
        'Target="elsewhere.xml"',
      ),
  ],
]) {
  test(`Word OOXML: rejects ${label} before mutation`, async () => {
    const { fake, adapter, inserted, control } = await drawing();
    control.ooxml = xml(control);
    const count = fake.state.mutations.length;
    await assert.rejects(adapter.update(inserted.target, await envelope("changed")), {
      code: "UNSUPPORTED_CONTAINER",
    });
    assert.equal(fake.state.mutations.length, count);
    assert.deepEqual((await adapter.read(inserted.target)).envelope, inserted.envelope);
  });
}

test("Word OOXML: a transform added during the final check stops image replacement", async () => {
  const { fake, adapter, inserted, control } = await drawing();
  let checks = 0;
  fake.beforeBatch(
    (labels) => labels.includes("word.getOoxml") && ++checks === 2,
    () => {
      control.ooxml = wordPictureXml(control, '<a:xfrm rot="60000"/>');
    },
  );
  await assert.rejects(adapter.update(inserted.target, await envelope("changed")), {
    code: "UNSUPPORTED_TRANSFORM",
  });
  assert.equal(fake.state.mutations.filter((mutation) => mutation === "word.replace").length, 0);
  assert.deepEqual((await adapter.read(inserted.target)).envelope, inserted.envelope);
});

const DPI_URI = "{28A0092B-C50C-407E-A947-70E740481C1C}";
const OFFICE_DRAWING = "http://schemas.microsoft.com/office/drawing/2010/main";
const dpi = (attributes = 'val="0"') =>
  `<a14:useLocalDpi xmlns:a14="${OFFICE_DRAWING}" ${attributes}/>`;
const extension = (content, uri = DPI_URI) => `<a:ext uri="${uri}">${content}</a:ext>`;
const blipExtensions = (control, extensions) =>
  wordPictureXml(control).replace(
    "<pic:spPr>",
    `<pic:blipFill><a:blip><a:extLst>${extensions}</a:extLst></a:blip></pic:blipFill><pic:spPr>`,
  );
// MS-ODRAWXML Pictures documents this extension as an editable artistic effect,
// not plain image metadata. Word replacement must not silently discard it.
const artisticExtension = extension(
  `<a14:imgProps xmlns:a14="${OFFICE_DRAWING}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
    <a14:imgLayer r:embed="rId3"><a14:imgEffect>
      <a14:artisticLineDrawing trans="75000" pencilSize="15"/>
    </a14:imgEffect></a14:imgLayer>
  </a14:imgProps>`,
  "BEBA8EAE-BF5A-486c-A8C5-ECC9F3942E4B",
);

test("Word OOXML: artistic blip extensions stop the adapter before replacement", async () => {
  const { fake, adapter, inserted, control } = await drawing();
  control.ooxml = blipExtensions(control, artisticExtension);
  const count = fake.state.mutations.length;
  await assert.rejects(adapter.update(inserted.target, await envelope("changed")), {
    code: "UNSUPPORTED_TRANSFORM",
  });
  assert.equal(fake.state.mutations.length, count);
  assert.equal(fake.state.mutations.filter((mutation) => mutation === "word.replace").length, 0);
  assert.deepEqual((await adapter.read(inserted.target)).envelope, inserted.envelope);
});

test("Word OOXML: documented useLocalDpi metadata permits normal replacement", async () => {
  const { fake, adapter, inserted, control } = await drawing();
  control.ooxml = blipExtensions(control, extension(dpi()));
  const changed = await envelope("changed");
  const updated = await adapter.update(inserted.target, changed);
  assert.deepEqual((await adapter.read(updated.target)).envelope, changed);
  assert.equal(fake.state.mutations.filter((mutation) => mutation === "word.replace").length, 1);
});

test("Word OOXML: raw parser allows only valid useLocalDpi extension metadata", () => {
  const control = { id: 41, tag: "reshiki:fixture" };
  for (const attributes of ['val="0"', 'val="1"', 'val="false"', 'val="true"', ""]) {
    for (const uri of [DPI_URI, "28A0092B-C50C-407e-A947-70E740481C1C"]) {
      assert.doesNotThrow(() =>
        assertEditableWordPicture(
          blipExtensions(control, extension(dpi(attributes), uri)),
          control,
        ),
      );
    }
  }
  for (const bad of [
    artisticExtension,
    extension(`<a14:imgProps xmlns:a14="${OFFICE_DRAWING}"/>`),
    extension(`<a14:artisticLineDrawing xmlns:a14="${OFFICE_DRAWING}"/>`),
    extension('<custom:effect xmlns:custom="urn:unknown"/>', "unknown"),
    extension(dpi(), "unknown"),
    extension(dpi('val="invalid"')),
    extension(dpi('val="0" custom="effect"')),
    extension(dpi().replace(OFFICE_DRAWING, "urn:unknown")),
    extension(dpi().replace("/>", "><a:blur/></a14:useLocalDpi>")),
    extension(dpi() + dpi()),
    extension(dpi()).replace('uri="', 'unexpected="effect" uri="'),
    extension(dpi()) + artisticExtension,
  ]) {
    assert.throws(() => assertEditableWordPicture(blipExtensions(control, bad), control), {
      code: "UNSUPPORTED_TRANSFORM",
    });
  }
});
