# Changelog

All notable changes to SentinelLite are recorded here. The project follows a simple pre-1.0 versioning policy: minor versions may change behavior and data contracts.

## [0.1.0] - 2026-11-04 (source-only release candidate; publication pending)

> This release is source-only. It does not provide a Windows installer, a prebuilt
> `sentinel-lite.exe`, a `SentinelLiteSensor.exe`, or any other executable build output.
> Build from source; see `RELEASE_NOTES.md`.

### Added

- Read-only software inventory from HKLM 32/64-bit and a single shared HKCU Uninstall view, with source/item observation status, field-state tracking, bounded item/string/time limits, and a path-clue-based DisplayIcon field.
- Read-only startup inventory for Run/RunOnce and per-user/all-user startup folders, including registry value types and read-only `.lnk` target resolution. Shortcut arguments and working directories are recorded only as present/absent.
- Read-only service and scheduled-task inventory with per-item access/partial status and bounded enumeration. Service parameters, task arguments, working directories, and triggers are not read.
- Read-only process snapshot with creation-time identity tokens, CPU affinity inspection, disk signature state, file metadata, bounded SHA-256, and a short CPU sample for a selected process.
- Read-only Windows Defender and firewall snapshots, triggered by the user, with explicit partial/unknown/access-denied states.
- Optional administrator process sensor (`SentinelLiteSensor.exe`) using Kernel-Process ETW start/stop, normalized event identities, start/stop pairing, bounded queues, health counters, and a restricted named-pipe status/event protocol.
- Local SQLite process-event history with idempotent ingestion, pairing revisions, session/gap records, retention limits, and read-only queries.
- Live process-event history, event detail, and explainable behavior-analysis views based on local process events.
- Static path associations between software and processes, services, scheduled tasks, and startup items, with exact/contained/weak labels, multiple-candidate preservation, and insufficient-evidence states.
- Read-only security posture view that combines existing native-protection and asset-association snapshots without producing a score or an overall "protected" verdict.
- Light/dark theme, expandable sections, date-range filters, pagination, and explicit loading/empty/partial/unknown states.

### Security and privacy

- Tauri application-level command ACL with 16 fixed commands; no `core:default`, Shell, filesystem, or HTTP plugin exposure.
- No in-app elevation path: 0.1.0 does not register `start_sensor`; no application code calls `ShellExecuteW` or `runas` for sensor startup, and there is no UAC launch path. The optional sensor is started manually by an administrator outside the app.
- Presentation-boundary redaction for user-profile paths, personal service/task accounts, task names/paths/IDs, software paths, startup targets, risk evidence, and association evidence.
- Release builds use `--remap-path-prefix` to remove the build user's Windows profile path from panic location metadata; see `scripts/build-release.ps1`.
- Bounded registry, process, association, event, and query inputs; truncation and access denial remain visible.
- No telemetry, analytics, advertising, account system, cloud upload, or update check.
- Local SQLite database is ACL-protected but not encrypted and not tamper-proof.

### Fixed

- Task Scheduler collection-count failures and truncation are now surfaced as explicit partial/truncated states instead of being silently interpreted as zero tasks or a safe limit.
- Task action-count failures are retained as issues instead of treating an unreadable action list as empty.
- Incomplete scheduled-task sources now prevent false "no association" conclusions while preserving evidence from other sources.
- Personal-account and user-profile path segments are redacted at the Tauri command boundary for services, tasks, software, startup items, and associations.

### Known limitations

- Not an antivirus. No malware detection engine, sample validation, false-positive/false-negative rate, or remediation.
- No blocking, termination, quarantine, uninstall, deletion, registry modification, or system-setting change.
- No 0-100 security score and no overall "protected" state. The overall protection state remains unknown.
- The sensor covers process start/stop only. No file, network, DLL, service, task, or registry change monitoring.
- The source-only release does not provide an installer or prebuilt binaries. A locally built 0.1.0 installer does not bundle `SentinelLiteSensor.exe`; an administrator must build and start the sensor manually outside the app. 0.1.0 provides no in-app start/stop or UAC elevation action.
- No signed binary/installer, code signing, auto-update, or reproducible-build proof. The source SBOM is included; the binary SBOM is a historical build record.
- No third-party security audit, penetration test, large-scale fuzzing, or malware-sample evaluation.
- AppX/MSIX/Store apps, portable apps, browser extensions, and uninstall leftovers are not covered.
- Windows 10, ARM64, enterprise policy environments, and multi-DPI automation are not fully validated.
- The application process tree has exceeded the 100 MB memory target in an idle measurement.
- `npm audit` reports 11 moderate findings in the development toolchain; production dependencies report zero known vulnerabilities. A RustSec advisory-db cross-check found three low-exposure DoS advisories in transitive Tauri dependencies (`quick-xml` and `time`); the targeted fix requires Rust 1.88 and a Tauri compatibility pass and was not applied.

