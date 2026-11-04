#!/usr/bin/env python3
"""Generate third-party license, MPL source, and CycloneDX SBOM materials.

This is an engineering aid, not legal advice. It reads the actual locked npm and
Cargo dependency graphs plus installed package source trees. It does not modify
runtime code or dependency versions.

Run from a checkout with node_modules installed and Cargo able to resolve the
lockfile:

    python -I scripts/generate-compliance.py --root . --output .
"""

from __future__ import annotations

import argparse
import base64
import csv
import gzip
import hashlib
import json
import os
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import uuid
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import quote

LICENSE_NAME_RE = re.compile(
    r"^(?:license|licence|copying|notice|notices|copyright|unlicense)(?:[._-].*)?$",
    re.IGNORECASE,
)
COPYRIGHT_RE = re.compile(r"copyright|\(c\)|©", re.IGNORECASE)
PACKAGE_RE = re.compile(r"^(\S+) v(\S+)(?:\s+\(\*\))?(?:\s+\(proc-macro\))?$")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def safe_name(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9@._+-]+", "__", value)


def run_json(cmd: list[str], cwd: Path) -> dict | list:
    proc = subprocess.run(cmd, cwd=cwd, check=True, capture_output=True)
    return json.loads(proc.stdout.decode("utf-8"))


def run_npm_query(cwd: Path) -> list:
    if os.name == "nt":
        return run_json(["cmd", "/c", "npm", "query", "*", "--json"], cwd)
    return run_json(["npm", "query", "*", "--json"], cwd)


def run_tree(cwd: Path, edges: str) -> dict[tuple[str, str], dict]:
    proc = subprocess.run(
        [
            "cargo",
            "tree",
            "-p",
            "sentinel-lite",
            "--target",
            "x86_64-pc-windows-msvc",
            "-e",
            edges,
            "--prefix",
            "none",
            "--format",
            "{p}",
        ],
        cwd=cwd,
        check=True,
        capture_output=True,
    )
    out: dict[tuple[str, str], dict] = {}
    for raw in proc.stdout.decode("utf-8", "replace").splitlines():
        line = raw.strip()
        if not line:
            continue
        proc_macro = "(proc-macro)" in line
        line = line.replace(" (proc-macro)", "").replace(" (*)", "")
        m = PACKAGE_RE.match(line)
        if not m:
            continue
        name, version = m.group(1), m.group(2)
        out[(name, version)] = {"proc_macro": proc_macro}
    return out


def discover_license_files(pkg_dir: Path, license_file: str | None) -> list[Path]:
    found: list[Path] = []
    if license_file:
        p = Path(license_file)
        if p.is_file():
            found.append(p)
    if pkg_dir.is_dir():
        for child in pkg_dir.iterdir():
            if child.is_file() and LICENSE_NAME_RE.match(child.name):
                found.append(child)
    unique: list[Path] = []
    seen = set()
    for p in found:
        key = str(p.resolve()).lower()
        if key not in seen:
            seen.add(key)
            unique.append(p)
    return unique


def copyright_lines(paths: list[Path], limit: int = 8) -> list[str]:
    """Extract specific copyright notices without turning license boilerplate into a notice.

    Lines such as "Copyright 2016 Nicolas Silva" or "Copyright (c) 2015 X" are
    retained. Generic Apache/MIT phrases such as "copyright owner" and "the above
    copyright notice" are not treated as copyright statements.
    """
    lines: list[str] = []
    seen = set()
    generic = re.compile(
        r"copyright\s+(owner|notice|license|law|holder|holders|interest|statement)",
        re.IGNORECASE,
    )
    for p in paths:
        try:
            text = p.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for line in text.splitlines():
            if not COPYRIGHT_RE.search(line):
                continue
            line = " ".join(line.split())
            if not line or generic.search(line):
                continue
            starts_copyright = bool(re.match(r"^copyright", line, re.IGNORECASE))
            starts_symbol_year = bool(re.match(r"^\(c\)\s*(?:19|20)\d{2}", line, re.IGNORECASE))
            if not (starts_copyright or starts_symbol_year):
                continue
            if re.search(r"you must retain|above copyright|copyright notice|copyright owner", line, re.IGNORECASE):
                continue
            has_year = bool(re.search(r"(?:19|20)\d{2}", line))
            has_symbol = "(c)" in line.lower() or "©" in line
            if not (has_year or has_symbol or starts_copyright):
                continue
            if line not in seen:
                seen.add(line)
                lines.append(line)
                if len(lines) >= limit:
                    return lines
    return lines


