import { cp, mkdir, writeFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const binDir = path.join(root, ".wails-bin");
const wrapper = path.join(binDir, "wails3");

function run(command, args, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: options.cwd ?? root,
      stdio: "inherit",
      env: {
        ...process.env,
        PACKAGE_MANAGER: "bun",
        PATH: `${binDir}:${process.env.PATH ?? ""}`
      }
    });
    child.on("exit", (code) => {
      if (code === 0) resolve();
      else reject(new Error(`${command} ${args.join(" ")} exited with ${code}`));
    });
  });
}

await mkdir(binDir, { recursive: true });
await writeFile(
  wrapper,
  "#!/bin/sh\nexec go run github.com/wailsapp/wails/v3/cmd/wails3@v3.0.0-alpha.97 \"$@\"\n",
  { mode: 0o755 }
);
await mkdir(path.join(root, "wails-bench/resources"), { recursive: true });
await cp(path.join(root, "shared/projects.json"), path.join(root, "wails-bench/resources/projects.json"));
await run(wrapper, ["package"], { cwd: path.join(root, "wails-bench") });
const appResources = path.join(root, "wails-bench/bin/wailsbench.app/Contents/Resources");
await mkdir(appResources, { recursive: true });
await cp(path.join(root, "shared/projects.json"), path.join(appResources, "projects.json"));
await run("codesign", ["--force", "--deep", "--sign", "-", path.join(root, "wails-bench/bin/wailsbench.app")]);
