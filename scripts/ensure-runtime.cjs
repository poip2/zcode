#!/usr/bin/env node
const { execFileSync } = require("node:child_process");
const path = require("node:path");
const fs = require("node:fs");

const root = path.resolve(__dirname, "..");
const runtimeDir = path.join(root, "src-tauri", "resources", "runtime");

function runtimeReady() {
  return (
    fs.existsSync(path.join(runtimeDir, "bin")) &&
    fs.existsSync(path.join(runtimeDir, "python"))
  );
}

if (runtimeReady()) {
  console.log("[ensure-runtime] Bundled runtime already present, skipping fetch.");
  process.exit(0);
}

console.log("[ensure-runtime] Bundled runtime not found, fetching for", process.platform);

try {
  if (process.platform === "darwin") {
    execFileSync("bash", ["scripts/fetch-runtime/fetch-macos.sh"], { stdio: "inherit", cwd: root });
  } else if (process.platform === "win32") {
    execFileSync("powershell", ["-ExecutionPolicy", "Bypass", "-File", "scripts/fetch-runtime/fetch-windows.ps1"], { stdio: "inherit", cwd: root });
  } else if (process.platform === "linux") {
    execFileSync("bash", ["scripts/fetch-runtime/fetch-linux.sh"], { stdio: "inherit", cwd: root });
  } else {
    console.error(`[ensure-runtime] Unsupported platform: ${process.platform}`);
    process.exit(1);
  }
} catch (err) {
  console.error("[ensure-runtime] Fetch failed:", err.message);
  process.exit(1);
}

console.log("[ensure-runtime] Done.");
