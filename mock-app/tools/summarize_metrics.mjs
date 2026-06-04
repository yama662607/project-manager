import { readdir, readFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

const logDir = process.argv[2] ?? path.join(os.homedir(), "Library/Logs/ProjectLauncherBench");

function percentile(values, p) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1);
  return sorted[index];
}

function round(value) {
  return value == null ? null : Math.round(value * 1000) / 1000;
}

function summarize(values) {
  return {
    count: values.length,
    p50: round(percentile(values, 50)),
    p95: round(percentile(values, 95)),
    max: round(values.length ? Math.max(...values) : null)
  };
}

const files = (await readdir(logDir).catch(() => []))
  .filter((file) => file.endsWith(".jsonl"))
  .map((file) => path.join(logDir, file));

const cycles = new Map();
const direct = new Map();

function scenarioFor(event) {
  if (typeof event.scenario === "string" && event.scenario.length > 0) return event.scenario;
  if (event.metric === "selection_move_ms") return "navigation";
  if (event.alias_hit === "a" || event.query === "a") return "alias";
  if (event.query === "pr") return "narrowing";
  return "";
}

function pushMetric(app, metric, value, scenario = "") {
  const keys = [`${app}:${metric}`];
  if (scenario) keys.push(`${app}:${scenario}_${metric}`);
  for (const key of keys) {
    if (!direct.has(key)) direct.set(key, []);
    direct.get(key).push(value);
  }
}

for (const file of files) {
  const content = await readFile(file, "utf8");
  for (const line of content.split("\n")) {
    if (!line.trim()) continue;
    let event;
    try {
      event = JSON.parse(line);
    } catch {
      continue;
    }

    const app = event.app ?? "unknown";
    if (event.metric && typeof event.duration_ms === "number") {
      pushMetric(app, event.metric, event.duration_ms, scenarioFor(event));
      continue;
    }

    if (!event.cycle_id || typeof event.mono_ns !== "number") continue;
    const key = `${app}:${event.cycle_id}`;
    if (!cycles.has(key)) cycles.set(key, { app });
    cycles.get(key)[event.event] = event.mono_ns;
    if (!cycles.get(key).scenario) {
      cycles.get(key).scenario = scenarioFor(event);
    }
  }
}

const byApp = new Map();

for (const cycle of cycles.values()) {
  if (!byApp.has(cycle.app)) {
    byApp.set(cycle.app, {
      hotkey_to_render_ms: [],
      open_dispatch_ms: []
    });
  }
  const appStats = byApp.get(cycle.app);
  if (cycle.hotkey_received && cycle.palette_rendered) {
    appStats.hotkey_to_render_ms.push((cycle.palette_rendered - cycle.hotkey_received) / 1_000_000);
  }
  if (cycle.open_requested && cycle.open_dispatched) {
    const duration = (cycle.open_dispatched - cycle.open_requested) / 1_000_000;
    appStats.open_dispatch_ms.push(duration);
    if (cycle.scenario) {
      const key = `${cycle.scenario}_open_dispatch_ms`;
      appStats[key] ??= [];
      appStats[key].push(duration);
    }
  }
}

for (const [key, values] of direct) {
  const [app, metric] = key.split(":");
  if (!byApp.has(app)) byApp.set(app, {});
  byApp.get(app)[metric] = values;
}

const result = {};
for (const [app, metrics] of byApp) {
  result[app] = {};
  for (const [metric, values] of Object.entries(metrics)) {
    result[app][metric] = summarize(values);
  }
}

console.log(JSON.stringify(result, null, 2));