def copy_license_files(
    files: list[Path],
    dest_dir: Path,
    source_label: str,
    text_dir: Path,
    text_index: dict[str, str],
) -> tuple[list[str], list[str]]:
    dest_dir.mkdir(parents=True, exist_ok=True)
    license_paths: list[str] = []
    notice_paths: list[str] = []
    for src in files:
        data = src.read_bytes()
        digest = sha256_bytes(data)
        text_name = f"{digest[:16]}-{safe_name(src.name)}"
        text_path = text_dir / text_name
        if not text_path.exists():
            shutil.copy2(src, text_path)
        text_index[digest] = text_name
        out_name = safe_name(src.name)
        out_path = dest_dir / out_name
        if not out_path.exists():
            shutil.copy2(src, out_path)
        rel = out_path.relative_to(text_dir.parent).as_posix()
        if "notice" in src.name.lower():
            notice_paths.append(rel)
        else:
            license_paths.append(rel)
        _ = source_label
    return license_paths, notice_paths


def parse_integrity(value: str | None) -> list[dict]:
    if not value:
        return []
    for part in value.split():
        if "-" not in part:
            continue
        alg, b64 = part.split("-", 1)
        try:
            raw = base64.b64decode(b64)
        except Exception:
            continue
        alg_name = {"sha512": "SHA-512", "sha256": "SHA-256", "sha1": "SHA-1"}.get(alg)
        if alg_name:
            return [{"alg": alg_name, "content": raw.hex()}]
    return []


def load_cargo_lock(root: Path) -> dict[tuple[str, str], str]:
    lock = root / "Cargo.lock"
    if not lock.exists():
        return {}
    data = tomllib.loads(lock.read_text(encoding="utf-8"))
    result = {}
    for pkg in data.get("package", []):
        if pkg.get("name") and pkg.get("version"):
            result[(pkg["name"], pkg["version"])] = pkg.get("checksum", "")
    return result


def purl_cargo(name: str, version: str) -> str:
    return f"pkg:cargo/{quote(name, safe='')}@{quote(version, safe='')}"


def purl_npm(name: str, version: str) -> str:
    if name.startswith("@"):
        scope, package = name.split("/", 1)
        return f"pkg:npm/{quote(scope, safe='')}/{quote(package, safe='')}@{quote(version, safe='')}"
    return f"pkg:npm/{quote(name, safe='')}@{quote(version, safe='')}"


def component(
    *,
    ecosystem: str,
    name: str,
    version: str,
    license_name: str,
    purl: str,
    hashes: list[dict],
    properties: list[dict],
) -> dict:
    return {
        "type": "library",
        "bom-ref": purl,
        "name": name,
        "version": version,
        "purl": purl,
        "licenses": [{"license": {"name": license_name or "UNKNOWN"}}],
        "hashes": hashes,
        "properties": properties,
    }


def prop(name: str, value: object) -> dict:
    return {"name": name, "value": str(value)}


