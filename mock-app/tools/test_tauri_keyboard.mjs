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
  const prResults = [
    ["manual-project-manager", "Project Manager"],
    ["code-projects-agent-config-sync", "agent-config-sync"],
    ["code-projects-anki-connect-extension", "anki-connect-extension"],
    ["code-projects-anki-mcp", "anki-mcp"]
  ];
  const defaultResults = Array.from({ length: 20 }, (_, index) => [`default-${index}`, `Default Project ${index}`]);

  function toItem([id, name]) {
    return {
      id,
      name,
      path: `/tmp/${name}`,
      aliases: [name.toLowerCase()],
      language: "Project",
      isDebug: false
    };
  }

  const state = {
    visible: false,
    query: "",
    selectedIndex: 0,
    totalMatches: defaultResults.length,
    results: defaultResults.map(toItem),
    footer: "ready",
    cycleId: null
  };

  function applySearch() {
    if (state.query === "-") {
      state.results = [];
      state.totalMatches = 0;
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
    } else if (state.query === "pr") {
      state.results = prResults.map(([id, name]) => ({
        id,
        name,
        path: `/tmp/${name}`,
        aliases: [name.toLowerCase()],
        language: "Project",
        isDebug: false
      }));
      state.totalMatches = state.results.length;
    } else {
      state.results = defaultResults.map(toItem);
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
      if (cmd === "frontend_loaded") return null;
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
        if (
          key === "char:k" ||
          key === "char:a" ||
          key === "char:n" ||
          key === "char:p" ||
          key === "char:r" ||
          key === "char:-"
        ) {
          state.query += key.slice(5);
          applySearch();
        }
        if (key === "next") state.selectedIndex = Math.min(state.selectedIndex + 1, state.results.length - 1);
        if (key === "previous") state.selectedIndex = Math.max(state.selectedIndex - 1, 0);
        if (key === "enter" || key.startsWith("open:")) {
          const editor = key === "open:vscode" ? "vscode" : key === "open:antigravity" ? "antigravity" : "zed";
          const item = state.results[state.selectedIndex];
          if (item) {
            window.__IPC_CALLS__.push({
              cmd: "open_dispatched_mock",
              args: { projectId: item.id, query: state.query, selectedIndex: state.selectedIndex, editor }
            });
            state.visible = false;
          }
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

for (let index = 0; index < 8; index += 1) {
  await page.keyboard.press("ArrowDown");
}
state = await snapshot("after eight arrows");
let scrollTop = await page.$eval("#results", (element) => element.scrollTop);
check(state.selected === 8, "ArrowDown should move to visible index 8", state);
check(scrollTop === 0, "Selecting the last fully visible row should not scroll early", { scrollTop, state });

await page.keyboard.press("ArrowDown");
const scrollGeometry = await page.evaluate(() => {
  const results = document.querySelector("#results");
  const row = document.querySelectorAll(".row:not([hidden])")[9];
  const resultsRect = results.getBoundingClientRect();
  const rowRect = row.getBoundingClientRect();
  return {
    scrollTop: results.scrollTop,
    rowBottom: rowRect.bottom,
    resultsBottom: resultsRect.bottom
  };
});
check(scrollGeometry.scrollTop > 0, "Selecting the first clipped row should scroll", scrollGeometry);
check(
  scrollGeometry.rowBottom <= scrollGeometry.resultsBottom + 1,
  "Scrolled selected row should be fully visible",
  scrollGeometry
);

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
check(dispatched?.args?.editor === "zed", "Enter should dispatch zed by default", dispatched);

await page.keyboard.press("Control+M");
await page.keyboard.type("kan");
await page.keyboard.press("Control+Enter");
calls = await page.evaluate(() => window.__IPC_CALLS__);
dispatched = calls.filter((call) => call.cmd === "open_dispatched_mock").at(-1);
check(dispatched?.args?.editor === "zed", "Control+Enter should dispatch zed", dispatched);

await page.keyboard.press("Control+M");
await page.keyboard.type("kan");
await page.keyboard.press("Meta+Enter");
calls = await page.evaluate(() => window.__IPC_CALLS__);
dispatched = calls.filter((call) => call.cmd === "open_dispatched_mock").at(-1);
check(dispatched?.args?.editor === "vscode", "Command+Enter should dispatch vscode", dispatched);

await page.keyboard.press("Control+M");
await page.keyboard.type("kan");
await page.keyboard.press("Shift+Enter");
calls = await page.evaluate(() => window.__IPC_CALLS__);
dispatched = calls.filter((call) => call.cmd === "open_dispatched_mock").at(-1);
check(dispatched?.args?.editor === "antigravity", "Shift+Enter should dispatch antigravity", dispatched);

await page.keyboard.press("Control+M");
await page.keyboard.type("pr");
state = await snapshot("after pr");
check(state.query === "pr", "pr query should render", state);
check(state.names.length >= 3, "pr should keep multiple word-search results", state);
await page.keyboard.press("ArrowDown");
await page.keyboard.press("ArrowDown");
state = await snapshot("after pr arrows");
check(state.selected === 2, "ArrowDown should move after word search", state);
await page.keyboard.press("Enter");
calls = await page.evaluate(() => window.__IPC_CALLS__);
dispatched = calls.filter((call) => call.cmd === "open_dispatched_mock").at(-1);
check(dispatched?.args?.query === "pr", "Enter after word search should dispatch pr query", dispatched);
check(dispatched?.args?.selectedIndex === 2, "Enter after word search should dispatch moved index", dispatched);

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
check(state.selected === -1, "Minus should not select a debug switch in normal mode", state);
await page.keyboard.press("Enter");
calls = await page.evaluate(() => window.__IPC_CALLS__);
dispatched = calls.filter((call) => call.cmd === "open_dispatched_mock").at(-1);
check(dispatched?.args?.projectId !== "debug-switch-to-appkit", "dash Enter should not dispatch debug switch", dispatched);

await browser.close();
if (server) server.kill();

if (failures.length) {
  console.error(JSON.stringify({ ok: false, failures }, null, 2));
  process.exit(1);
}

console.log(JSON.stringify({ ok: true }, null, 2));
