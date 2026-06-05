import "./settings.css";
import { invoke } from "@tauri-apps/api/core";

type ShortcutConfig = {
  modifiers: string[];
  key: string;
};

type Project = {
  id: string;
  name: string;
  path: string;
  openPaths: string[];
  aliases: string[];
  tags: string[];
  language: string;
  lastOpenedAt: string;
};

type AppConfig = {
  projects: Project[];
  shortcut: ShortcutConfig;
};

let projects: Project[] = [];
let shortcut: ShortcutConfig = { modifiers: ["control"], key: "m" };
let selectedIndex = 0;

const list = document.querySelector<HTMLUListElement>("#project-list")!;
const statusEl = document.querySelector<HTMLElement>("#status")!;
const fields = {
  name: document.querySelector<HTMLInputElement>("#field-name")!,
  path: document.querySelector<HTMLInputElement>("#field-path")!,
  openPaths: document.querySelector<HTMLInputElement>("#field-openpaths")!,
  aliases: document.querySelector<HTMLInputElement>("#field-aliases")!,
  tags: document.querySelector<HTMLInputElement>("#field-tags")!,
  language: document.querySelector<HTMLInputElement>("#field-language")!
};

function splitList(value: string): string[] {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

function basename(value: string): string {
  return value.replace(/\/$/, "").split("/").at(-1) || value;
}

function renderList(): void {
  list.textContent = "";
  for (const [index, project] of projects.entries()) {
    const item = document.createElement("li");
    item.textContent = project.name || "Untitled";
    item.classList.toggle("selected", index === selectedIndex);
    item.addEventListener("click", () => {
      syncFieldsToProject();
      selectedIndex = index;
      renderList();
      renderFields();
    });
    list.append(item);
  }
}

function renderFields(): void {
  const project = projects[selectedIndex];
  const disabled = !project;
  for (const field of Object.values(fields)) field.disabled = disabled;
  if (!project) {
    for (const field of Object.values(fields)) field.value = "";
    return;
  }
  fields.name.value = project.name;
  fields.path.value = project.path;
  fields.openPaths.value = project.openPaths.join(", ");
  fields.aliases.value = project.aliases.join(", ");
  fields.tags.value = project.tags.join(", ");
  fields.language.value = project.language;
}

function syncFieldsToProject(): void {
  const project = projects[selectedIndex];
  if (!project) return;
  project.name = fields.name.value;
  project.path = fields.path.value;
  project.openPaths = splitList(fields.openPaths.value);
  project.aliases = splitList(fields.aliases.value);
  project.tags = splitList(fields.tags.value);
  project.language = fields.language.value || "Project";
}

for (const field of Object.values(fields)) {
  field.addEventListener("input", () => {
    syncFieldsToProject();
    renderList();
  });
}

document.querySelector<HTMLButtonElement>("#add-project")!.addEventListener("click", () => {
  syncFieldsToProject();
  projects.push({
    id: `manual-${Date.now()}`,
    name: "New Project",
    path: "",
    openPaths: [],
    aliases: [],
    tags: [],
    language: "Project",
    lastOpenedAt: ""
  });
  selectedIndex = projects.length - 1;
  renderList();
  renderFields();
  fields.name.focus();
  fields.name.select();
});

document.querySelector<HTMLButtonElement>("#remove-project")!.addEventListener("click", () => {
  if (!projects.length) return;
  projects.splice(selectedIndex, 1);
  selectedIndex = Math.max(0, Math.min(selectedIndex, projects.length - 1));
  renderList();
  renderFields();
});

async function addBrowsedProject(command: "browse_folder" | "browse_workspace_file"): Promise<void> {
  const path = await invoke<string | null>(command);
  if (!path) return;
  syncFieldsToProject();
  projects.push({
    id: `manual-${Date.now()}`,
    name: basename(path),
    path,
    openPaths: [],
    aliases: [],
    tags: ["manual"],
    language: "Project",
    lastOpenedAt: ""
  });
  selectedIndex = projects.length - 1;
  renderList();
  renderFields();
}

document
  .querySelector<HTMLButtonElement>("#browse-folder")!
  .addEventListener("click", () => void addBrowsedProject("browse_folder"));
document
  .querySelector<HTMLButtonElement>("#browse-workspace")!
  .addEventListener("click", () => void addBrowsedProject("browse_workspace_file"));

document.querySelector<HTMLButtonElement>("#save")!.addEventListener("click", async () => {
  syncFieldsToProject();
  statusEl.textContent = "Saving...";
  try {
    await invoke("save_config", { config: { projects, shortcut } });
    await invoke("close_settings_window");
  } catch (error) {
    statusEl.textContent = `Error: ${String(error)}`;
  }
});

document.querySelector<HTMLButtonElement>("#cancel")!.addEventListener("click", async () => {
  await invoke("close_settings_window");
});

const config = await invoke<AppConfig>("get_config");
projects = config.projects;
shortcut = config.shortcut;
renderList();
renderFields();
