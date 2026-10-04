import { fail, STORAGE_LIMITS } from "./common.js";

const NS = {
  word: "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
  drawing: "http://schemas.openxmlformats.org/drawingml/2006/main",
  picture: "http://schemas.openxmlformats.org/drawingml/2006/picture",
  inline: "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
  package: "http://schemas.microsoft.com/office/2006/xmlPackage",
  relationships: "http://schemas.openxmlformats.org/package/2006/relationships",
  drawing2010: "http://schemas.microsoft.com/office/drawing/2010/main",
};
const LOCAL_DPI_URI = "28a0092b-c50c-407e-a947-70e740481c1c";
const OFFICE_DOCUMENT =
  "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const EFFECTS = new Set([
  "duotone",
  "grayscl",
  "biLevel",
  "lum",
  "clrChange",
  "tint",
  "shade",
  "glow",
  "outerShdw",
  "innerShdw",
  "softEdge",
  "reflection",
]);
const is = (node, namespace, name) => node?.namespaceURI === namespace && node.localName === name;
const elements = (node) => Array.from(node.childNodes).filter((child) => child.nodeType === 1);
const children = (node, namespace, name) =>
  elements(node).filter((child) => is(child, namespace, name));
const descendants = (node, namespace, name) =>
  Array.from(node.getElementsByTagNameNS(namespace, name));

function unreadable() {
  fail(
    "UNSUPPORTED_CONTAINER",
    "Word did not return an unambiguous original inline picture for this ReShiki drawing. Reopen the drawing before editing.",
  );
}

function one(nodes) {
  if (nodes.length !== 1) unreadable();
  return nodes[0];
}

function parse(xml) {
  if (
    typeof xml !== "string" ||
    xml.length > STORAGE_LIMITS.xmlBytes ||
    /<!\s*(?:DOCTYPE|ENTITY)\b/i.test(xml)
  )
    unreadable();
  let document;
  try {
    // Use the host webview's XML parser. DTDs are rejected before parsing, and
    // browser parser-error documents are never treated as editable content.
    document = new DOMParser().parseFromString(xml, "application/xml");
  } catch {
    unreadable();
  }
  if (
    !document.documentElement ||
    document.doctype ||
    descendants(document, "*", "parsererror").length
  )
    unreadable();
  return document.documentElement;
}

