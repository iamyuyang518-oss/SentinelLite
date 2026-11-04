#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const outputPath = path.join(root, "docs", "THIRD_PARTY_INVENTORY.md");

function normalizeLicense(packageJson) {
  const license = packageJson.license;
  if (typeof license === "string" && license.trim()) {
    return license.trim();
  }
  if (license && typeof license === "object" && typeof license.type === "string") {
    return license.type;
  }
  if (Array.isArray(packageJson.licenses)) {
    const entries = packageJson.licenses
      .map((entry) => (typeof entry === "string" ? entry : entry?.type))
      .filter((entry) => typeof entry === "string" && entry.trim());
    if (entries.length > 0) {
      return [...new Set(entries)].join(" OR ");
    }
  }
  return "UNKNOWN";
}

function runJson(command, args) {
  const output = execFileSync(command, args, {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  return JSON.parse(output);
}

function runNpmQuery() {
  const npmCli = process.env.npm_execpath;
  if (npmCli) {
    return runJson(process.execPath, [npmCli, "query", "*", "--json"]);
  }
  return runJson("npm", ["query", "*", "--json"]);
}

function escapeCell(value) {
  return String(value).replaceAll("|", "\\|").replaceAll("\n", " ");
}

function compareLooksLikeGpl(expression) {
  return /(?:^|[^L])GPL|AGPL|SSPL|Commons Clause|Business Source|Elastic License/i.test(
    expression,
  );
}

function compareNeedsReview(expression) {
  return /MPL|LGPL|CC-BY|BlueOak|Unicode-3.0/i.test(expression);
}

function collectNpm() {
  const packages = runNpmQuery();
  const byId = new Map();

  for (const pkg of packages) {
    if (!pkg?.name || !pkg?.version || pkg.name === "sentinel-lite") {
      continue;
    }
    byId.set(`${pkg.name}@${pkg.version}`, {
      name: pkg.name,
      version: pkg.version,
      license: normalizeLicense(pkg),
    });
  }

  return [...byId.values()].sort(
    (left, right) => compareCodeUnits(left.name, right.name) || compareCodeUnits(left.version, right.version),
  );
}

function compareCodeUnits(left, right) {
  if (left < right) return -1;
  if (left > right) return 1;
  return 0;
}

function collectCargo() {
  const metadata = runJson("cargo", ["metadata", "--format-version", "1", "--locked"]);
  const byId = new Map();

  for (const pkg of metadata.packages) {
    if (!pkg?.source || !pkg?.name || !pkg?.version) {
      continue;
    }
    byId.set(`${pkg.name}@${pkg.version}`, {
      name: pkg.name,
      version: pkg.version,
      license: pkg.license?.trim() || "UNKNOWN",
    });
  }

  return [...byId.values()].sort(
    (left, right) => compareCodeUnits(left.name, right.name) || compareCodeUnits(left.version, right.version),
  );
}

const npmPackages = collectNpm();
const cargoPackages = collectCargo();
const allPackages = [
  ...npmPackages.map((pkg) => ({ ...pkg, ecosystem: "npm" })),
  ...cargoPackages.map((pkg) => ({ ...pkg, ecosystem: "Cargo" })),
];

const unknown = allPackages.filter((pkg) => pkg.license === "UNKNOWN");
const disallowed = allPackages.filter((pkg) => compareLooksLikeGpl(pkg.license));
const needsReview = allPackages.filter(
  (pkg) => !compareLooksLikeGpl(pkg.license) && compareNeedsReview(pkg.license),
);

const lines = [
  "# 第三方依赖许可证清单",
  "",
  "> 由 `node scripts/check-licenses.mjs` 根据当前锁定的 npm 与 Cargo 依赖生成。",
  "> 本清单是工程审查辅助，不构成法律意见。发布前仍需人工复核许可证义务，",
  "> 特别是 MPL、CC-BY、LGPL 选项、商标与 NOTICE 文件要求。",
  "",
  "## 摘要",
  "",
  `- npm 已安装依赖：${npmPackages.length}`,
  `- Cargo 已解析依赖：${cargoPackages.length}`,
  `- 缺少许可证字段：${unknown.length}`,
  `- 需要额外人工审查的许可证：${needsReview.length}`,
  `- 自动标记为禁止的强 copyleft/source-available 表达式：${disallowed.length}`,
  "",
  "## npm 依赖",
  "",
  "| 软件包 | 版本 | 许可证 |",
  "| --- | --- | --- |",
  ...npmPackages.map(
    (pkg) =>
      `| ${escapeCell(pkg.name)} | ${escapeCell(pkg.version)} | ${escapeCell(pkg.license)} |`,
  ),
  "",
  "## Cargo 依赖",
  "",
  "| Crate | 版本 | 许可证 |",
  "| --- | --- | --- |",
  ...cargoPackages.map(
    (pkg) =>
      `| ${escapeCell(pkg.name)} | ${escapeCell(pkg.version)} | ${escapeCell(pkg.license)} |`,
  ),
  "",
];

fs.writeFileSync(outputPath, lines.join("\n"), "utf8");

console.log(`Wrote ${path.relative(root, outputPath)}`);
console.log(`npm packages: ${npmPackages.length}`);
console.log(`Cargo packages: ${cargoPackages.length}`);
console.log(`Missing license: ${unknown.length}`);
console.log(`Needs review: ${needsReview.length}`);
if (needsReview.length > 0) {
  console.log(
    `Review license expressions: ${[...new Set(needsReview.map((pkg) => pkg.license))].join("; ")}`,
  );
}
if (unknown.length > 0 || disallowed.length > 0) {
  for (const pkg of unknown) {
    console.error(`UNKNOWN license: ${pkg.ecosystem} ${pkg.name}@${pkg.version}`);
  }
  for (const pkg of disallowed) {
    console.error(`BLOCKED license expression: ${pkg.ecosystem} ${pkg.name}@${pkg.version} (${pkg.license})`);
  }
  process.exitCode = 1;
}
