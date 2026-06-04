import { cp, mkdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const appDir = path.join(root, "dist", "IcedBench.app");
const contents = path.join(appDir, "Contents");
const macos = path.join(contents, "MacOS");
const resources = path.join(contents, "Resources");

await rm(appDir, { recursive: true, force: true });
await mkdir(macos, { recursive: true });
await mkdir(resources, { recursive: true });
await cp(path.join(root, "iced-bench/target/release/IcedBench"), path.join(macos, "IcedBench"));
await cp(path.join(root, "shared/projects.json"), path.join(resources, "projects.json"));

const plist = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>IcedBench</string>
  <key>CFBundleIdentifier</key>
  <string>dev.projectlauncher.bench.iced</string>
  <key>CFBundleName</key>
  <string>IcedBench</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
`;

await writeFile(path.join(contents, "Info.plist"), plist);
await writeFile(path.join(contents, "PkgInfo"), "APPL????");
console.log(`built ${path.relative(root, appDir)}`);
