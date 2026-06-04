import "./style.css";
import { Events } from "@wailsio/runtime";
import { BenchService } from "../bindings/dev.projectlauncher/wailsbench";

const search = document.querySelector("#search");
const palette = document.querySelector(".palette");
const caret = document.querySelector("#caret");
const resultsEl = document.querySelector("#results");
const footer = document.querySelector("#footer");
const runBenchmark = document.querySelector("#runBenchmark");
const measureCanvas = document.createElement("canvas");
const measureContext = measureCanvas.getContext("2d");

let indexed = [];
let results = [];
let selected = 0;
let activeCycleId = "";
let totalMatches = 0;
let aliasMap = new Map();
let queryValue = "";
let activeScenario = "";

function fuzzyContains(token, candidate) {
  let index = 0;
  for (const char of token) {
    const found = candidate.indexOf(char, index);
    if (found === -1) return false;
    index = found + 1;
  }
  return true;
}

function wordStartsWith(token, candidate) {
  return candidate.split(/[^a-z0-9]+/).some((word) => word.startsWith(token));
}

function scoreToken(token, item) {
  if (item.id === token) return 1400;
  if (token.length >= 3 && item.id.includes(token)) return 1000;
  if (item.aliasesList.includes(token)) return 1500;
  if (token.length >= 3 && item.aliases.includes(token)) return 1100;
  if (item.name.startsWith(token)) return 1200 - Math.min(item.name.length, 300);
  if (token.length === 1 && wordStartsWith(token, item.name)) return 600;
  if (token.length >= 3 && item.name.includes(token)) return 900 - Math.min(item.name.length, 250);
  if (token.length >= 3 && item.tags.includes(token)) return 700;
  if (token.length >= 3 && item.path.includes(token)) return 450;
  if (/\d/.test(token)) return 0;
  if (token.length >= 3 && fuzzyContains(token, item.aliases)) return 300;
  if (token.length >= 3 && fuzzyContains(token, item.name)) return 250;
  if (token.length >= 3 && fuzzyContains(token, item.path)) return 120;
  return 0;
}

function scenarioName(query, aliasHit) {
  const normalized = query.toLowerCase().trim();
  if (aliasHit && normalized === "a") return "alias";
  if (normalized === "pr") return "narrowing";
  return "";
}

