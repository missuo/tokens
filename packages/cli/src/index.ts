#!/usr/bin/env node
import { spawnSync, execSync } from "node:child_process";
import { existsSync, readdirSync, realpathSync } from "node:fs";
import { resolve, join, basename } from "node:path";
import { fileURLToPath } from "node:url";

const binaryName = process.platform === "win32" ? "tokens.exe" : "tokens";

const currentDir = fileURLToPath(new URL(".", import.meta.url));
const dirName = basename(currentDir);
// The packages are unscoped, so the platform binaries are siblings of the
// dispatcher under the same node_modules (no @scope/ directory level).
// In npm install: currentDir = .../node_modules/tokens-cli/dist/
//   cliDir = .../node_modules/tokens-cli/
//   parentDir = .../node_modules/   (siblings: tokens-cli-<platform>/)
// In monorepo dev (dist): currentDir = .../packages/cli/dist/
//   cliDir = .../packages/cli/
//   parentDir = .../packages/
// In monorepo dev (src): currentDir = .../packages/cli/src/
//   cliDir = .../packages/cli/
//   parentDir = .../packages/
const isSubDir = dirName === "dist" || dirName === "src";
const cliDir = isSubDir ? resolve(currentDir, "..") : currentDir;
const parentDir = resolve(cliDir, "..");
const workspaceRoot = resolve(parentDir, "..");

type LibcKind = "gnu" | "musl";

function detectLibcKind(): LibcKind {
  const override = process.env.TOKENS_LIBC?.trim().toLowerCase();
  if (override === "musl") return "musl";
  if (override === "gnu" || override === "glibc") return "gnu";

  const report = process.report?.getReport?.() as
    | {
        header?: {
          glibcVersionRuntime?: string;
          release?: { sourceUrl?: string };
        };
        sharedObjects?: string[];
      }
    | undefined;

  if (report?.header?.glibcVersionRuntime) {
    return "gnu";
  }

  if (
    Array.isArray(report?.sharedObjects) &&
    report.sharedObjects.some((obj) => obj.toLowerCase().includes("musl"))
  ) {
    return "musl";
  }

  // Bun reports neither glibcVersionRuntime nor sharedObjects, but its
  // release.sourceUrl names the build flavor (e.g. bun-linux-x64-musl-baseline.zip).
  if (report?.header?.release?.sourceUrl?.toLowerCase().includes("musl")) {
    return "musl";
  }

  try {
    const output = execSync("ldd --version", {
      encoding: "utf-8",
      stdio: ["ignore", "pipe", "pipe"],
    }).toLowerCase();
    if (output.includes("musl")) return "musl";
    if (output.includes("glibc") || output.includes("gnu")) return "gnu";
  } catch (error) {
    // musl's ldd rejects --version: it prints "musl libc" to stderr and
    // exits non-zero, so the answer is in the error, not the output.
    const { stdout, stderr } = (error ?? {}) as { stdout?: unknown; stderr?: unknown };
    const combined = `${stdout ?? ""}\n${stderr ?? ""}`.toLowerCase();
    if (combined.includes("musl")) return "musl";
    if (combined.includes("glibc") || combined.includes("gnu")) return "gnu";
  }

  // ldd missing or inconclusive: look for dynamic loaders. Either loader
  // can coexist with the other's libc (Debian's musl package installs
  // ld-musl-*; Alpine's gcompat installs ld-linux-*), so when both are
  // present, let the distro break the tie.
  const hasGnuLoader = loaderPresent("ld-linux-");
  const hasMuslLoader = loaderPresent("ld-musl-");
  if (hasGnuLoader !== hasMuslLoader) return hasMuslLoader ? "musl" : "gnu";
  if (hasGnuLoader && hasMuslLoader) {
    return existsSync("/etc/alpine-release") ? "musl" : "gnu";
  }

  return "gnu";
}

// Glibc ships ld-linux-*.so.* in /lib64 (or /lib on some arches); musl
// distros (Alpine, Void-musl, ...) ship /lib/ld-musl-<arch>.so.1.
function loaderPresent(prefix: string): boolean {
  for (const dir of ["/lib", "/lib64"]) {
    try {
      if (readdirSync(dir).some((entry) => entry.startsWith(prefix))) {
        return true;
      }
    } catch {
      // Directory unreadable or missing; try the next one.
    }
  }
  return false;
}

