import "./styles.css";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

type Project = {
  id: string;
  name: string;
  path: string;
  openPaths?: string[];
  aliases?: string[];
  tags: string[];
  language: string;
  lastOpenedAt: string;
};

type IndexedProject = {
  project: Project;
  id: string;
  name: string;
  path: string;
  aliases: string;
  tags: string;
};

type SearchResult = {
  project: Project;
  score: number;
};

const debugSwitchProject: Project = {
  id: "debug-switch-to-appkit",
  name: "Switch to AppKitBench",
  path: "/Applications/AppKitBench.app",
  openPaths: [],
  aliases: ["-"],
  tags: ["debug", "switch"],
  language: "Action",
  lastOpenedAt: ""
};

const search = document.querySelector<HTMLInputElement>("#search")!;
const palette = document.querySelector<HTMLElement>(".palette")!;
const caret = document.querySelector<HTMLElement>("#caret")!;
const resultsEl = document.querySelector<HTMLElement>("#results")!;
const footer = document.querySelector<HTMLElement>("#footer")!;
const measureCanvas = document.createElement("canvas");
const measureContext = measureCanvas.getContext("2d")!;

let indexed: IndexedProject[] = [];
let aliasMap = new Map<string, Project>();
let results: SearchResult[] = [];
let selected = 0;
let activeCycleId: string | null = null;
let totalMatches = 0;
let queryValue = "";
let activeScenario = "";
const rowEls: HTMLElement[] = [];
let lastRenderedSelected = -1;


function fuzzyContains(token: string, candidate: string): boolean {
  let index = 0;
  for (const char of token) {
    const found = candidate.indexOf(char, index);
    if (found === -1) return false;
    index = found + 1;
  }
  return true;
}

function wordStartsWith(token: string, candidate: string): boolean {
  return candidate.split(/[^a-z0-9]+/).some((word) => word.startsWith(token));
}

function scoreToken(token: string, item: IndexedProject): number {
  if (item.aliases.split(" ").includes(token)) return 1500;
  if (item.id === token) return 1400;
  if (token.length >= 3 && item.id.includes(token)) return 1000;
  if (item.name.startsWith(token)) return 1200 - Math.min(item.name.length, 300);
  if (token.length === 1 && wordStartsWith(token, item.name)) return 600;
  if (token.length >= 3 && item.name.includes(token)) return 900 - Math.min(item.name.length, 250);
  if (token.length >= 3 && item.aliases.includes(token)) return 1100;
  if (token.length >= 3 && item.tags.includes(token)) return 700;
  if (token.length >= 3 && item.path.includes(token)) return 450;
  if (/\d/.test(token)) return 0;
  if (token.length >= 3 && fuzzyContains(token, item.name)) return 250;
  if (token.length >= 3 && fuzzyContains(token, item.path)) return 120;
  return 0;
}

function scenarioName(query: string, aliasHit: Project | undefined): string {
  const normalized = query.toLowerCase().trim();
  if (aliasHit && normalized === "a") return "alias";
  if (normalized === "pr") return "narrowing";
  return "";
}

function runSearch(query: string): { duration: number; scenario: string } {
  const start = performance.now();
  const normalizedQuery = query.toLowerCase().trim();
  const aliasHit = aliasMap.get(normalizedQuery);
  const tokens = normalizedQuery.split(/\s+/).filter(Boolean);
  const scenario = scenarioName(query, aliasHit);
  activeScenario = scenario;

  if (aliasHit) {
    totalMatches = 1;
    results = [{ project: aliasHit, score: 1500 }];
    selected = 0;
  } else if (tokens.length === 0) {
    totalMatches = indexed.length;
    results = indexed.slice(0, 50).map((item) => ({ project: item.project, score: 0 }));
  } else {
    const matches: SearchResult[] = [];
    for (const item of indexed) {
      let total = 0;
      let matched = true;
      for (const token of tokens) {
        const score = scoreToken(token, item);
        if (score === 0) {
          matched = false;
          break;
        }
        total += score;
      }
      if (matched) matches.push({ project: item.project, score: total });
    }
    matches.sort((a, b) => b.score - a.score || a.project.name.localeCompare(b.project.name));
    totalMatches = matches.length;
    results = matches.slice(0, 50);
  }

  selected = results.length ? 0 : -1;
  renderResults();
  const duration = performance.now() - start;
  footer.textContent = aliasHit
    ? `alias hit - ${aliasHit.name} - search ${duration.toFixed(3)} ms`
    : `showing ${results.length} of ${totalMatches} matches - search ${duration.toFixed(3)} ms`;
  void invoke("log_event", {
    event: "search_completed",
    cycleId: activeCycleId,
    fields: {
      metric: "search_ms",
      duration_ms: duration,
      query,
      result_count: results.length,
      alias_hit: aliasHit ? normalizedQuery : "",
      scenario
    }
  });
  return { duration, scenario };
}

