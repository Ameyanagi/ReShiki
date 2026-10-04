import { DOMParser } from "@xmldom/xmldom";

// Node has no native DOMParser. Never allow xmldom's recovery mode to make a
// malformed-XML regression pass when a browser would return a parsererror.
globalThis.DOMParser ??= class extends DOMParser {
  constructor() {
    super({
      onError(_level, message) {
        throw new Error(message);
      },
    });
  }
};
