import "./styles.css";
import { invoke } from "@tauri-apps/api/core";

type ViewItem = {
  id: string;
  name: string;
  path: string;
  aliases: string[];
  language: string;
  isDebug: boolean;
};

type ViewState = {
  visible: boolean;
  query: string;
  selectedIndex: number;
  totalMatches: number;
  results: ViewItem[];
  footer: string;
  cycleId?: string | null;
};

declare global {
  interface Window {
    __TAURI_BENCH_APPLY_STATE?: (state: ViewState) => void;
  }
}

const palette = document.querySelector<HTMLElement>("#palette")!;
const keyCatcher = document.querySelector<HTMLInputElement>("#key-catcher")!;
const queryEl = document.querySelector<HTMLElement>("#query")!;
const resultsEl = document.querySelector<HTMLElement>("#results")!;
const footerEl = document.querySelector<HTMLElement>("#footer")!;
const rows: HTMLElement[] = [];

let currentState: ViewState | null = null;
let keyQueue = Promise.resolve();
let lastRenderedCycleId: string | null = null;
const scrollEpsilon = 1;
const scrollGutter = 2;

function ensureRows(): void {
  while (rows.length < 50) {
    const index = rows.length;
    const row = document.createElement("div");
    row.className = "row";
    row.innerHTML = `<div class="row-main"><span class="name"></span><span class="aliases"></span></div><div class="meta"></div>`;
    row.addEventListener("click", () => {
      keyQueue = keyQueue.then(async () => {
        const state = await invoke<ViewState>("select_index", { index });
        applyState(state);
      });
    });
    row.addEventListener("dblclick", () => {
      sendKey("enter");
    });
    rows.push(row);
    resultsEl.append(row);
  }
}

function applyState(state: ViewState): void {
  const applyStartedAt = performance.now();
  currentState = state;
  palette.dataset.visible = state.visible ? "true" : "false";
  keyCatcher.value = state.query;
  queryEl.textContent = state.query;
  footerEl.textContent = state.footer;
  ensureRows();

  for (let index = 0; index < rows.length; index += 1) {
    const row = rows[index]!;
    const item = state.results[index];
    row.classList.toggle("selected", index === state.selectedIndex);
    if (!item) {
      row.hidden = true;
      continue;
    }
    row.hidden = false;
    row.querySelector(".name")!.textContent = item.name;
    row.querySelector(".meta")!.textContent = `${item.language} - ${item.path}`;
    const aliases = row.querySelector(".aliases")!;
    aliases.textContent = "";
    for (const alias of item.aliases.slice(0, 3)) {
      const pill = document.createElement("span");
      pill.className = "alias";
      pill.textContent = alias;
      aliases.append(pill);
    }
  }

  const selectedRow = rows[state.selectedIndex];
  if (selectedRow && !selectedRow.hidden) {
    const rowRect = selectedRow.getBoundingClientRect();
    const resultsRect = resultsEl.getBoundingClientRect();
    if (rowRect.top < resultsRect.top - scrollEpsilon) {
      resultsEl.scrollTop -= resultsRect.top - rowRect.top + scrollGutter;
    } else if (rowRect.bottom > resultsRect.bottom + scrollEpsilon) {
      resultsEl.scrollTop += rowRect.bottom - resultsRect.bottom + scrollGutter;
    }
  }

  if (state.visible) {
    keyCatcher.focus();
    if (state.cycleId && state.cycleId !== lastRenderedCycleId) {
      lastRenderedCycleId = state.cycleId;
      requestAnimationFrame(() => {
        void invoke("palette_rendered", {
          cycleId: state.cycleId,
          frontendApplyToRenderMs: performance.now() - applyStartedAt
        });
      });
    }
  }
}

function keyFromEvent(event: KeyboardEvent): string | null {
  const ctrlOnly = event.ctrlKey && !event.metaKey && !event.altKey;
  const metaOnly = event.metaKey && !event.ctrlKey && !event.altKey;
  const plain = !event.metaKey && !event.ctrlKey && !event.altKey;

  if (event.key === "Escape") return "escape";
  if (event.key === "Enter") return "enter";
  if (ctrlOnly && event.code === "KeyM") return "toggle";
  if (metaOnly && event.code === "Comma") return "settings";
  if (ctrlOnly && event.code === "KeyN") return "next";
  if (ctrlOnly && event.code === "KeyP") return "previous";
  if (event.code === "ArrowDown") return "next";
  if (event.code === "ArrowUp") return "previous";
  if (!plain) return null;
  if (event.code === "Backspace") return "backspace";
  if (event.code === "Delete") return "delete";
  if (/^Key[A-Z]$/.test(event.code)) return `char:${event.code.slice(3).toLowerCase()}`;
  if (/^Digit[0-9]$/.test(event.code)) return `char:${event.code.slice(5)}`;
  if (event.code === "Space") return "char: ";
  if (event.code === "Minus") return "char:-";
  return null;
}

function sendKey(key: string): void {
  keyQueue = keyQueue
    .then(async () => {
      const state = await invoke<ViewState>("handle_key", { input: { key } });
      applyState(state);
    })
    .catch((error) => {
      footerEl.textContent = `error: ${String(error)}`;
    });
}

function handleKeydown(event: KeyboardEvent): void {
  const key = keyFromEvent(event);
  if (!key) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  sendKey(key);
}

keyCatcher.addEventListener("keydown", handleKeydown, true);
document.addEventListener("keydown", handleKeydown, true);

document.addEventListener("beforeinput", (event) => event.preventDefault(), true);
document.addEventListener("compositionstart", (event) => event.preventDefault(), true);
document.addEventListener("compositionupdate", (event) => event.preventDefault(), true);
document.addEventListener("compositionend", (event) => event.preventDefault(), true);

window.__TAURI_BENCH_APPLY_STATE = applyState;

const initialState = await invoke<ViewState>("frontend_ready");
applyState(initialState);
await invoke("palette_rendered", { cycleId: null });
await invoke("frontend_loaded");