function runSearch(query) {
  const start = performance.now();
  const normalizedQuery = query.toLowerCase().trim();
  const aliasHit = aliasMap.get(normalizedQuery);
  const scenario = scenarioName(query, aliasHit);
  activeScenario = scenario;

  if (aliasHit) {
    totalMatches = 1;
    results = [{ project: aliasHit, score: 1500 }];
    selected = 0;
  } else if (normalizedQuery.length === 0) {
    totalMatches = indexed.length;
    results = indexed.slice(0, 50).map((item) => ({ project: item.project, score: 0 }));
  } else {
    const tokens = normalizedQuery.split(/\s+/).filter(Boolean);
    const matches = [];
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

  selected = results.length ? Math.min(selected, results.length - 1) : 0;
  renderResults();
  const duration = performance.now() - start;
  footer.textContent = aliasHit
    ? `alias hit - ${aliasHit.name} - search ${duration.toFixed(3)} ms`
    : `showing ${results.length} of ${totalMatches} matches - search ${duration.toFixed(3)} ms`;
  void BenchService.LogEvent("search_completed", activeCycleId, {
    metric: "search_ms",
    duration_ms: duration,
    query,
    result_count: results.length,
    alias_hit: aliasHit ? normalizedQuery : "",
    scenario
  });
  return { duration, scenario };
}

function renderResults() {
  resultsEl.textContent = "";
  for (const [index, result] of results.entries()) {
    const row = document.createElement("div");
    row.className = `row${index === selected ? " selected" : ""}`;
    row.style.animationDelay = `${index * 12}ms`;
    row.innerHTML = `<div class="name"></div><div class="meta"></div>`;
    row.querySelector(".name").textContent = result.project.name;
    row.querySelector(".meta").textContent = `${result.project.id} - ${result.project.language} - ${result.project.path}`;
    row.addEventListener("click", () => {
      selected = index;
      renderResults();
    });
    row.addEventListener("dblclick", () => {
      selected = index;
      void openSelected();
    });
    resultsEl.append(row);
  }
}

async function openSelected() {
  if (!results.length) return;
  const project = results[selected].project;
  footer.textContent = `opening ${project.name}`;
  await BenchService.OpenProject(activeCycleId, project.path, project.id, activeScenario, queryValue, selected);
  await closePalette();
}

async function closePalette() {
  await BenchService.Hide();
}

function normalizeSearchInput(value) {
  return value.normalize("NFKC").replace(/[^\x20-\x7E]/g, "");
}

function setQuery(value, cursor = value.length) {
  queryValue = normalizeSearchInput(value);
  search.value = queryValue;
  const next = Math.max(0, Math.min(cursor, queryValue.length));
  search.setSelectionRange(next, next);
  updateCaret(next);
}

function updateCaret(cursor = queryValue.length) {
  const style = getComputedStyle(search);
  measureContext.font = style.font;
  const paddingLeft = Number.parseFloat(style.paddingLeft) || 16;
  const text = queryValue.slice(0, cursor);
  caret.style.setProperty(
    "--caret-left",
    `${paddingLeft + measureContext.measureText(text).width + 1}px`
  );
}

function asciiFromCode(event) {
  if (event.metaKey || event.ctrlKey || event.altKey) return "";
  if (/^Key[A-Z]$/.test(event.code)) return event.code.slice(3).toLowerCase();
  if (/^Digit[0-9]$/.test(event.code)) return event.code.slice(5);
  if (event.code === "Space") return " ";
  return "";
}

function updateQueryFromKeyboard(event) {
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
    void BenchService.LogEvent("input_processed", activeCycleId, {
      metric: "input_to_result_ms",
      duration_ms: 0,
      query: queryValue,
      result_count: results.length,
      scenario: result.scenario || activeScenario
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
  void BenchService.LogEvent("input_processed", activeCycleId, {
    metric: "input_to_result_ms",
    duration_ms: performance.now() - inputStart,
    query: queryValue,
    result_count: results.length,
    scenario: result.scenario || activeScenario
  });
  return true;
}

function moveSelection(offset) {
  if (!results.length) return;
  const start = performance.now();
  selected = Math.max(0, Math.min(selected + offset, results.length - 1));
  activeScenario = "navigation";
  renderResults();
  void BenchService.LogEvent("selection_moved", activeCycleId, {
    metric: "selection_move_ms",
    duration_ms: performance.now() - start,
    query: queryValue,
    selected_index: selected,
    scenario: activeScenario
  });
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
  if (event.key === "Escape") {
    event.preventDefault();
    void closePalette();
  } else if (event.key === "Enter") {
    event.preventDefault();
    void openSelected();
  } else if (event.ctrlKey && !event.metaKey && !event.altKey && event.code === "KeyN") {
    event.preventDefault();
    moveSelection(1);
  } else if (event.ctrlKey && !event.metaKey && !event.altKey && event.code === "KeyP") {
    event.preventDefault();
    moveSelection(-1);
  } else if (event.key === "ArrowDown") {
    event.preventDefault();
    moveSelection(1);
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    moveSelection(-1);
  } else if (updateQueryFromKeyboard(event)) {
    return;
  }
});

document.addEventListener(
  "keydown",
  (event) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopImmediatePropagation();
    void closePalette();
  },
  true
);

runBenchmark.addEventListener("click", () => BenchService.RunBenchmark());

Events.On("show_palette", async (event) => {
  activeCycleId = event.data.cycle_id;
  activeScenario = "";
  setQuery("");
  selected = 0;
  runSearch(queryValue);
  palette.classList.add("active");
  search.focus();
  await new Promise((resolve) => requestAnimationFrame(resolve));
  await BenchService.LogEvent("palette_rendered", activeCycleId, {});
});

Events.On("benchmark_query", async (event) => {
  activeCycleId = event.data.cycle_id;
  activeScenario = "";
  setQuery(event.data.query);
  selected = 0;
  runSearch(queryValue);
  palette.classList.add("active");
  await new Promise((resolve) => requestAnimationFrame(resolve));
  await BenchService.LogEvent("palette_rendered", activeCycleId, {});
});

const projects = await BenchService.LoadProjects();
indexed = projects.map((project) => ({
  project: { ...project, aliases: project.aliases ?? [] },
  id: project.id.toLowerCase(),
  name: project.name.toLowerCase(),
  path: project.path.toLowerCase(),
  tags: project.tags.join(" ").toLowerCase(),
  aliases: (project.aliases ?? []).join(" ").toLowerCase(),
  aliasesList: (project.aliases ?? []).map((alias) => alias.toLowerCase())
}));
aliasMap = new Map();
for (const item of indexed) {
  for (const alias of item.aliasesList) {
    aliasMap.set(alias, item.project);
  }
}
runSearch("");
