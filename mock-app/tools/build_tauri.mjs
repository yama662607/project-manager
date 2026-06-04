import { cp, mkdir } from "node:fs/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tauriDir = path.join(root, "tauri-bench");
const appPath = path.join(
  tauriDir,
  "src-tauri/target/release/bundle/macos/TauriBench.app"
);
const publicDir = path.join(tauriDir, "public");

function run(command, args, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: options.cwd ?? root,
      stdio: "inherit",
      env: process.env
    });
    child.on("exit", (code) => {
      if (code === 0) resolve();
      else reject(new Error(`${command} ${args.join(" ")} exited with ${code}`));
    });
  });
}

await mkdir(publicDir, { recursive: true });
await cp(path.join(root, "shared/projects.json"), path.join(publicDir, "projects.json"));
await run("bun", ["run", "build"], { cwd: tauriDir });
await run("codesign", ["--force", "--deep", "--sign", "-", appPath]);
