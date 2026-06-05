import { chromium } from "playwright";
import fs from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const appDir = path.join(root, "tauri-bench");
const url = "http://127.0.0.1:5173/";
let server = null;

async function waitForServer() {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    try {
      const response = await fetch(url);
      if (response.ok) return true;
    } catch {
      // keep polling
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  return false;
}

if (!(await waitForServer())) {
  server = Bun.spawn(["bun", "run", "dev"], {
    cwd: appDir,
    stdout: "ignore",
    stderr: "ignore"
  });
  if (!(await waitForServer())) {
    server.kill();
    throw new Error("Vite dev server did not become ready");
  }
}

const browserCandidates = [
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
  "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"
];
const executablePath = browserCandidates.find((candidate) => fs.existsSync(candidate));
if (!executablePath) {
  throw new Error("No local Chromium-based browser found");
}

const browser = await chromium.launch({ executablePath, headless: true });
const page = await browser.newPage({ viewport: { width: 760, height: 520 } });
const failures = [];

await page.addInitScript(() => {
  const kanResults = [
    ["code-projects-kanade", "Kanade"],
    ["code-projects-kanade-acm-integration", "kanade-acm-integration"],
    ["code-projects-kanade-reference", "kanade-reference"],
    ["code-projects-kulms-ta-extension", "kulms-ta-extension"]
  ];
  const debug = ["debug-switch-to-appkit", "Switch to AppKitBench"];
  const state = {
    visible: false,
    query: "",
    selectedIndex: 0,
    totalMatches: 5,
    results: [debug, ...kanResults].map(([id, name]) => ({
      id,
      name,
      path: id === "debug-switch-to-appkit" ? "/Applications/AppKitBench.app" : `/tmp/${name}`,
      aliases: id === "debug-switch-to-appkit" ? ["-"] : [name.toLowerCase()],
      language: id === "debug-switch-to-appkit" ? "Action" : "Project",
      isDebug: id === "debug-switch-to-appkit"
    })),
    footer: "ready",
    cycleId: null
  };

  function applySearch() {
    if (state.query === "-") {
      state.results = [{
        id: debug[0],
        name: debug[1],
        path: "/Applications/AppKitBench.app",
        aliases: ["-"],
        language: "Action",
        isDebug: true
      }];
      state.totalMatches = 1;
    } else if (state.query === "kan") {
      state.results = kanResults.map(([id, name]) => ({
        id,
        name,
        path: `/tmp/${name}`,
        aliases: [name.toLowerCase()],
        language: "Project",
        isDebug: false
      }));
      state.totalMatches = state.results.length;
    } else {
      state.results = [debug, ...kanResults].map(([id, name]) => ({
        id,
        name,
        path: id === "debug-switch-to-appkit" ? "/Applications/AppKitBench.app" : `/tmp/${name}`,
        aliases: id === "debug-switch-to-appkit" ? ["-"] : [name.toLowerCase()],
        language: id === "debug-switch-to-appkit" ? "Action" : "Project",
        isDebug: id === "debug-switch-to-appkit"
      }));
      state.totalMatches = state.results.length;
    }
    state.selectedIndex = state.results.length ? 0 : -1;
    state.footer = `${state.results.length} results`;
  }

  window.__IPC_CALLS__ = [];
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    transformCallback: () => 1,
    unregisterCallback: () => {},
    runCallback: () => {},
    convertFileSrc: (filePath) => filePath,
    invoke: async (cmd, args) => {
      window.__IPC_CALLS__.push({ cmd, args });
      if (cmd === "frontend_ready") return { ...state };
      if (cmd === "palette_rendered") return null;
      if (cmd === "select_index") {
        state.selectedIndex = args.index;
        return { ...state };
      }
      if (cmd === "handle_key") {
        const key = args.input.key;
        if (key === "toggle") {
          state.visible = !state.visible;
          if (state.visible) {
            state.query = "";
            applySearch();
          }
        }
        if (key === "escape") state.visible = false;
        if (key === "char:k" || key === "char:a" || key === "char:n" || key === "char:-") {
          state.query += key.slice(5);
          applySearch();
        }
        if (key === "next") state.selectedIndex = Math.min(state.selectedIndex + 1, state.results.length - 1);
        if (key === "previous") state.selectedIndex = Math.max(state.selectedIndex - 1, 0);
        if (key === "enter") {
          const item = state.results[state.selectedIndex];
          window.__IPC_CALLS__.push({
            cmd: "open_dispatched_mock",
            args: { projectId: item.id, query: state.query, selectedIndex: state.selectedIndex }
          });
          state.visible = false;
        }
        return { ...state };
      }
      return null;
    }
  };
});