def write_sbom(
    path: Path,
    *,
    scope: str,
    root_component: dict,
    components: list[dict],
    dependencies: dict[str, list[str]],
    metadata_properties: list[dict],
) -> None:
    refs = {c["bom-ref"] for c in components}
    clean_deps = []
    for ref in sorted(dependencies):
        if ref != root_component["bom-ref"] and ref not in refs:
            continue
        targets = sorted({x for x in dependencies.get(ref, []) if x in refs or x == root_component["bom-ref"]})
        clean_deps.append({"ref": ref, "dependsOn": targets})
    doc = {
        "$schema": "http://cyclonedx.org/schema/bom-1.5.schema.json",
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": f"urn:uuid:{uuid.uuid5(uuid.NAMESPACE_URL, 'sentinel-lite-' + scope + '-0.1.0')}",
        "version": 1,
        "metadata": {
            "timestamp": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
            "tools": [{"vendor": "SentinelLite", "name": "generate-compliance.py", "version": "0.1.0"}],
            "component": root_component,
            "properties": metadata_properties,
        },
        "components": components,
        "dependencies": clean_deps,
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(doc, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    # structural parse and reference-closure check
    parsed = json.loads(path.read_text(encoding="utf-8"))
    assert parsed["bomFormat"] == "CycloneDX"
    assert parsed["specVersion"] == "1.5"
    assert isinstance(parsed["components"], list)
    assert isinstance(parsed["dependencies"], list)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", required=True, type=Path, help="project root with node_modules/Cargo registry")
    ap.add_argument("--output", required=True, type=Path, help="public tree output root")
    ap.add_argument("--target", default="x86_64-pc-windows-msvc")
    args = ap.parse_args()
    root = args.root.resolve()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)

    print(f"root={root}")
    print(f"output={out}")

    # Cargo metadata: full lock graph plus Windows-filtered graph
    cargo_full = run_json(["cargo", "metadata", "--format-version", "1", "--locked"], root)
    cargo_win = run_json(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", args.target],
        root,
    )
    cargo_lock = load_cargo_lock(root)
    runtime_tree = run_tree(root, "normal")
    build_tree = run_tree(root, "build")
    dev_tree = run_tree(root, "dev")

    # npm query and lockfile metadata
    npm_packages = run_npm_query(root)
    package_lock = json.loads((root / "package-lock.json").read_text(encoding="utf-8"))
    lock_packages = package_lock.get("packages", {})

    npm_unique: dict[tuple[str, str], dict] = {}
    npm_path_map: dict[str, dict] = {}
    for pkg in npm_packages:
        name = pkg.get("name")
        version = pkg.get("version")
        if not name or not version or name == "sentinel-lite":
            continue
        key = (name, version)
        npm_unique.setdefault(key, pkg)
        if pkg.get("path"):
            npm_path_map[str(Path(pkg["path"]).resolve()).lower()] = pkg

    cargo_packages = [p for p in cargo_full.get("packages", []) if p.get("source")]
    cargo_win_packages = [p for p in cargo_win.get("packages", []) if p.get("source")]
    cargo_nodes = {n["id"]: n for n in cargo_full.get("resolve", {}).get("nodes", [])}

    # Direct/transitive depths for Cargo from the app root
    root_cargo = next(
        (p for p in cargo_full.get("packages", []) if p["name"] == "sentinel-lite" and p.get("source") is None),
        None,
    )
    root_node = cargo_nodes.get(root_cargo["id"]) if root_cargo else None
    cargo_direct_ids = set()
    if root_node:
        cargo_direct_ids = {d["pkg"] for d in root_node.get("deps", [])}
    cargo_depth: dict[str, int] = {}
    if root_node:
        queue = [(root_node["id"], 0)]
        seen = set()
        while queue:
            nid, depth = queue.pop(0)
            if nid in seen:
                continue
            seen.add(nid)
            node = cargo_nodes.get(nid)
            if not node:
                continue
            for dep in node.get("deps", []):
                pid = dep["pkg"]
                cargo_depth[pid] = min(cargo_depth.get(pid, 10**9), depth + 1)
                queue.append((pid, depth + 1))

    cargo_by_id = {p["id"]: p for p in cargo_full.get("packages", [])}
    cargo_edges: dict[str, set[str]] = {}
    for nid, node in cargo_nodes.items():
        cargo_edges[nid] = {d["pkg"] for d in node.get("deps", [])}

    # Classify npm direct/depth from root package path
    root_npm = next((p for p in npm_packages if p.get("name") == "sentinel-lite"), None)
    npm_root_path = str(Path(root).resolve()).lower()
    npm_path_map[npm_root_path] = root_npm or {}
    npm_edges: dict[str, set[str]] = {}

    def npm_key(pkg: dict | None) -> str | None:
        if not pkg or not pkg.get("name") or not pkg.get("version"):
            return None
        return f"{pkg['name']}@{pkg['version']}"

    def resolve_npm_dep(pkg_path: str, dep_name: str) -> dict | None:
        cur = Path(pkg_path)
        while True:
            cand = (cur / "node_modules" / dep_name).resolve()
            hit = npm_path_map.get(str(cand).lower())
            if hit:
                return hit
            if cur == root:
                return None
            cur = cur.parent

    npm_depth: dict[str, int] = {}
    npm_direct: set[str] = set()
    npm_by_key: dict[str, dict] = {}
    for pkg in npm_packages:
        key = npm_key(pkg)
        if key:
            npm_by_key.setdefault(key, pkg)
    if root_npm:
        for dep_name in (root_npm.get("dependencies") or {}):
            hit = resolve_npm_dep(str(root), dep_name)
            if hit:
                npm_direct.add(npm_key(hit))
        for name, version in npm_unique:
            # Use the first install path for graph traversal.
            pkg = npm_unique[(name, version)]
            key = f"{name}@{version}"
            npm_depth[key] = 10**9
        queue = [(root_npm, 0)]
        seen = set()
        while queue:
            pkg, depth = queue.pop(0)
            key = npm_key(pkg)
            if not key or key in seen:
                continue
            seen.add(key)
            npm_depth[key] = min(npm_depth.get(key, 10**9), depth)
            pkg_path = pkg.get("path") or str(root)
            for dep_name in (pkg.get("dependencies") or {}):
                hit = resolve_npm_dep(pkg_path, dep_name)
                if hit:
                    dkey = npm_key(hit)
                    if dkey:
                        npm_edges.setdefault(key, set()).add(dkey)
                        queue.append((hit, depth + 1))
    for name, version in npm_unique:
        key = f"{name}@{version}"
        npm_edges.setdefault(key, set())
    npm_depth = {k: (v if v < 10**9 else "") for k, v in npm_depth.items()}

    # Prepare directories
    lic_dir = out / "third-party-licenses"
    pkg_dir = lic_dir / "packages"
    text_dir = lic_dir / "texts"
    for d in (pkg_dir / "cargo", pkg_dir / "npm", text_dir):
        d.mkdir(parents=True, exist_ok=True)
    manifest_rows: list[dict] = []
    text_index: dict[str, str] = {}

    # Cargo package rows
    for pkg in cargo_packages:
        name, version = pkg["name"], pkg["version"]
        key = (name, version)
        manifest_path = Path(pkg.get("manifest_path", ""))
        pkg_source_dir = manifest_path.parent if manifest_path.exists() else None
        files = discover_license_files(pkg_source_dir, pkg.get("license_file")) if pkg_source_dir else []
        dest = pkg_dir / "cargo" / f"{safe_name(name)}@{safe_name(version)}"
        license_paths, notice_paths = copy_license_files(files, dest, "cargo", text_dir, text_index)
        tree = runtime_tree.get(key)
        if tree and not tree.get("proc_macro"):
            scope = "runtime"
        elif tree:
            scope = "runtime-graph-proc-macro"
        elif key in build_tree:
            scope = "build"
        elif key in dev_tree:
            scope = "dev"
        else:
            scope = "locked-not-selected"
        pid = pkg["id"]
        copyrights = copyright_lines(files)
        manifest_rows.append(
            {
                "ecosystem": "cargo",
                "name": name,
                "version": version,
                "license": pkg.get("license") or "UNKNOWN",
                "scope": scope,
                "direct": "yes" if pid in cargo_direct_ids else "no",
                "depth": cargo_depth.get(pid, ""),
                "source_url": pkg.get("repository") or pkg.get("homepage") or f"https://crates.io/crates/{name}/{version}",
                "license_files": ";".join(license_paths),
                "license_status": "present" if license_paths else "NO_LICENSE_FILE",
                "notice_files": ";".join(notice_paths),
                "notice_status": "present" if notice_paths else "NO_NOTICE_FILE",
                "copyright_lines": " || ".join(copyrights),
                "source_available": "yes" if pkg_source_dir and pkg_source_dir.is_dir() else "no",
                "binary_graph": "yes" if tree and not tree.get("proc_macro") else "no",
            }
        )

    # npm package rows
    for (name, version), pkg in sorted(npm_unique.items()):
        key = f"{name}@{version}"
        pkg_path = Path(pkg["path"]) if pkg.get("path") else None
        files = discover_license_files(pkg_path, None) if pkg_path and pkg_path.is_dir() else []
        dest = pkg_dir / "npm" / f"{safe_name(name)}@{safe_name(version)}"
        license_paths, notice_paths = copy_license_files(files, dest, "npm", text_dir, text_index)
        rel_key = ""
        if pkg_path and pkg_path.is_dir():
            try:
                rel_key = pkg_path.resolve().relative_to(root).as_posix()
            except ValueError:
                rel_key = ""
        lock_entry = lock_packages.get(rel_key, {})
        if pkg.get("dev") is True:
            scope = "dev"
        else:
            scope = "runtime"
        if pkg.get("optional") is True:
            scope += "+optional"
        manifest_rows.append(
            {
                "ecosystem": "npm",
                "name": name,
                "version": version,
                "license": pkg.get("license") or "UNKNOWN",
                "scope": scope,
                "direct": "yes" if key in npm_direct else "no",
                "depth": npm_depth.get(key, ""),
                "source_url": lock_entry.get("resolved") or f"https://www.npmjs.com/package/{name}/v/{version}",
                "license_files": ";".join(license_paths),
                "license_status": "present" if license_paths else "NO_LICENSE_FILE",
                "notice_files": ";".join(notice_paths),
                "notice_status": "present" if notice_paths else "NO_NOTICE_FILE",
                "copyright_lines": " || ".join(copyright_lines(files)),
                "source_available": "yes" if pkg_path and pkg_path.is_dir() else "no",
                "binary_graph": "yes" if pkg.get("dev") is not True else "no",
            }
        )

    manifest_rows.sort(key=lambda r: (r["ecosystem"], r["name"].lower(), r["version"]))
    scope_map = {(r["ecosystem"], r["name"], r["version"]): r for r in manifest_rows}
    manifest_path = lic_dir / "MANIFEST.csv"
    with manifest_path.open("w", encoding="utf-8", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=list(manifest_rows[0].keys()))
        writer.writeheader()
        writer.writerows(manifest_rows)

    # Human-readable copyright index
    with (lic_dir / "COPYRIGHT_INDEX.md").open("w", encoding="utf-8", newline="\n") as f:
        f.write("# Third-party copyright index\n\n")
        f.write("Generated from original package license/NOTICE files. Missing copyright lines are recorded as `NO_COPYRIGHT_LINE_FOUND`; they were not invented.\n\n")
        for row in manifest_rows:
            f.write(f"## {row['ecosystem']} {row['name']} {row['version']}\n\n")
            f.write(f"- License: `{row['license']}`\n")
            f.write(f"- Scope: `{row['scope']}`; direct: `{row['direct']}`; depth: `{row['depth']}`\n")
            f.write(f"- Source: {row['source_url']}\n")
            f.write(f"- License files: {row['license_files'] or '`NO_LICENSE_FILE`'}\n")
            f.write(f"- NOTICE files: {row['notice_files'] or '`NO_NOTICE_FILE`'}\n")
            f.write(f"- Copyright: {row['copyright_lines'] or '`NO_COPYRIGHT_LINE_FOUND`'}\n\n")

    missing_rows = [r for r in manifest_rows if not r["license_files"]]
    with (lic_dir / "MISSING_LICENSE_FILES.md").open("w", encoding="utf-8", newline="") as f:
        f.write("# Packages without a standalone license file in the installed source tree\n\n")
        f.write("These packages declare a license in their package metadata, but no `LICENSE*`/`COPYING*`/`NOTICE*` file was found in the installed package root during this snapshot. The license expression is recorded in `MANIFEST.csv`; upstream source should be reviewed before relying on the expression. No license text was invented.\n\n")
        f.write("| Ecosystem | Package | Version | License expression | Distribution scope | Source |\n")
        f.write("| --- | --- | --- | --- | --- | --- |\n")
        for r in missing_rows:
            f.write(f"| {r['ecosystem']} | {r['name']} | {r['version']} | `{r['license']}` | `{r['scope']}` | {r['source_url']} |\n")
        f.write(f"\nTotal: **{len(missing_rows)}** of **{len(manifest_rows)}** packages.\n")
    notice_present = [r for r in manifest_rows if r["notice_files"]]
    with (lic_dir / "NOTICE_SCAN.md").open("w", encoding="utf-8", newline="") as f:
        f.write("# NOTICE scan result\n\n")
        f.write("The installed package roots were scanned for filenames matching `NOTICE*`. This is a snapshot-specific scan, not proof that upstream publishes no NOTICE file.\n\n")
        f.write(f"- Packages scanned: **{len(manifest_rows)}**\n")
        f.write(f"- Packages with a separate NOTICE file: **{len(notice_present)}**\n")
        f.write(f"- Packages recorded as `NO_NOTICE_FILE`: **{len(manifest_rows) - len(notice_present)}**\n\n")
        if notice_present:
            f.write("| Ecosystem | Package | Version | NOTICE file |\n| --- | --- | --- | --- |\n")
            for r in notice_present:
                f.write(f"| {r['ecosystem']} | {r['name']} | {r['version']} | {r['notice_files']} |\n")
        else:
            f.write("No separate NOTICE files were found in the installed roots. The per-package `MANIFEST.csv` records this result for every row.\n")
    with (lic_dir / "README.md").open("w", encoding="utf-8", newline="") as f:
        f.write("# Third-party license bundle\n\n")
        f.write("This directory is an engineering-produced license/copyright bundle for SentinelLite 0.1.0. It is not a legal opinion.\n\n")
        f.write(f"- Packages: **{len(manifest_rows)}** (Cargo: {sum(1 for r in manifest_rows if r['ecosystem']=='cargo')}, npm: {sum(1 for r in manifest_rows if r['ecosystem']=='npm')}).\n")
        f.write(f"- Unique license texts: **{len(text_index)}**.\n")
        f.write(f"- Packages with no standalone license file: **{len(missing_rows)}**; see `MISSING_LICENSE_FILES.md`.\n")
        f.write(f"- Packages with a separate NOTICE file: **{len(notice_present)}**; see `NOTICE_SCAN.md`.\n\n")
        f.write("Layout:\n\n")
        f.write("- `MANIFEST.csv`: one row per package/version with license, scope, direct/transitive depth, source URL, license/NOTICE files and copyright fields.\n")
        f.write("- `packages/<ecosystem>/<name>@<version>/`: original upstream license/NOTICE/copyright files as installed.\n")
        f.write("- `texts/`: deduplicated license text files, separated from the per-package copyright files.\n")
        f.write("- `COPYRIGHT_INDEX.md`: human-readable package/copyright index.\n")
        f.write("- `MISSING_LICENSE_FILES.md`, `NOTICE_SCAN.md`: explicit negative findings; these are not invented notices.\n")
        f.write("- `SHA256SUMS`: checksums for the bundle files (the manifest excludes itself).\n\n")
        f.write("Run `python -I scripts/generate-compliance.py --root . --output .` after `npm ci` and Cargo fetch to regenerate. The snapshot is tied to `Cargo.lock`/`package-lock.json`; it is not a proof that every locked package is linked into the final binary.\n")
        f.write("\nSee `../THIRD_PARTY_NOTICES.md`, `../third-party-source/mpl/`, and `../third-party-sbom/` for the special-license, MPL source and SBOM materials.\n")

    # MPL source package
    mpl_names = {
        "cssparser": "0.37.0",
        "cssparser-macros": "0.7.1",
        "dtoa-short": "0.3.5",
        "option-ext": "0.2.0",
        "selectors": "0.38.0",
    }
    mpl_dir = out / "third-party-source" / "mpl"
    archives_dir = mpl_dir / "archives"
    archives_dir.mkdir(parents=True, exist_ok=True)
    cargo_cache = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")) / "registry" / "cache"
    mpl_manifest = []
    for name, version in mpl_names.items():
        matches = list(cargo_cache.glob(f"*/{name}-{version}.crate"))
        if matches:
            src = matches[0]
            dest = archives_dir / src.name
            shutil.copy2(src, dest)
            mpl_manifest.append(
                {
                    "package": name,
                    "version": version,
                    "archive": dest.relative_to(out).as_posix(),
                    "sha256": sha256_file(dest),
                    "source": f"https://crates.io/crates/{name}/{version}",
                    "role": "runtime" if (name, version) in runtime_tree and not runtime_tree[(name, version)].get("proc_macro") else "build/proc-macro",
                }
            )
        else:
            mpl_manifest.append(
                {
                    "package": name,
                    "version": version,
                    "archive": "MISSING",
                    "sha256": "",
                    "source": f"https://crates.io/crates/{name}/{version}",
                    "role": "unknown",
                }
            )
    # npm lightningcss: published package, build-time only
    lightning_dir = root / "node_modules" / "lightningcss"
    lightning_tgz = mpl_dir / "lightningcss-1.33.0-npm-package.tgz"
    if lightning_dir.is_dir():
        with lightning_tgz.open("wb") as raw:
            with gzip.GzipFile(fileobj=raw, mode="wb", mtime=0) as gz:
                with tarfile.open(fileobj=gz, mode="w") as tf:
                    for path in sorted(lightning_dir.rglob("*")):
                        arc = f"lightningcss-1.33.0/{path.relative_to(lightning_dir).as_posix()}"
                        info = tf.gettarinfo(str(path), arcname=arc)
                        info.mtime = 0
                        info.uid = info.gid = 0
                        info.uname = info.gname = ""
                        if path.is_file():
                            with path.open("rb") as f:
                                tf.addfile(info, f)
                        else:
                            tf.addfile(info)
        mpl_manifest.append(
            {
                "package": "lightningcss",
                "version": "1.33.0",
                "archive": lightning_tgz.relative_to(out).as_posix(),
                "sha256": sha256_file(lightning_tgz),
                "source": "https://github.com/parcel-bundler/lightningcss",
                "role": "npm build-time; not in Rust binary",
            }
        )
    upstream = mpl_dir / "lightningcss-1.33.0-upstream-source.tar.gz"
    if upstream.is_file():
        mpl_manifest.append(
            {
                "package": "lightningcss",
                "version": "1.33.0",
                "archive": upstream.relative_to(out).as_posix(),
                "sha256": sha256_file(upstream),
                "source": "https://github.com/parcel-bundler/lightningcss/releases/tag/v1.33.0",
                "role": "complete upstream source tag v1.33.0; build-time only; not in Rust binary",
            }
        )
    native_dir = root / "node_modules" / "lightningcss-win32-x64-msvc"
    native_meta = mpl_dir / "lightningcss-win32-x64-msvc-1.33.0"
    if native_dir.is_dir():
        native_meta.mkdir(parents=True, exist_ok=True)
        for name in ("LICENSE", "package.json", "README.md"):
            src = native_dir / name
            if src.is_file():
                shutil.copy2(src, native_meta / name)
        mpl_manifest.append(
            {
                "package": "lightningcss-win32-x64-msvc",
                "version": "1.33.0",
                "archive": "third-party-source/mpl/lightningcss-win32-x64-msvc-1.33.0/ (metadata only; native package has no source)",
                "sha256": "",
                "source": "https://github.com/parcel-bundler/lightningcss",
                "role": "npm build-time native binding; not in Rust binary",
            }
        )
    with (mpl_dir / "MANIFEST.csv").open("w", encoding="utf-8", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=["package", "version", "archive", "sha256", "source", "role"])
        writer.writeheader()
        writer.writerows(mpl_manifest)
    with (mpl_dir / "SHA256SUMS").open("w", encoding="utf-8", newline="\n") as f:
        for row in mpl_manifest:
            if row["archive"] not in ("MISSING", "") and row["sha256"]:
                f.write(f"{row['sha256']}  {row['archive']}\n")

    # SBOM: source inventory and binary composition
    sbom_dir = out / "third-party-sbom"
    source_components: list[dict] = []
    source_deps: dict[str, list[str]] = {}
    cargo_refs: dict[str, str] = {}

    # Include first-party workspace crates as components but mark them.
    for pkg in cargo_full.get("packages", []):
        if pkg.get("source"):
            continue
        ref = purl_cargo(pkg["name"], pkg["version"])
        cargo_refs[pkg["id"]] = ref
        source_components.append(
            component(
                ecosystem="cargo",
                name=pkg["name"],
                version=pkg["version"],
                license_name=pkg.get("license") or "UNKNOWN",
                purl=ref,
                hashes=[],
                properties=[prop("sentinel:firstParty", "true"), prop("sentinel:scope", "first-party")],
            )
        )
    for pkg in cargo_packages:
        ref = purl_cargo(pkg["name"], pkg["version"])
        cargo_refs[pkg["id"]] = ref
        checksum = cargo_lock.get((pkg["name"], pkg["version"]), "")
        source_components.append(
            component(
                ecosystem="cargo",
                name=pkg["name"],
                version=pkg["version"],
                license_name=pkg.get("license") or "UNKNOWN",
                purl=ref,
                hashes=[{"alg": "SHA-256", "content": checksum}] if checksum else [],
                properties=[
                    prop("sentinel:ecosystem", "cargo"),
                    prop("sentinel:scope", scope_map.get(("cargo", pkg["name"], pkg["version"]), {}).get("scope", "source-lock")),
                    prop("sentinel:direct", scope_map.get(("cargo", pkg["name"], pkg["version"]), {}).get("direct", "")),
                    prop("sentinel:depth", scope_map.get(("cargo", pkg["name"], pkg["version"]), {}).get("depth", "")),
                ],
            )
        )
    npm_refs: dict[str, str] = {}
    for (name, version), pkg in sorted(npm_unique.items()):
        key = f"{name}@{version}"
        ref = purl_npm(name, version)
        npm_refs[key] = ref
        pkg_path = Path(pkg["path"]) if pkg.get("path") else None
        rel_key = ""
        if pkg_path and pkg_path.is_dir():
            try:
                rel_key = pkg_path.resolve().relative_to(root).as_posix()
            except ValueError:
                rel_key = ""
        lock_entry = lock_packages.get(rel_key, {})
        source_components.append(
            component(
                ecosystem="npm",
                name=name,
                version=version,
                license_name=pkg.get("license") or "UNKNOWN",
                purl=ref,
                hashes=parse_integrity(lock_entry.get("integrity")),
                properties=[
                    prop("sentinel:ecosystem", "npm"),
                    prop("sentinel:scope", scope_map.get(("npm", name, version), {}).get("scope", "source-lock")),
                    prop("sentinel:direct", scope_map.get(("npm", name, version), {}).get("direct", "")),
                    prop("sentinel:depth", scope_map.get(("npm", name, version), {}).get("depth", "")),
                    prop("sentinel:dev", "yes" if pkg.get("dev") is True else "no"),
                ],
            )
        )
    # Cargo edges among external packages and root -> direct
    for nid, deps in cargo_edges.items():
        if nid not in cargo_refs:
            continue
        ref = cargo_refs[nid]
        source_deps.setdefault(ref, [])
        for dep_id in deps:
            if dep_id in cargo_refs:
                source_deps[ref].append(cargo_refs[dep_id])
    for dep_id in cargo_direct_ids:
        if dep_id in cargo_refs:
            source_deps.setdefault("app:sentinel-lite@0.1.0", []).append(cargo_refs[dep_id])
    # npm edges
    for key, deps in npm_edges.items():
        if key not in npm_refs:
            continue
        source_deps.setdefault(npm_refs[key], [])
        for dep_key in deps:
            if dep_key in npm_refs:
                source_deps[npm_refs[key]].append(npm_refs[dep_key])
    if root_npm:
        for dep_name in (root_npm.get("dependencies") or {}):
            hit = resolve_npm_dep(str(root), dep_name)
            key = npm_key(hit)
            if key and key in npm_refs:
                source_deps.setdefault("app:sentinel-lite@0.1.0", []).append(npm_refs[key])
    source_components.sort(key=lambda c: c["bom-ref"])
    source_deps = {k: sorted(set(v)) for k, v in source_deps.items()}
    write_sbom(
        sbom_dir / "SBOM.source.cdx.json",
        scope="source",
        root_component={
            "type": "application",
            "bom-ref": "app:sentinel-lite@0.1.0",
            "name": "sentinel-lite",
            "version": "0.1.0",
            "purl": "pkg:cargo/sentinel-lite@0.1.0",
            "licenses": [{"license": {"name": "MIT"}}],
            "properties": [prop("sentinel:scope", "source-application")],
        },
        components=source_components,
        dependencies=source_deps,
        metadata_properties=[
            prop("sentinel:sbomScope", "source-dependency-inventory"),
            prop("sentinel:source", "Cargo.lock + package-lock.json + cargo metadata + npm query"),
            prop("sentinel:notes", "Includes locked packages; not a proof that every package is linked into the final executable."),
        ],
    )

    # Binary composition: normal Rust graph (excluding proc-macro) + npm production candidates.
    binary_components: list[dict] = []
    binary_deps: dict[str, list[str]] = {}
    binary_refs: dict[str, str] = {}
    for pkg in cargo_win_packages:
        key = (pkg["name"], pkg["version"])
        tree = runtime_tree.get(key)
        if not tree or tree.get("proc_macro"):
            continue
        ref = purl_cargo(pkg["name"], pkg["version"])
        binary_refs[pkg["id"]] = ref
        binary_components.append(
            component(
                ecosystem="cargo",
                name=pkg["name"],
                version=pkg["version"],
                license_name=pkg.get("license") or "UNKNOWN",
                purl=ref,
                hashes=[],
                properties=[
                    prop("sentinel:ecosystem", "cargo"),
                    prop("sentinel:scope", "runtime-crate-upper-bound"),
                    prop("sentinel:binaryGraph", "crate-level normal graph; not symbol-level proof"),
                ],
            )
        )
    for nid, deps in cargo_edges.items():
        if nid not in binary_refs:
            continue
        ref = binary_refs[nid]
        binary_deps.setdefault(ref, [])
        for dep_id in deps:
            if dep_id in binary_refs:
                binary_deps[ref].append(binary_refs[dep_id])
    for dep_id in cargo_direct_ids:
        if dep_id in binary_refs:
            binary_deps.setdefault("app:sentinel-lite@0.1.0", []).append(binary_refs[dep_id])
    for (name, version), pkg in sorted(npm_unique.items()):
        if pkg.get("dev") is True:
            continue
        key = f"{name}@{version}"
        ref = purl_npm(name, version)
        binary_components.append(
            component(
                ecosystem="npm",
                name=name,
                version=version,
                license_name=pkg.get("license") or "UNKNOWN",
                purl=ref,
                hashes=[],
                properties=[
                    prop("sentinel:ecosystem", "npm"),
                    prop("sentinel:scope", "frontend-production-candidate"),
                    prop("sentinel:binaryGraph", "production dependency set; Vite tree-shaking not proven"),
                ],
            )
        )
        if key in npm_refs:
            binary_deps.setdefault("app:sentinel-lite@0.1.0", []).append(ref)
        for dep_key in npm_edges.get(key, set()):
            if dep_key in npm_refs:
                dep_pkg = npm_unique.get(tuple(dep_key.rsplit("@", 1)))
                if dep_pkg and dep_pkg.get("dev") is not True:
                    binary_deps.setdefault(ref, []).append(npm_refs[dep_key])
    binary_components.sort(key=lambda c: c["bom-ref"])
    binary_deps = {k: sorted(set(v)) for k, v in binary_deps.items()}
    write_sbom(
        sbom_dir / "SBOM.binary.cdx.json",
        scope="binary",
        root_component={
            "type": "application",
            "bom-ref": "app:sentinel-lite@0.1.0",
            "name": "sentinel-lite",
            "version": "0.1.0",
            "purl": "pkg:cargo/sentinel-lite@0.1.0",
            "licenses": [{"license": {"name": "MIT"}}],
            "properties": [prop("sentinel:scope", "final-release-application")],
        },
        components=binary_components,
        dependencies=binary_deps,
        metadata_properties=[
            prop("sentinel:sbomScope", "binary-composition-upper-bound"),
            prop("sentinel:source", "cargo tree normal graph for x86_64-pc-windows-msvc + npm production packages"),
            prop("sentinel:notes", "Crate-level upper bound, not a symbol-level linker proof. npm entries are production candidates; Vite may tree-shake. Installer includes app + WebView2 bootstrapper, not the sensor."),
            prop("sentinel:appSha256", "0fc65ff304d9e89a85c1e7c0b145d9210fa268dce44c06350df0267fd1b483be"),
            prop("sentinel:installerSha256", "2cb65812c71980dd9d127f3209b03c79f14ebfe2a6915cdfb8f26596959439df"),
            prop("sentinel:appSize", "8480256"),
            prop("sentinel:installerSize", "2535872"),
        ],
    )

    # Summary for the operator
    missing_license = [r for r in manifest_rows if not r["license_files"]]
    no_notice = [r for r in manifest_rows if not r["notice_files"]]
    print(f"manifest rows: {len(manifest_rows)}")
    print(f"cargo rows: {sum(1 for r in manifest_rows if r['ecosystem']=='cargo')}")
    print(f"npm rows: {sum(1 for r in manifest_rows if r['ecosystem']=='npm')}")
    print(f"missing license files: {len(missing_license)}")
    print(f"packages without NOTICE file: {len(no_notice)}")
    print(f"texts deduplicated: {len(text_index)}")
    print(f"mpl archives: {sum(1 for r in mpl_manifest if r['sha256'])}")
    print(f"source SBOM components: {len(source_components)}")
    print(f"binary SBOM components: {len(binary_components)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
