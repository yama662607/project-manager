import { spawnSync } from "node:child_process";

const result = spawnSync("bunx", ["tauri", "build"], {
  cwd: new URL("../tauri-bench", import.meta.url),
  stdio: "inherit"
});

process.exit(result.status ?? 1);
