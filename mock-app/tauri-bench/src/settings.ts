import "./settings.css";
import { invoke } from "@tauri-apps/api/core";

type Project = {
  id: string;
  name: string;
  path: string;
  openPaths?: string[];
  aliases: string[];
  tags: string[];
  language: string;
  lastOpenedAt?: string;
};

type AppConfig = {
  projects: Project[];
};

let projects: Project[] = [];
let selectedIndex = 0;

const projectList = document.querySelector<HTMLUListElement>("#project-list")!;
const addBtn = document.querySelector<HTMLButtonElement>("#add-project")!;
const removeBtn = document.querySelector<HTMLButtonElement>("#remove-project")!;
const browseBtn = document.querySelector<HTMLButtonElement>("#browse-project")!;
const fieldName = document.querySelector<HTMLInputElement>("#field-name")!;
const fieldPath = document.querySelector<HTMLInputElement>("#field-path")!;
const fieldOpenPaths = document.querySelector<HTMLInputElement>("#field-openpaths")!;
const fieldAliases = document.querySelector<HTMLInputElement>("#field-aliases")!;
const fieldTags = document.querySelector<HTMLInputElement>("#field-tags")!;
const fieldLanguage = document.querySelector<HTMLInputElement>("#field-language")!;
const saveBtn = document.querySelector<HTMLButtonElement>("#save")!;
const cancelBtn = document.querySelector<HTMLButtonElement>("#cancel")!;
const statusEl = document.querySelector<HTMLSpanElement>("#status")!;

// Tab switching
document.querySelectorAll<HTMLButtonElement>(".tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach((t) => t.classList.remove("active"));
    document.querySelectorAll(".tab-content").forEach((c) => c.classList.remove("active"));
    tab.classList.add("active");
    document.getElementById(tab.dataset.tab!)?.classList.add("active");
  });
});

// Load config on startup
async function loadConfig() {
  const config = (await invoke("get_config")) as AppConfig;
  projects = config.projects;
  renderProjectList();
}

// Project list rendering
function renderProjectList() {
  projectList.textContent = "";
  for (const [index, project] of projects.entries()) {
    const li = document.createElement("li");
    li.textContent = project.name || "Untitled";
    if (index === selectedIndex) li.classList.add("selected");
    li.addEventListener("click", () => {
      selectedIndex = index;
      renderProjectList();
      populateDetail();
    });
    projectList.append(li);
  }
}

function populateDetail() {
  const project = projects[selectedIndex];
  if (!project) {
    fieldName.value = "";
    fieldPath.value = "";
    fieldOpenPaths.value = "";
    fieldAliases.value = "";
    fieldTags.value = "";
    fieldLanguage.value = "";
    return;
  }
  fieldName.value = project.name;
  fieldPath.value = project.path;
  fieldOpenPaths.value = (project.openPaths ?? []).join(", ");
  fieldAliases.value = (project.aliases ?? []).join(", ");
  fieldTags.value = (project.tags ?? []).join(", ");
  fieldLanguage.value = project.language;
}

function syncDetailToProject() {
  const project = projects[selectedIndex];
  if (!project) return;
  project.name = fieldName.value;
  project.path = fieldPath.value;
  project.openPaths = fieldOpenPaths.value
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  project.aliases = fieldAliases.value
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  project.tags = fieldTags.value
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  project.language = fieldLanguage.value;
}

// Detail field changes
[fieldName, fieldPath, fieldOpenPaths, fieldAliases, fieldTags, fieldLanguage].forEach((field) => {
  field.addEventListener("input", () => {
    syncDetailToProject();
    renderProjectList();
  });
});

// Add / Remove / Browse
addBtn.addEventListener("click", () => {
  const id = `project-${Date.now()}`;
  projects.push({
    id,
    name: "New Project",
    path: "",
    openPaths: [],
    aliases: [],
    tags: [],
    language: "Project",
  });
  selectedIndex = projects.length - 1;
  renderProjectList();
  populateDetail();
  fieldName.focus();
  fieldName.select();
});

browseBtn.addEventListener("click", async () => {
  try {
    const selected = await invoke("browse_folder") as string | null;
    if (!selected) return;
    const name = selected.split("/").pop() || selected;
    const id = `browse-${Date.now()}`;
    projects.push({
      id,
      name,
      path: selected,
      openPaths: [],
      aliases: [],
      tags: [],
      language: "Project",
    });
    selectedIndex = projects.length - 1;
    renderProjectList();
    populateDetail();
  } catch (e) {
    statusEl.textContent = `Browse error: ${e}`;
  }
});

removeBtn.addEventListener("click", () => {
  if (projects.length === 0) return;
  projects.splice(selectedIndex, 1);
  selectedIndex = Math.min(selectedIndex, projects.length - 1);
  renderProjectList();
  populateDetail();
});

// Save / Cancel
saveBtn.addEventListener("click", async () => {
  statusEl.textContent = "Saving...";
  try {
    await invoke("save_config", {
      config: {
        projects: projects.map(({ id, name, path, openPaths, aliases, tags, language }) => ({
          id,
          name,
          path,
          openPaths: openPaths ?? [],
          aliases,
          tags,
          language,
          lastOpenedAt: "",
        })),
      },
    });
    await invoke("close_settings_window");
  } catch (error) {
    statusEl.textContent = `Error: ${error}`;
  }
});

cancelBtn.addEventListener("click", async () => {
  await invoke("close_settings_window");
});

// Init
loadConfig().then(() => {
  populateDetail();
});
