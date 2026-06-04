import { chromium } from "playwright";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const appDir = path.join(root, "tauri-bench");
const url = "http://127.0.0.1:5173/";
const configPath = path.join(os.homedir(), ".project-manager.json");
const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
const projects = config.projects;

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
    stdout: "pipe",
    stderr: "pipe",
  });
  if (!(await waitForServer())) {
    server.kill();
    throw new Error("Vite dev server did not become ready");
  }
}

const browserPathCandidates = [
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
  "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
];
const executablePath = browserPathCandidates.find((candidate) => fs.existsSync(candidate));
if (!executablePath) {
  throw new Error("No local Chromium-based browser found");
}

const failures = [];
const browser = await chromium.launch({ executablePath, headless: true });
const page = await browser.newPage({ viewport: { width: 760, height: 520 } });

await page.addInitScript((projectsArg) => {
  const callbacks = new Map();
  let nextCallback = 1;
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    callbacks,
    transformCallback(callback) {
      const id = nextCallback;
      nextCallback += 1;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback(id) {
      callbacks.delete(id);
    },
    runCallback(id, args) {
      callbacks.get(id)?.(args);
    },
    convertFileSrc(filePath) {
      return filePath;
    },
    invoke: async (cmd, args) => {
      window.__IPC_CALLS__ = window.__IPC_CALLS__ || [];
      window.__IPC_CALLS__.push({ cmd, args });
      if (cmd === "load_projects") return projectsArg;
      return null;
    },
  };
}, projects);

await page.goto(url, { waitUntil: "networkidle" });
await page.waitForSelector(".row:not([hidden])");

async function showFresh() {
  await page.evaluate(() =>
    window.__PROJECT_LAUNCHER_SHOW?.({ cycle_id: `test-${Date.now()}`, source: "test" })
  );
  await page.waitForTimeout(20);
}

async function snapshot(label) {
  return page.evaluate((labelArg) => {
    const rows = [...document.querySelectorAll(".row:not([hidden])")];
    const selectedRows = [...document.querySelectorAll(".row.selected:not([hidden])")];
    const selected = rows.findIndex((row) => row.classList.contains("selected"));
    return {
      label: labelArg,
      query: document.querySelector("#search").value,
      selected,
      visibleSelected: selectedRows.length,
      selectedName: selected >= 0 ? rows[selected].querySelector(".name")?.textContent : null,
      names: rows.slice(0, 8).map((row) => row.querySelector(".name")?.textContent),
    };
  }, label);
}

function check(condition, message, details = {}) {
  if (!condition) failures.push({ message, details });
}

await showFresh();
await page.keyboard.type("kan");
let state = await snapshot("kan");
check(state.query === "kan", "kan query should be accepted", state);
check(state.selected === 0, "search should select the first result", state);
check(state.visibleSelected === 1, "there should be exactly one selected visible row", state);
check(state.names.length > 3, "kan should have multiple candidates", state);

await page.keyboard.press("Control+N");
state = await snapshot("kan ctrl+n");
check(state.selected === 1, "Control+N should move selection down", state);

await page.keyboard.press("Control+N");
state = await snapshot("kan ctrl+n ctrl+n");
check(state.selected === 2, "second Control+N should move selection down again", state);

await page.keyboard.press("ArrowDown");
state = await snapshot("kan arrowdown");
check(state.selected === 3, "ArrowDown should move selection down", state);

await page.keyboard.press("Control+P");
state = await snapshot("kan ctrl+p");
check(state.selected === 2, "Control+P should move selection up", state);

await page.keyboard.press("Enter");
let openProject = await page.evaluate(() => window.__IPC_CALLS__.filter((call) => call.cmd === "open_project").at(-1));
check(openProject?.args?.query === "kan", "Enter should dispatch the searched query", openProject);
check(openProject?.args?.selectedIndex === 2, "Enter should dispatch the moved selection index", openProject);

await showFresh();
await page.keyboard.type("-");
state = await snapshot("dash alias");
check(state.query === "-", "Minus should be accepted as query input", state);
check(state.selectedName === "Switch to AppKitBench", "dash alias should select the debug switch card", state);
await page.keyboard.press("Enter");
openProject = await page.evaluate(() => window.__IPC_CALLS__.filter((call) => call.cmd === "open_project").at(-1));
check(openProject?.args?.projectId === "debug-switch-to-appkit", "dash alias should dispatch debug switch", openProject);

await browser.close();
if (server) server.kill();

if (failures.length) {
  console.error(JSON.stringify({ ok: false, failures }, null, 2));
  process.exit(1);
}

console.log(JSON.stringify({ ok: true, cases: ["kan navigation", "enter dispatch", "dash alias"] }, null, 2));
