import { validateEnvelope } from "../protocol.js";
import { createWordAdapter } from "./word.js";
import { createExcelAdapter } from "./excel.js";
import { createPowerPointAdapter } from "./powerpoint.js";
import { fail, HostAdapterError, objectId, validateToken } from "./common.js";

export { HostAdapterError } from "./common.js";

const REQUIREMENTS = {
  Word: {
    name: "WordApi",
    version: "1.4",
    minimum: "Word for Microsoft 365 2208 on Windows or 16.64 on Mac",
  },
  Excel: {
    name: "ExcelApi",
    version: "1.19",
    minimum: "Excel for Microsoft 365 2504 on Windows or 16.96 on Mac",
  },
  PowerPoint: {
    name: "PowerPointApi",
    version: "1.8",
    minimum: "PowerPoint for Microsoft 365 2504 on Windows or 16.96 on Mac",
  },
};

export function createHostAdapter({ Office, Word, Excel, PowerPoint }) {
  const host = Office?.context?.host;
  const requirement = REQUIREMENTS[host];
  const sessionId = objectId();
  const implementations = {
    Word: createWordAdapter,
    Excel: createExcelAdapter,
    PowerPoint: createPowerPointAdapter,
  };
  const runtime = { Word, Excel, PowerPoint }[host];
  const implementation =
    requirement && runtime?.run ? implementations[host](runtime, { sessionId, Office }) : null;
  let busy = false;

  async function checkSupport() {
    if (
      !requirement ||
      !implementation ||
      !Office?.context?.requirements?.isSetSupported(requirement.name, requirement.version)
    ) {
      fail(
        "UNSUPPORTED_HOST",
        requirement
          ? `Editable ReShiki drawings require ${requirement.name} ${requirement.version} (${requirement.minimum} or later). Update Office before inserting or editing.`
          : "Editable ReShiki drawings are supported in Word, Excel and PowerPoint. Open this add-in in a supported Office application.",
      );
    }
    const canEdit =
      host !== "Excel" || Office.context.requirements.isSetSupported("ExcelApiDesktop", "1.1");
    return {
      host,
      requirement: requirement.name,
      version: requirement.version,
      canEdit,
      ...(canEdit
        ? {}
        : {
            editReason:
              "Editing existing Excel drawings requires ExcelApiDesktop 1.1 (Microsoft 365 2509 on Windows or 16.102 on Mac). You can still insert or copy editable drawings.",
          }),
    };
  }

  async function invoke(method, args) {
    await checkSupport();
    if (busy)
      fail("BUSY", "Another ReShiki document operation is still running. Wait for it to finish.");
    busy = true;
    try {
      if (method === "read" || method === "update")
        args[0] = validateToken(args[0], host, sessionId);
      if (method === "insert") args[0] = await validateEnvelope(args[0]);
      if (method === "update") args[1] = await validateEnvelope(args[1]);
      return await implementation[method](...args);
    } catch (error) {
      if (error instanceof HostAdapterError) throw error;
      throw new HostAdapterError(
        "HOST_ERROR",
        `Office could not ${method === "readSelected" || method === "read" ? "read" : "update"} the ReShiki drawing: ${error.message || error}`,
        {},
        error,
      );
    } finally {
      busy = false;
    }
  }

  return {
    host,
    checkSupport,
    insert: (envelope) => invoke("insert", [envelope]),
    readSelected: () => invoke("readSelected", []),
    read: (target) => invoke("read", [target]),
    update: (target, envelope) => invoke("update", [target, envelope]),
  };
}