function resolveTargetPackageName(): string | null {
  const arch = process.arch;

  if (process.platform === "darwin") {
    if (arch === "arm64") return "cli-darwin-arm64";
    if (arch === "x64") return "cli-darwin-x64";
    return null;
  }

  if (process.platform === "linux") {
    const libc = detectLibcKind();
    if (arch === "arm64") {
      return libc === "musl" ? "cli-linux-arm64-musl" : "cli-linux-arm64-gnu";
    }
    if (arch === "x64") {
      return libc === "musl" ? "cli-linux-x64-musl" : "cli-linux-x64-gnu";
    }
    return null;
  }

  if (process.platform === "win32") {
    if (arch === "arm64") return "cli-win32-arm64-msvc";
    if (arch === "x64") return "cli-win32-x64-msvc";
    return null;
  }

  return null;
}

function resolveRustTargetTriple(): string | null {
  const arch = process.arch;

  if (process.platform === "darwin") {
    if (arch === "arm64") return "aarch64-apple-darwin";
    if (arch === "x64") return "x86_64-apple-darwin";
    return null;
  }

  if (process.platform === "linux") {
    const libc = detectLibcKind();
    if (arch === "arm64") {
      return libc === "musl"
        ? "aarch64-unknown-linux-musl"
        : "aarch64-unknown-linux-gnu";
    }
    if (arch === "x64") {
      return libc === "musl"
        ? "x86_64-unknown-linux-musl"
        : "x86_64-unknown-linux-gnu";
    }
    return null;
  }

  if (process.platform === "win32") {
    if (arch === "arm64") return "aarch64-pc-windows-msvc";
    if (arch === "x64") return "x86_64-pc-windows-msvc";
    return null;
  }

  return null;
}

// e.g. "cli-darwin-arm64" — the monorepo directory name and the suffix of the
// unscoped npm package name `tokens-cli-darwin-arm64`.
const targetDir = resolveTargetPackageName();
const searchPaths: string[] = [];

if (targetDir) {
  const targetPackage = `tokens-${targetDir}`;
  searchPaths.push(
    // npm/bun install: sibling package (node_modules/tokens-cli-<platform>/bin/...)
    join(parentDir, targetPackage, "bin", binaryName),
    // Nested node_modules: non-hoisted / pnpm (node_modules/tokens-cli/node_modules/tokens-cli-<platform>/bin/...)
    join(cliDir, "node_modules", targetPackage, "bin", binaryName),
    // Hoisted to the workspace-root node_modules
    join(workspaceRoot, "node_modules", targetPackage, "bin", binaryName),
    // Monorepo development (packages/cli-<platform>/bin/...)
    join(workspaceRoot, "packages", targetDir, "bin", binaryName),
  );
}

// Cargo output lives under cli/target/, because the Rust workspace manifest is
// cli/Cargo.toml rather than the repo root. These paths only matter for
// monorepo development — an npm install resolves the sibling platform package
// above and never reaches here.
const rustTargetTriple = resolveRustTargetTriple();
if (rustTargetTriple) {
  searchPaths.push(join(workspaceRoot, "cli", "target", rustTargetTriple, "release", binaryName));
}

searchPaths.push(
  join(workspaceRoot, "cli", "target", "release", binaryName),
  join(cliDir, "bin", binaryName),
);

function tryRealpath(p: string): string {
  try {
    return realpathSync(p);
  } catch {
    return p;
  }
}

// Paths that would re-enter this wrapper if executed - using any of these as
// the "real" binary causes infinite recursion (a fork bomb). We compare by
// realpath so symlinks (e.g. npm/bun bin shims) are dereferenced.
const selfPaths = new Set<string>([
  tryRealpath(fileURLToPath(import.meta.url)),
  tryRealpath(join(cliDir, "bin.js")),
]);
if (process.argv[1]) {
  selfPaths.add(tryRealpath(process.argv[1]));
}

function isSelfReference(p: string): boolean {
  return selfPaths.has(tryRealpath(p));
}

let binary = searchPaths.find((p) => existsSync(p) && !isSelfReference(p));

if (!binary) {
  console.error("Error: tokens binary not found");
  console.error("Build from source: cargo build --release --manifest-path cli/Cargo.toml -p tokens-cli");
  if (targetDir) {
    console.error(`Expected optional package: tokens-${targetDir}`);
  }
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
process.exit(result.status ?? 1);
