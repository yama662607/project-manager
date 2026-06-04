import { cp, mkdir, rm, writeFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const appDir = path.join(root, "dist", "AppKitBench.app");
const contents = path.join(appDir, "Contents");
const macos = path.join(contents, "MacOS");
const resources = path.join(contents, "Resources");

function run(command, args, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: options.cwd ?? root,
      stdio: "inherit"
    });
    child.on("exit", (code) => {
      if (code === 0) resolve();
      else reject(new Error(`${command} ${args.join(" ")} exited with ${code}`));
    });
  });
}

await run("swift", ["build", "-c", "release", "--package-path", path.join(root, "appkit-bench")]);

await rm(appDir, { recursive: true, force: true });
await mkdir(macos, { recursive: true });
await mkdir(resources, { recursive: true });

await cp(path.join(root, "appkit-bench/.build/release/AppKitBench"), path.join(macos, "AppKitBench"));
await cp(path.join(root, "shared/projects.json"), path.join(resources, "projects.json"));

const infoPlist = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>AppKitBench</string>
  <key>CFBundleIdentifier</key>
  <string>dev.projectlauncher.bench.appkit</string>
  <key>CFBundleName</key>
  <string>AppKitBench</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
  <key>LSUIElement</key>
  <true/>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
`;

await writeFile(path.join(contents, "Info.plist"), infoPlist);
await writeFile(path.join(contents, "PkgInfo"), "APPL????");
console.log(`built ${path.relative(root, appDir)}`);
