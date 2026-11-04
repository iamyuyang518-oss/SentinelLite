# SentinelLite 0.1.0 source-only release notes

Status: **source-only release candidate; GitHub publication not completed.**

This release is **source-only**. It does not provide a Windows installer, a prebuilt
`sentinel-lite.exe`, a `SentinelLiteSensor.exe`, or any other executable build output.
There is nothing to install directly from this repository; users must prepare a
supported development environment and build from source.

## What is included

- SentinelLite's own Rust, TypeScript, React, Tauri, test, and build configuration
  source.
- `Cargo.toml`, `Cargo.lock`, `package.json`, and `package-lock.json`.
- README, architecture, security, privacy, validation, and contribution documentation.
- `THIRD_PARTY_NOTICES.md` and the per-package license/copyright bundle.
- Exact-version MPL-2.0 source archives and the npm `lightningcss` source/provenance
  materials.
- `SOURCE_ONLY_LICENSE_NOTICE.md`.
- `third-party-sbom/SBOM.source.cdx.json`, the source dependency inventory.
- `SHA256SUMS.txt` for the source-only public tree. It records the LF-normalized Git
  blob content. Because `.gitattributes` requests `eol=lf`, a Windows checkout may show
  CRLF while the repository stores LF; verify with a clean clone using
  `git -c core.autocrlf=false clone ...` or from the GitHub source archive.

## What is not included

- No Windows installer or MSI.
- No prebuilt `sentinel-lite.exe`.
- No prebuilt `SentinelLiteSensor.exe`.
- No `.exe`, `.dll`, `.pdb`, `.obj`, `.lib`, `.node`, `.msi`, or static-library
  binaries.
- No `target/`, `node_modules/`, `dist/`, database, log, or local test data.
- No original development-repository `.git` history.

`third-party-sbom/SBOM.binary.cdx.json` is retained only as a historical record of an
earlier binary build. It is not an asset list for this source-only release and does not
describe downloadable files.

## Build from source

The build prerequisites and commands are in `README.md`. In summary:

```powershell
npm ci
npm run build
npx tauri build --no-bundle
```

The optional administrator sensor is built separately:

```powershell
cargo build --release -p sentinel-sensor
```

These commands produce local build outputs under `target/`; those outputs are not part
of this repository or release. The public release does not supply the resulting
binaries.

## Product scope

SentinelLite 0.1.0 is an early, local-first, read-only Windows security inspection
preview. It shows what it can observe, what it cannot observe, and why. It is not an
antivirus, does not claim an overall "protected" state, and does not change system
security settings.

Capabilities in the source include:

- Read-only installed-software, startup, service, scheduled-task, and process
  inventories.
- User-triggered read-only Windows Defender and firewall status snapshots.
- Optional process start/stop telemetry from a separate administrator-run ETW sensor,
  local SQLite event history, and basic explainable analysis.
- Static path associations and a read-only security-posture overview without a score or
  "protected" verdict.
- Light/dark themes, pagination, date filters, and explicit partial/unknown states.

## Runtime limitations that remain

- **The app cannot enable real-time protection in-app.** There is no in-app sensor
  start action and no application-level UAC, `runas`, or elevation path.
- **The app cannot stop the sensor.** 0.1.0 provides no in-app stop or restart control.
  Closing the app does not stop an already running sensor.
- **The source-only release does not provide the sensor.** An administrator must build
  it from source and start it manually outside the application.
- The sensor covers process start/stop events only. It does not provide continuous file,
  network, DLL, service, scheduled-task, or registry monitoring.
- The app does not block, terminate, quarantine, uninstall, delete, or modify anything.
- The overall protection state remains unknown. A complete source read does not mean the
  machine is safe.
- The local SQLite event database is not encrypted and is not a tamper-proof audit log.
- AppX/MSIX/Store apps, portable apps, browser extensions, and uninstall leftovers are
  not covered.
- There is no malware-detection engine, malware-sample validation, or measured
  false-positive/false-negative rate.

## Supported build environment

- Windows 10 or Windows 11, x64.
- Microsoft Edge WebView2 Runtime is required to run a locally built app. Windows 11
  normally includes it; Windows 10 may need it installed separately.
- Node.js 22.13 or newer, npm, Rust stable with the MSVC target, and Visual Studio 2022
  Build Tools with the Desktop C++ workload.
- The main application runs as a standard user.
- The optional sensor requires administrator rights to create the ETW session.

The current project has been validated only on Windows 11 Pro 10.0.26200 x64. Windows 10,
ARM64, enterprise-policy environments, multi-DPI automation, and clean-VM builds are
not fully validated.

## Source-only license position

This repository distributes Source Code Form only. It does not distribute an Executable
Form, so MPL-2.0 Section 3.2's executable-form source-availability mechanics are not
triggered by this release. MPL-2.0 Section 3.1 is addressed by including exact source
archives, the MPL-2.0 text, upstream license files where available, and
`SOURCE_ONLY_LICENSE_NOTICE.md`.

`selectors@0.38.0` has no standalone upstream license file. Its exact source archive is
included; its 16 Rust source files carry the standard MPL-2.0 header; the crate metadata
declares `MPL-2.0`; and the upstream README states that Stylo is MPL-2.0. The exact
evidence is in `third-party-licenses/packages/cargo/selectors@0.38.0/`.

The native `lightningcss-win32-x64-msvc` metadata is retained only for license and
provenance documentation. Its prebuilt `.node` file is not included. The
`third-party-source/mpl/MPL_SOURCE_OFFER.md` template is not an active offer for this
source-only release.

This is an engineering assessment, not a legal review or a compliance certification.
The remaining license-inventory gaps and legal questions are recorded in
`RELEASE_RISKS.md` and `third-party-licenses/COPYRIGHT_REVIEW.md`.

## Verification completed for this source-only tree

- The public tree contains no `.exe`, `.dll`, `.pdb`, `.obj`, `.lib`, `.node`, `.msi`,
  static-library, database, log, or `target/` artifact.
- No real developer username, hostname, or development-machine path was found in the
  public tree.
- The public tree has no `.git` directory and does not inherit the development
  repository history.
- Relative Markdown links resolve.
- `SHA256SUMS.txt` covers the LF-normalized Git source content; a clean checkout or
  `git archive` extraction is the canonical verification target.
- The MPL source archives and license materials have SHA-256 manifests.
- `SBOM.source.cdx.json` passes the official CycloneDX 1.5 JSON schema validation with
  0 errors: 642 components and 643 dependency entries.
- The source SBOM is a locked dependency inventory, not a file-by-file list of every
  archive included in this repository.

## Residual risks

- `selectors@0.38.0` has no standalone upstream license file; the available source
  headers, metadata, README statement, MPL text, and exact archive are recorded rather
  than replaced with an invented file.
- 39 of 635 packages have no standalone license file in their installed package root;
  134 rows lack a specific copyright line. These are documented in the license review
  files and remain a legal/documentation risk.
- No formal legal review, third-party security audit, penetration test, fuzzing campaign,
  or malware-sample evaluation has been completed.
- The project is not a substitute for Windows Defender and is not an EDR product.

Do not describe this repository as "license-compliant", "legally reviewed", "fully
protected", or "secure". No public GitHub repository or Release was created in this
pass because GitHub publishing credentials/tools were not available.
