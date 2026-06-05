import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const names = ["AppKitBench", "IcedBench"];

const { stdout } = await execFileAsync("ps", ["-axo", "pid,args,rss"]);
const rows = stdout
  .trim()
  .split("\n")
  .slice(1)
  .map((line) => line.trim())
  .filter(Boolean);

const result = [];
for (const row of rows) {
  const match = row.match(/^(\d+)\s+(.+?)\s+(\d+)$/);
  if (!match) continue;
  const [, pid, command, rssKb] = match;
  if (!names.some((name) => command.includes(name))) continue;
  result.push({
    pid: Number(pid),
    command,
    rss_kb: Number(rssKb),
    rss_mb: Math.round((Number(rssKb) / 1024) * 10) / 10
  });
}

console.log(JSON.stringify(result, null, 2));