function renderResults(): void {
  ensureRows();
  for (let index = 0; index < rowEls.length; index += 1) {
    const row = rowEls[index]!;
    row.classList.remove("selected");
    const result = results[index];
    if (!result) {
      row.hidden = true;
      continue;
    }
    row.hidden = false;
    row.querySelector(".name")!.textContent = result.project.name;
    const aliasesEl = row.querySelector(".aliases")!;
    aliasesEl.textContent = "";
    const displayedAliases = (result.project.aliases ?? []).slice(0, 3);
    for (const alias of displayedAliases) {
      const pill = document.createElement("span");
      pill.className = "alias-pill";
      pill.textContent = alias;
      aliasesEl.append(pill);
    }
    if ((result.project.aliases ?? []).length > 3) {
      const more = document.createElement("span");
      more.className = "alias-pill more";
      more.textContent = `+${result.project.aliases!.length - 3}`;
      aliasesEl.append(more);
    }
    row.querySelector(".meta")!.textContent = `${result.project.language} · ${result.project.path}`;
  }
  lastRenderedSelected = -1;
  renderSelection();
}

function ensureRows(): void {
  while (rowEls.length < 50) {
    const index = rowEls.length;
    const row = document.createElement("div");
    row.className = "row";
    row.innerHTML = `<div class="row-main"><span class="name"></span><span class="aliases"></span></div><div class="meta"></div>`;
    row.addEventListener("click", () => {
      selected = index;
      renderSelection();
    });
    row.addEventListener("dblclick", () => {
      selected = index;
      void openSelected();
    });
    rowEls.push(row);
    resultsEl.append(row);
  }
}

function renderSelection(): void {
  if (lastRenderedSelected >= 0 && lastRenderedSelected !== selected) {
    rowEls[lastRenderedSelected]?.classList.remove("selected");
  }
  if (selected >= 0) {
    const row = rowEls[selected];
    if (row) {
      row.classList.add("selected");
      const container = resultsEl;
      const containerTop = container.scrollTop;
      const containerBottom = containerTop + container.clientHeight;
      const rowTop = row.offsetTop;
      const rowBottom = rowTop + row.offsetHeight;
      if (rowTop < containerTop) {
        container.scrollTop = rowTop - 4;
      } else if (rowBottom > containerBottom) {
        container.scrollTop = rowBottom - container.clientHeight + 4;
      }
    }
  }
  lastRenderedSelected = selected;
}

async function openSelected(): Promise<void> {
  if (!results.length) return;
  const selectedIndex = Math.max(0, Math.min(selected, results.length - 1));
  const project = results[selectedIndex]!.project;
  footer.textContent = `opening ${project.name}`;
  await invoke("open_project", {
    cycleId: activeCycleId,
    path: project.path,
    openPaths: project.openPaths ?? [],
    projectId: project.id,
    scenario: activeScenario,
    query: queryValue,
    selectedIndex
  });
  await closePalette();
}

async function closePalette(): Promise<void> {
  palette.classList.remove("active");
  await invoke("close_palette_command").catch(() => getCurrentWindow().hide());
}

function normalizeSearchInput(value: string): string {
  return value.normalize("NFKC").replace(/[^\x20-\x7E]/g, "");
}

function setQuery(value: string, cursor = value.length): void {
  queryValue = normalizeSearchInput(value);
  search.value = queryValue;
  const next = Math.max(0, Math.min(cursor, queryValue.length));
  search.setSelectionRange(next, next);
  updateCaret(next);
}