await page.goto(url, { waitUntil: "networkidle" });
await page.waitForSelector(".row:not([hidden])");

function check(condition, message, details = {}) {
  if (!condition) failures.push({ message, details });
}

async function snapshot(label) {
  return page.evaluate((labelArg) => {
    const rows = [...document.querySelectorAll(".row:not([hidden])")];
    const selected = rows.findIndex((row) => row.classList.contains("selected"));
    return {
      label: labelArg,
      query: document.querySelector("#query").textContent,
      selected,
      selectedName: selected >= 0 ? rows[selected].querySelector(".name")?.textContent : null,
      visibleSelected: document.querySelectorAll(".row.selected:not([hidden])").length,
      visible: document.querySelector("#palette").dataset.visible,
      names: rows.map((row) => row.querySelector(".name")?.textContent)
    };
  }, label);
}

await page.keyboard.press("Control+M");
let state = await snapshot("after toggle");
check(state.visible === "true", "Control+M should show the palette", state);

await page.keyboard.type("kan");
state = await snapshot("after kan");
check(state.query === "kan", "kan query should render", state);
check(state.selected === 0, "search should select the first result", state);
check(state.visibleSelected === 1, "there should be exactly one selected row", state);

await page.keyboard.press("Control+N");
await page.keyboard.press("Control+N");
state = await snapshot("after ctrl+n twice");
check(state.selected === 2, "Control+N twice should select index 2", state);
check(state.selectedName === "kanade-reference", "Control+N should move through visible results", state);

await page.keyboard.press("Enter");
let calls = await page.evaluate(() => window.__IPC_CALLS__);
let dispatched = calls.filter((call) => call.cmd === "open_dispatched_mock").at(-1);
check(dispatched?.args?.query === "kan", "Enter should dispatch current query", dispatched);
check(dispatched?.args?.selectedIndex === 2, "Enter should dispatch moved selected index", dispatched);

await page.keyboard.press("Control+M");
await page.keyboard.type("kan");
await page.keyboard.press("ArrowDown");
await page.keyboard.press("Control+P");
state = await snapshot("after arrowdown ctrl+p");
check(state.selected === 0, "ArrowDown then Control+P should return to first result", state);
await page.keyboard.press("Escape");
state = await snapshot("after escape");
check(state.visible === "false", "Escape should hide the palette", state);

await page.keyboard.press("Control+M");
await page.keyboard.type("-");
state = await snapshot("after dash");
check(state.query === "-", "Minus should be accepted as physical input", state);
check(state.selectedName === "Switch to AppKitBench", "dash alias should select debug switch", state);
await page.keyboard.press("Enter");
calls = await page.evaluate(() => window.__IPC_CALLS__);
dispatched = calls.filter((call) => call.cmd === "open_dispatched_mock").at(-1);
check(dispatched?.args?.projectId === "debug-switch-to-appkit", "dash Enter should dispatch debug switch", dispatched);

await browser.close();
if (server) server.kill();

if (failures.length) {
  console.error(JSON.stringify({ ok: false, failures }, null, 2));
  process.exit(1);
}

console.log(JSON.stringify({ ok: true }, null, 2));