function partPath(value) {
  // Package relationships are URI references, not filesystem paths or URLs to fetch.
  if (!value || /[\s\\?#]/.test(value) || value.startsWith("//") || value.includes(":"))
    unreadable();
  try {
    return new URL(value, "https://reshiki.invalid/").pathname;
  } catch {
    unreadable();
  }
}

function documentRoot(root) {
  if (!is(root, NS.package, "package")) return root;
  const parts = new Map();
  for (const part of children(root, NS.package, "part")) {
    const name = part.getAttributeNS(NS.package, "name") || "";
    if (!name.startsWith("/") || partPath(name) !== name || parts.has(name)) unreadable();
    parts.set(name, part);
  }
  const xmlRoot = (name) => {
    const part = parts.get(name);
    if (!part) unreadable();
    return one(elements(one(children(part, NS.package, "xmlData"))));
  };
  const relationships = xmlRoot("/_rels/.rels");
  if (!is(relationships, NS.relationships, "Relationships")) unreadable();
  const main = one(
    children(relationships, NS.relationships, "Relationship").filter(
      (relationship) => relationship.getAttribute("Type") === OFFICE_DOCUMENT,
    ),
  );
  if (![null, "", "Internal"].includes(main.getAttribute("TargetMode"))) unreadable();
  const document = xmlRoot(partPath(main.getAttribute("Target")));
  if (!is(document, NS.word, "document")) unreadable();
  return document;
}

function ownedInline(root, control) {
  if (!is(root, NS.word, "document") && !is(root, NS.word, "sdt")) unreadable();
  const controls = descendants(root, NS.word, "sdt");
  if (is(root, NS.word, "sdt")) controls.unshift(root);
  const target = one(
    controls.filter((candidate) =>
      children(candidate, NS.word, "sdtPr").some((properties) =>
        children(properties, NS.word, "id").some(
          (id) => id.getAttributeNS(NS.word, "val") === String(control.id),
        ),
      ),
    ),
  );
  const properties = one(children(target, NS.word, "sdtPr"));
  one(children(properties, NS.word, "id"));
  if (one(children(properties, NS.word, "tag")).getAttributeNS(NS.word, "val") !== control.tag)
    unreadable();
  const content = one(children(target, NS.word, "sdtContent"));
  if (
    ["sdt", "pict", "object"].some((name) => descendants(content, NS.word, name).length) ||
    descendants(content, NS.inline, "anchor").length
  )
    unreadable();
  const drawing = one(descendants(content, NS.word, "drawing"));
  const inline = one(descendants(content, NS.inline, "inline"));
  if (inline.parentNode !== drawing || elements(drawing).length !== 1) unreadable();
  const graphic = one(children(inline, NS.drawing, "graphic"));
  const data = one(children(graphic, NS.drawing, "graphicData"));
  const picture = one(descendants(content, NS.picture, "pic"));
  if (
    data.getAttribute("uri") !== NS.picture ||
    picture.parentNode !== data ||
    elements(data).length !== 1
  )
    unreadable();
  return inline;
}

function nonzero(node, attribute, pattern = /^[+-]?\d+$/) {
  if (!node.hasAttribute(attribute)) return false;
  const value = node.getAttribute(attribute).trim();
  return !pattern.test(value) || Number(value.replace(/%$/, "")) !== 0;
}

function onlyAttributes(node, names) {
  return Array.from(node.attributes).every(
    (attribute) =>
      attribute.namespaceURI === "http://www.w3.org/2000/xmlns/" ||
      (!attribute.namespaceURI && names.includes(attribute.localName)),
  );
}

function plainBlipExtensions(list) {
  if (
    !is(list, NS.drawing, "extLst") ||
    !is(list.parentNode, NS.drawing, "blip") ||
    children(list.parentNode, NS.drawing, "extLst").length !== 1 ||
    !onlyAttributes(list, []) ||
    list.textContent.trim() ||
    elements(list).length > 1
  )
    return false;
  return elements(list).every((extension) => {
    const content = elements(extension);
    const dpi = content[0];
    return (
      is(extension, NS.drawing, "ext") &&
      onlyAttributes(extension, ["uri"]) &&
      [LOCAL_DPI_URI, `{${LOCAL_DPI_URI}}`].includes(
        extension.getAttribute("uri")?.toLowerCase(),
      ) &&
      content.length === 1 &&
      is(dpi, NS.drawing2010, "useLocalDpi") &&
      onlyAttributes(dpi, ["val"]) &&
      !elements(dpi).length &&
      (!dpi.hasAttribute("val") ||
        ["0", "1", "false", "true"].includes(dpi.getAttribute("val").trim()))
    );
  });
}

export function assertEditableWordPicture(xml, control) {
  // getOoxml can include Flat OPC parts (including theme defaults), even for a
  // single content control. Inspect only the uniquely identified inline picture.
  const inline = ownedInline(documentRoot(parse(xml)), control);
  const transformed = descendants(inline, NS.drawing, "xfrm").some(
    (node) =>
      nonzero(node, "rot") ||
      ["flipH", "flipV"].some(
        (name) =>
          node.hasAttribute(name) && !["0", "false"].includes(node.getAttribute(name).trim()),
      ),
  );
  const cropped = descendants(inline, NS.drawing, "srcRect").some((node) =>
    ["l", "t", "r", "b"].some((name) => nonzero(node, name, /^[+-]?\d+(?:\.\d+)?%?$/)),
  );
  const effects = descendants(inline, NS.drawing, "*").some(
    (node) =>
      EFFECTS.has(node.localName) ||
      (["effectLst", "effectDag"].includes(node.localName) && elements(node).length > 0) ||
      (node.localName === "effectRef" && (!node.hasAttribute("idx") || nonzero(node, "idx"))) ||
      (node.localName === "blip" &&
        elements(node).some((child) => !is(child, NS.drawing, "extLst"))),
  );
  // MS-ODRAWXML permits artistic effects in blip/extLst/imgProps. Only the
  // documented useLocalDpi compression flag is known nondecorative metadata.
  const extensions = descendants(inline, "*", "extLst").some((list) => !plainBlipExtensions(list));
  if (transformed || cropped || effects || extensions)
    fail(
      "UNSUPPORTED_TRANSFORM",
      "This Word picture has cropping, rotation, flipping or picture effects that cannot be safely preserved. Reset those picture effects before editing with ReShiki.",
    );
}