function updateCaret(cursor = queryValue.length): void {
  const style = getComputedStyle(search);
  measureContext.font = style.font;
  const paddingLeft = Number.parseFloat(style.paddingLeft) || 14;
  const text = queryValue.slice(0, cursor);
  caret.style.setProperty(
    "--caret-left",
    `${paddingLeft + measureContext.measureText(text).width + 1}px`
  );
}

function asciiFromCode(event: KeyboardEvent): string {
  if (event.metaKey || event.ctrlKey || event.altKey) return "";
  if (/^Key[A-Z]$/.test(event.code)) return event.code.slice(3).toLowerCase();
  if (/^Digit[0-9]$/.test(event.code)) return event.code.slice(5);
  if (event.code === "Minus") return "-";
  if (event.code === "Space") return " ";
  return "";
}

function updateQueryFromKeyboard(event: KeyboardEvent): boolean {
  if (!event.metaKey && !event.ctrlKey && !event.altKey && (event.key === "Backspace" || event.key === "Delete")) {
    const start = search.selectionStart ?? queryValue.length;
    const end = search.selectionEnd ?? queryValue.length;
    let nextValue = queryValue;
    let nextCursor = start;
    if (start !== end) {
      nextValue = `${queryValue.slice(0, start)}${queryValue.slice(end)}`;
    } else if (event.key === "Backspace" && start > 0) {
      nextValue = `${queryValue.slice(0, start - 1)}${queryValue.slice(start)}`;
      nextCursor = start - 1;
    } else if (event.key === "Delete" && start < queryValue.length) {
      nextValue = `${queryValue.slice(0, start)}${queryValue.slice(start + 1)}`;
    } else {
      return true;
    }

    event.preventDefault();
    setQuery(nextValue, nextCursor);
    const result = runSearch(queryValue);
    void invoke("log_event", {
      event: "input_processed",
      cycleId: activeCycleId,
      fields: {
        metric: "input_to_result_ms",
        duration_ms: 0,
        query: queryValue,
        result_count: results.length,
        scenario: result.scenario || activeScenario
      }
    });
    return true;
  }

  const char = asciiFromCode(event);
  if (!char) return false;

  const inputStart = performance.now();
  event.preventDefault();
  const start = search.selectionStart ?? queryValue.length;
  const end = search.selectionEnd ?? queryValue.length;
  setQuery(`${queryValue.slice(0, start)}${char}${queryValue.slice(end)}`, start + char.length);
  const result = runSearch(queryValue);
  void invoke("log_event", {
    event: "input_processed",
    cycleId: activeCycleId,
    fields: {
      metric: "input_to_result_ms",
      duration_ms: performance.now() - inputStart,
      query: queryValue,
      result_count: results.length,
      scenario: result.scenario || activeScenario
    }
  });
  return true;
}

function moveSelection(offset: number): void {
  if (!results.length) return;
  const start = performance.now();
  selected = Math.max(0, Math.min(selected + offset, results.length - 1));
  activeScenario = "navigation";
  renderSelection();
  void invoke("log_event", {
    event: "selection_moved",
    cycleId: activeCycleId,
    fields: {
      metric: "selection_move_ms",
      duration_ms: performance.now() - start,
      query: queryValue,
      selected_index: selected,
      scenario: activeScenario
    }
  });
}

