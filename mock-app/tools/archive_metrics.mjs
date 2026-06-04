import { mkdir, readdir, rename } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

const logDir = process.argv[2] ?? path.join(os.homedir(), "Library/Logs/ProjectLauncherBench");
const stamp = new Date().toISOString().replace(/[-:]/g, "").replace(/\..+/, "").replace("T", "-");
const archiveDir = path.join(logDir, `archive-${stamp}`);

await mkdir(logDir, { recursive: true });
const files = (await readdir(logDir).catch(() => [])).filter((file) => file.endsWith(".jsonl"));

if (files.length === 0) {
  console.log(`no jsonl logs in ${logDir}`);
  process.exit(0);
}

await mkdir(archiveDir, { recursive: true });
for (const file of files) {
  await rename(path.join(logDir, file), path.join(archiveDir, file));
}

console.log(`archived ${files.length} logs to ${archiveDir}`);
