import { existsSync } from "node:fs";
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const outDir = path.join(root, "shared");
const outFile = path.join(outDir, "projects.json");
const home = os.homedir();
const projectManagerFile = path.join(
  home,
  "Library/Application Support/Code/User/globalStorage/alefragnani.project-manager/projects.json"
);

const manualProjects = [
  {
    name: "Project Manager",
    rootPath: path.join(home, "Code/Projects/project-manager"),
    aliases: ["pm", "proj"],
    tags: ["launcher", "appkit", "tauri", "favorite"]
  },
  {
    name: "Mac Agent",
    rootPath: path.join(home, "Code/Projects/mac-agent"),
    aliases: ["ma"],
    tags: ["agent", "mac", "automation"]
  },
  {
    name: "Kanade",
    rootPath: path.join(home, "Code/Projects/kanade"),
    aliases: ["ka"],
    tags: ["agent", "team"]
  },
  {
    name: "Environment Setup",
    rootPath: path.join(home, "Code/Tools/environment-setup"),
    aliases: ["env"],
    tags: ["environment", "setup", "tools"]
  }
];

const fallbackRoots = [
  path.join(home, "Code/Projects"),
  path.join(home, "Code/Learning"),
  path.join(home, "Code/Research"),
  path.join(home, "Code/Tools")
];

function slugify(value) {
  return value
    .normalize("NFKD")
    .replace(/[^\w\s-]/g, "")
    .trim()
    .toLowerCase()
    .replace(/[\s_]+/g, "-")
    .replace(/-+/g, "-");
}

function inferLanguage(rootPath) {
  if (rootPath.endsWith(".code-workspace")) return "Workspace";
  const checks = [
    ["Package.swift", "Swift"],
    ["Cargo.toml", "Rust"],
    ["go.mod", "Go"],
    ["package.json", "TypeScript"],
    ["pyproject.toml", "Python"],
    ["Gemfile", "Ruby"],
    ["pom.xml", "Java"],
    ["build.gradle", "Kotlin"],
    ["mise-config.toml", "Shell"]
  ];
  for (const [file, language] of checks) {
    if (existsSync(path.join(rootPath, file))) return language;
  }
  return "Project";
}

async function readJson(file) {
  return JSON.parse(await readFile(file, "utf8"));
}

function candidateNames(entry) {
  return [
    entry.name,
    path.basename(entry.rootPath ?? ""),
    slugify(entry.name),
    slugify(path.basename(entry.rootPath ?? ""))
  ].filter(Boolean);
}

async function resolveFromCode(entry) {
  const names = new Set(candidateNames(entry));
  const lowerNames = new Set([...names].map((name) => name.toLowerCase()));
  for (const rootDir of fallbackRoots) {
    for (const name of names) {
      const direct = path.join(rootDir, name);
      if (existsSync(direct)) return direct;
    }
    let children = [];
    try {
      children = await readdir(rootDir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const child of children) {
      if (!child.isDirectory()) continue;
      if (lowerNames.has(child.name.toLowerCase())) return path.join(rootDir, child.name);
    }
  }

  return null;
}

function isICloudPath(rootPath) {
  return rootPath.includes("/Library/Mobile Documents/");
}

async function resolveRootPath(entry) {
  if (typeof entry.rootPath !== "string") return null;

  const codePath = await resolveFromCode(entry);
  if (codePath) return codePath;

  if (isICloudPath(entry.rootPath)) return null;
  if (existsSync(entry.rootPath)) return entry.rootPath;

  return null;
}

async function workspaceFolders(workspacePath) {
  if (!workspacePath.endsWith(".code-workspace") || !existsSync(workspacePath)) return [];
  const workspace = await readJson(workspacePath);
  const base = path.dirname(workspacePath);
  return (workspace.folders ?? [])
    .map((folder) => folder.path)
    .filter((folderPath) => typeof folderPath === "string" && folderPath.length > 0)
    .map((folderPath) => path.resolve(base, folderPath))
    .filter((folderPath) => existsSync(folderPath));
}

function defaultAliases(name) {
  if (name === "DotFiles") return ["d", "df", "dot"];
  const normalized = slugify(name);
  const compact = normalized.replaceAll("-", "");
  const acronym = normalized
    .split("-")
    .filter(Boolean)
    .map((part) => part[0])
    .join("");
  return [...new Set([compact, acronym].filter((alias) => alias.length >= 2 && alias.length <= 12))];
}

async function normalizeProject(entry, index, source) {
  const rootPath = await resolveRootPath(entry);
  if (!rootPath) return null;

  const openPaths = await workspaceFolders(rootPath);
  const aliases = [...new Set([...(entry.aliases ?? []), ...defaultAliases(entry.name)])];
  const tags = [...new Set([...(entry.tags ?? []), ...(entry.tags?.length ? [] : ["project"]), source])];

  return {
    id: `${source}-${slugify(entry.name) || index}`,
    name: entry.name,
    path: rootPath,
    openPaths,
    aliases,
    tags,
    language: inferLanguage(rootPath),
    lastOpenedAt: `2026-06-03T${String(index % 24).padStart(2, "0")}:00:00Z`
  };
}

const projectManagerProjects = existsSync(projectManagerFile)
  ? (await readJson(projectManagerFile)).filter((project) => project.enabled !== false)
  : [];

const projects = [];
const seenPaths = new Set();
for (const [index, entry] of [...projectManagerProjects, ...manualProjects].entries()) {
  const project = await normalizeProject(entry, index, index < projectManagerProjects.length ? "vscode-pm" : "manual");
  if (!project || seenPaths.has(project.path)) continue;
  seenPaths.add(project.path);
  projects.push(project);
}

await mkdir(outDir, { recursive: true });
await writeFile(outFile, `${JSON.stringify(projects)}\n`);
console.log(`wrote ${projects.length} existing projects to ${path.relative(root, outFile)}`);
