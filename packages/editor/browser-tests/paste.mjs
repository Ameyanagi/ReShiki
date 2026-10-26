// Optional real-browser regressions against a built editor. Install Playwright
// separately and set RESHIKI_PLAYWRIGHT_MODULE to its index.mjs when it is not
// installed in this checkout. Serve examples/paste.html with examples/serve.ts.
// Native Safari clipboard permission/keyboard checks supplement these events.
import assert from "node:assert/strict";

const playwright = await import(process.env.RESHIKI_PLAYWRIGHT_MODULE ?? "playwright");
const base = process.env.RESHIKI_BROWSER_URL ?? "http://localhost:8080/examples/paste.html";
const names = process.argv.slice(2);

for (const name of names.length ? names : ["webkit", "chromium", "firefox"]) {
  assert.ok(["webkit", "chromium", "firefox"].includes(name), `Unknown browser: ${name}`);
  const browser = await playwright[name].launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1400, height: 950 } });
    await page.goto(base);
    await page.waitForFunction(() => !!window.editor);
    // A permission-delayed API read reproduces Safari's old fallback ordering.
    await page.evaluate(() => {
      window.pasteReads = 0;
      navigator.clipboard.readText = () => {
        window.pasteReads += 1;
        return new Promise((resolve) => {
          window.finishRead = resolve;
        });
      };
    });
    const canvas = page.locator("canvas");
    await canvas.click({ position: { x: 400, y: 300 } });
    await page.keyboard.press("t");
    await canvas.click({ position: { x: 400, y: 300 } });
    await page.waitForFunction(() => document.activeElement?.tagName === "TEXTAREA");

    const paste = async (text) => {
      await page.evaluate(() => {
        const mac = navigator.platform.includes("Mac");
        document.activeElement.dispatchEvent(
          new KeyboardEvent("keydown", {
            key: "v",
            code: "KeyV",
            metaKey: mac,
            ctrlKey: !mac,
            bubbles: true,
            cancelable: true,
          }),
        );
      });
      // Let a legacy setTimeout(0) fallback start before the native paste.
      await page.waitForTimeout(30);
      const prevented = await page.evaluate((value) => {
        const target = document.activeElement;
        const data = new DataTransfer();
        const event = new ClipboardEvent("paste", {
          clipboardData: data,
          bubbles: true,
          cancelable: true,
        });
        // Firefox constructs a different DataTransfer for this event.
        event.clipboardData.setData("text/plain", value);
        if (event.clipboardData.getData("text/plain") !== value) {
          throw new Error("Synthetic paste fixture lost its clipboard data");
        }
        target.dispatchEvent(event);
        // Even an extra insertFromPaste must never insert the text twice.
        target.dispatchEvent(
          new InputEvent("beforeinput", {
            inputType: "insertFromPaste",
            data: value,
            bubbles: true,
            cancelable: true,
          }),
        );
        window.finishRead?.(value);
        return event.defaultPrevented;
      }, text);
      return prevented;
    };
    const caption = () =>
      page.evaluate(async () => {
        const document = JSON.parse(await window.editor.save("rsk"));
        return document.annotations.map((annotation) => annotation.text).join("|");
      });
    const prevented = await paste("caffeine 日本語");
    await page.waitForTimeout(150);
    assert.equal(
      await caption(),
      "caffeine 日本語",
      `${name}: Command-V lost the clipboard text or typed a bare V`,
    );
    assert.equal(prevented, true, `${name}: native paste insertion must be cancelled`);
    await paste(" + aspirin");
    await page.waitForFunction(async () => {
      const doc = JSON.parse(await window.editor.save("rsk"));
      return doc.annotations[0]?.text === "caffeine 日本語 + aspirin";
    });
    await paste("");
    assert.equal(
      await caption(),
      "caffeine 日本語 + aspirin",
      `${name}: empty clipboard changed text`,
    );
    assert.equal(
      await page.evaluate(() => window.pasteReads),
      0,
      `${name}: field used async readText`,
    );

    // Composition still reaches the widget independently of the paste path.
    await page.evaluate(() => {
      const target = document.activeElement;
      for (const [type, data] of [
        ["compositionstart", ""],
        ["compositionupdate", "かな"],
        ["compositionend", "かな"],
      ]) {
        target.dispatchEvent(new CompositionEvent(type, { data, bubbles: true }));
      }
    });
    await page.waitForFunction(async () => {
      const doc = JSON.parse(await window.editor.save("rsk"));
      return doc.annotations[0]?.text === "caffeine 日本語 + aspirinかな";
    });
    // No paste event/read result must leave a bare V typed into the field.
    await page.evaluate(() => {
      const mac = navigator.platform.includes("Mac");
      document.activeElement.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "v",
          code: "KeyV",
          metaKey: mac,
          ctrlKey: !mac,
          bubbles: true,
          cancelable: true,
        }),
      );
    });
    await page.waitForTimeout(50);
    assert.equal(
      await caption(),
      "caffeine 日本語 + aspirinかな",
      `${name}: shortcut typed a bare V`,
    );
    assert.equal(await page.evaluate(() => window.pasteReads), 0);
    const focusPage = await browser.newPage({ viewport: { width: 1400, height: 950 } });
    await focusPage.goto(base);
    await focusPage.waitForFunction(() => !!window.editor);
    await focusPage.evaluate(() => {
      navigator.clipboard.readText = () =>
        new Promise((resolve) => {
          window.finishRead = resolve;
        });
    });
    const focusCanvas = focusPage.locator("canvas");
    await focusCanvas.click({ position: { x: 400, y: 300 } });
    await focusPage.evaluate(() => {
      const mac = navigator.platform.includes("Mac");
      document.activeElement.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "v",
          code: "KeyV",
          metaKey: mac,
          ctrlKey: !mac,
          bubbles: true,
          cancelable: true,
        }),
      );
    });
    await focusPage.waitForFunction(() => !!window.finishRead);
    await focusPage.keyboard.press("t");
    await focusCanvas.click({ position: { x: 400, y: 300 } });
    await focusPage.waitForFunction(() => document.activeElement?.tagName === "TEXTAREA");
    await focusPage.evaluate(() => window.finishRead("CCO"));
    await focusPage.waitForTimeout(150);
    const unchanged = await focusPage.evaluate(async () =>
      JSON.parse(await window.editor.save("rsk")),
    );
    assert.equal(
      unchanged.annotations.length,
      0,
      `${name}: delayed canvas paste reached the new caption`,
    );
    assert.equal(unchanged.atoms.length, 0);
    const canvasPage = await browser.newPage({ viewport: { width: 1400, height: 950 } });
    await canvasPage.goto(base);
    await canvasPage.waitForFunction(() => !!window.editor);
    await canvasPage.evaluate(() => {
      navigator.clipboard.readText = async () => "CCO";
      navigator.clipboard.read = async () => [
        {
          types: ["text/plain"],
          getType: async () => new Blob(["CCO"], { type: "text/plain" }),
        },
      ];
    });
    await canvasPage.locator("canvas").click({ position: { x: 400, y: 300 } });
    await canvasPage.evaluate(() => {
      const mac = navigator.platform.includes("Mac");
      for (const [key, code] of [
        [mac ? "Meta" : "Control", mac ? "MetaLeft" : "ControlLeft"],
        ["v", "KeyV"],
      ]) {
        document.activeElement.dispatchEvent(
          new KeyboardEvent("keydown", {
            key,
            code,
            metaKey: mac,
            ctrlKey: !mac,
            bubbles: true,
            cancelable: true,
          }),
        );
      }
    });
    await canvasPage.waitForFunction(async () => {
      const doc = JSON.parse(await window.editor.save("rsk"));
      return doc.atoms.length === 3 && doc.bonds.length === 2;
    });
    console.log(
      `${name}: delayed/repeated/empty paste, duplicate input, IME, no-result, changed-focus and canvas drawing checks passed`,
    );
  } finally {
    await browser.close();
  }
}