function handleLauncherKey(event: KeyboardEvent): boolean {
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopImmediatePropagation();
    void closePalette();
    return true;
  }
  if (event.key === "Enter") {
    event.preventDefault();
    event.stopImmediatePropagation();
    void openSelected();
    return true;
  }
  if (event.ctrlKey && !event.metaKey && !event.altKey && event.code === "KeyM") {
    event.preventDefault();
    event.stopImmediatePropagation();
    void closePalette();
    return true;
  }
  if (event.metaKey && !event.ctrlKey && !event.altKey && (event.key === "," || event.code === "Comma")) {
    event.preventDefault();
    event.stopImmediatePropagation();
    void invoke("open_settings_window");
    return true;
  }
  if (event.ctrlKey && !event.metaKey && !event.altKey && event.code === "KeyN") {
    event.preventDefault();
    event.stopImmediatePropagation();
    moveSelection(1);
    return true;
  }
  if (event.ctrlKey && !event.metaKey && !event.altKey && event.code === "KeyP") {
    event.preventDefault();
    event.stopImmediatePropagation();
    moveSelection(-1);
    return true;
  }
  if (event.key === "ArrowDown") {
    event.preventDefault();
    event.stopImmediatePropagation();
    moveSelection(1);
    return true;
  }
  if (event.key === "ArrowUp") {
    event.preventDefault();
    event.stopImmediatePropagation();
    moveSelection(-1);
    return true;
  }
  if (updateQueryFromKeyboard(event)) {
    event.stopImmediatePropagation();
    return true;
  }
  return false;
}

search.readOnly = true;
search.setAttribute("inputmode", "none");
search.addEventListener(
  "beforeinput",
  (event) => {
    event.preventDefault();
    event.stopImmediatePropagation();
  },
  true
);
search.addEventListener("compositionstart", (event) => event.preventDefault(), true);
search.addEventListener("compositionupdate", (event) => event.preventDefault(), true);
search.addEventListener("compositionend", (event) => event.preventDefault(), true);
search.addEventListener("input", () => setQuery(queryValue), true);
search.addEventListener("keydown", (event) => {
  handleLauncherKey(event);
});

document.addEventListener(
  "keydown",
  (event) => {
    handleLauncherKey(event);
  },
  true
);

window.__PROJECT_LAUNCHER_SHOW = async (payload) => {
  activeCycleId = payload.cycle_id;
  activeScenario = "";
  const needsReset = queryValue.length > 0 || results.length === 0;
  setQuery("");
  selected = 0;
  if (needsReset) {
    runSearch(queryValue);
  } else {
    renderSelection();
  }
  palette.classList.add("active");
  search.focus();
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  await invoke("log_event", { event: "palette_rendered", cycleId: activeCycleId, fields: {} });
};

window.__PROJECT_LAUNCHER_BENCHMARK = async (payload) => {
  activeCycleId = payload.cycle_id;
  activeScenario = "";
  setQuery(payload.query);
  runSearch(queryValue);
  palette.classList.add("active");
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  await invoke("log_event", { event: "palette_rendered", cycleId: activeCycleId, fields: {} });
};

function rebuildIndex(projects: Project[]): void {
  indexed = [debugSwitchProject, ...projects].map((project) => ({
    project: { ...project, aliases: project.aliases ?? [], openPaths: project.openPaths ?? [] },
    id: project.id.toLowerCase(),
    name: project.name.toLowerCase(),
    path: project.path.toLowerCase(),
    aliases: (project.aliases ?? []).join(" ").toLowerCase(),
    tags: project.tags.join(" ").toLowerCase()
  }));
  aliasMap = new Map();
  for (const item of indexed) {
    for (const alias of item.project.aliases ?? []) {
      const key = alias.toLowerCase().trim();
      if (key && !aliasMap.has(key)) aliasMap.set(key, item.project);
    }
  }
}

async function reloadProjects(): Promise<void> {
  const projects = await loadProjectsWithRetry();
  rebuildIndex(projects);
}

async function loadProjectsWithRetry(): Promise<Project[]> {
  let lastError: unknown = null;
  for (let attempt = 0; attempt < 20; attempt += 1) {
    try {
      return (await invoke("load_projects")) as Project[];
    } catch (error) {
      lastError = error;
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
  }

  footer.textContent = `failed to load projects: ${String(lastError)}`;
  await invoke("log_event", {
    event: "frontend_error",
    cycleId: null,
    fields: { message: String(lastError) }
  }).catch(() => {});
  return [];
}

window.__PROJECT_LAUNCHER_RELOAD = async () => {
  await reloadProjects();
  setQuery("");
  selected = 0;
  runSearch("");
};

await reloadProjects();
runSearch("");
search.focus();
palette.classList.add("active");
await invoke("log_event", {
  event: "app_ready",
  cycleId: null,
  fields: { project_count: indexed.length, source: "frontend" }
});
