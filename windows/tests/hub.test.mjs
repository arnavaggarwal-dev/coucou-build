// Language changes must redraw the Coucou window: fork, the settings draw all of it.
import { beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import ts from "typescript";
import { installFakeDom } from "./fakedom.mjs";
import { calls, emit, sent } from "./tauri.mjs";

// The shell imports its stylesheet; layout is checked in the browser.
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.endsWith(".css")) {
      return { url: new URL(specifier, context.parentURL).href, shortCircuit: true };
    }
    return nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    if (url.endsWith(".css")) return { format: "module", source: "", shortCircuit: true };
    // The canvas helper has a parameter property beyond Node's type stripping.
    if (url.endsWith("/hub/hero.ts")) {
      const { outputText } = ts.transpileModule(readFileSync(fileURLToPath(url), "utf8"), {
        compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
      });
      return { format: "module", source: outputText, shortCircuit: true };
    }
    return nextLoad(url, context);
  },
});

const dom = installFakeDom();
// Keep canvas animation asleep: these tests exercise the shell's DOM only.
document.visibilityState = "hidden";
document.addEventListener = () => {};
document.documentElement = { dataset: {} };
window.addEventListener = () => {};

const { h } = await import("../src/views/dom.ts");
const { onLanguageChange, setLanguage, t } = await import("../src/i18n/i18n.ts");
const { FLOW_PAGES, NOTCH_PAGES, startHub } = await import("../src/hub/shell.ts");
const pages = [...FLOW_PAGES, ...NOTCH_PAGES];
const settle = () => new Promise((resolve) => setImmediate(resolve));
/** The sidebar button of Notch → General (Mochi's room comes before it). */
const GENERAL = pages.findIndex((page) => page.id === "general");

beforeEach(() => {
  setLanguage("en");
  calls.length = 0;
});

async function openHub() {
  const root = dom.root();
  const hub = await startHub(root, {
    version: "test",
    flowPages: {},
    renderNotch: async (page, container, ctx) => {
      container.append(h("h1", { text: pages.find((p) => p.id === page).title() }), ctx.statusLine);
    },
  });
  // The settings entry point wires language changes to this public method.
  const off = onLanguageChange(() => hub.redraw());
  return { root, hub, off };
}

test("the window opens on General, with no sidebar of its own", async () => {
  const { root, hub, off } = await openHub();
  try {
    assert.equal(hub.current(), "general");
    assert.equal(root.find(".hub-side").length, 0);
    assert.equal(root.find("H1")[0].textContent, "General");
  } finally {
    off();
  }
});

test("a language change redraws the page on screen and saves nothing", async () => {
  const { root, hub, off } = await openHub();
  try {
    calls.length = 0;
    for (const language of ["fr", "ar", "en"]) {
      setLanguage(language);
      await settle();
      assert.equal(hub.current(), "general");
      assert.equal(root.find("H1")[0].textContent, t("General"));
      assert.equal(root.find(".hub-status")[0].textContent, t("Dictation is off"));
    }
    assert.deepEqual(calls, [], "redrawing must not save settings or request data");
  } finally {
    off();
  }
});

test("language redraw retains the latest dictation state and key in the status line", async () => {
  const { root, off } = await openHub();
  try {
    const status = root.find(".hub-status")[0];
    emit("flow-prefs", { enabled: true, key: "right_ctrl" });
    emit("speech-models", { parakeet: { state: "ready" } });
    const english = status.textContent;
    assert.match(english, /^Ready · hold /);
    calls.length = 0;
    setLanguage("fr");
    await settle();
    assert.notEqual(status.textContent, english);
    assert.equal(status.textContent, "Prête · maintiens Ctrl de droite");
    setLanguage("en");
    await settle();
    assert.equal(status.textContent, english);
    assert.deepEqual(sent("flow_set_prefs"), []);
    assert.deepEqual(calls, []);
  } finally {
    off();
  }
});
