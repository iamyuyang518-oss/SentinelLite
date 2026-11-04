# Security Policy

## Project status

SentinelLite v0.1.0 is an early public preview. It is a local-first, read-only Windows inspection tool. It has not been audited by a third party and has not been penetration-tested, fuzzed at scale, or evaluated against a malware corpus. It cannot replace Windows Defender or any other security product.

The main application is intentionally read-only: it does not remove software, block processes, change Defender or firewall settings, or modify system configuration. The optional administrator sensor reads Kernel-Process ETW start/stop events only.

## Supported versions

| Version | Supported |
| --- | --- |
| 0.1.x | Security fixes are applied to the latest 0.1.x code line. |
| Older versions | Not supported. |

There is no long-term support commitment and no guaranteed response time.

## Reporting a vulnerability

Use GitHub's **private vulnerability reporting / Security Advisory** feature for this repository. Enable it before the repository is made public. Do not disclose exploit details, bypasses, credentials, personal data, or real user information in a public issue, discussion, screenshot, or pull request.

If private reporting is not yet enabled, open a minimal public issue asking the maintainers to provide a private channel, without including technical details.

A useful report includes:

- The affected version or commit.
- Windows version and architecture.
- The smallest reproducible steps.
- The security impact and the boundary that is crossed.
- Sanitized logs or a minimal proof of concept that does not contain third-party personal data or a destructive payload.

Do not attach live malware, another person's personal data, or destructive payloads.

## In scope

- The Tauri command layer and application-level ACL.
- The named-pipe DACL, mandatory integrity label, client/server identity checks, message limits, and event/status protocol.
- Local SQLite path selection, ACL inheritance, schema migration, retention, corruption handling, and query bounds. The database is not encrypted and is not a tamper-proof audit log.
- Tauri configuration, CSP, capabilities, and WebView isolation.
- Read-only Windows API usage, path parsing, registry parsing, input bounds, and error handling.
- Process creation-time identity handling and the four-state path model.
- The optional sensor's ETW decoding, pairing, queue bounds, and shutdown behavior.
- Dependency locking, CI permissions, and supply-chain configuration.
- UI states that could misrepresent partial, unknown, stale, or unavailable data as safe.

## Out of scope

- Real-time protection, malware removal, process blocking, quarantine, or remediation. These features do not exist in 0.1.0.
- Kernel drivers, anti-rootkit functionality, and defense against an attacker who already has administrator or kernel access.
- Auditing third-party software vulnerabilities.
- Bypassing Windows access controls by elevating the main application.
- Treating a disk signature or SHA-256 value as proof that a running process is trustworthy.
- Automatic uninstall, deletion, or disabling of system components.

## Current security boundaries

- The main window runs as a standard user. It does not use a `requireAdministrator` manifest and does not elevate itself silently.
- The WebView can invoke 16 fixed commands through a Tauri application-level ACL. The capability applies to the main window and does not grant `core:default`. No Shell, filesystem, or HTTP plugin is exposed.
- 0.1.0 does not register `start_sensor`; no application code calls `ShellExecuteW` or `runas` for sensor startup, and there is no UAC launch path. The optional sensor must be started manually by an administrator outside the application. The `windows` crate's read-only ShellLink support may leave an unused `ShellExecuteW` import symbol in the linked binary; it has no application call site.
- The sensor runs elevated, creates a real-time ETW session for process start/stop, and exposes read-only status and event batches over a restricted named pipe. It does not expose stop, configuration, or arbitrary-command operations through IPC.
- Inventory results are bounded by item counts, string lengths, and time limits. Partial reads, access denial, truncation, and unavailable sources remain visible; they are not converted into a clean result.
- Service accounts, task identities, task names, software paths, startup targets, and association evidence are redacted at the Tauri command boundary before being returned to the WebView. System identities such as `NT AUTHORITY`, `NT SERVICE`, and `BUILTIN` are preserved.
- The process view may still display executable paths containing a user-profile name. This is documented in [PRIVACY.md](PRIVACY.md).
- Process event history is stored locally in SQLite. The database is not encrypted and is not tamper-proof. A local administrator or SYSTEM can read or modify it.
- The application does not include telemetry, analytics, update checks, or an HTTP client. Signature verification is configured not to use the network and does not check revocation.

## Known residual risks

### No in-app elevation path

0.1.0 does not register `start_sensor`; no application code calls `ShellExecuteW` or `runas` for sensor startup, and there is no UAC launch path. The optional `SentinelLiteSensor.exe` must be started manually by an administrator outside the application. The main app cannot start, stop, configure, or elevate the sensor. The linked `windows` crate's read-only ShellLink support may leave an unused `ShellExecuteW` import symbol; it has no application call site.

This source-only release does not include the sensor, an installer, or any prebuilt binary. A locally built installer does not bundle the sensor, and locally built binaries are unsigned.

### Unsigned binaries

This release does not publish binaries or installers. Any binary built locally from this source is not code-signed; Windows SmartScreen may warn. A SHA-256 checksum helps detect corruption, but it does not prove the publisher's identity.

### Dependency advisories

`npm audit` currently reports 11 moderate findings in the development toolchain (`minimatch` via ESLint and TypeScript ESLint). Production dependencies (`npm audit --omit=dev`) report zero known vulnerabilities. The project does not automatically apply breaking `npm audit fix --force` upgrades. A RustSec advisory-db cross-check found three low-exposure denial-of-service advisories in transitive Tauri dependencies (`quick-xml 0.38.4`, `time 0.3.45`); details and the required upgrade/MSRV review are in [docs/THIRD_PARTY.md](docs/THIRD_PARTY.md).

### No complete audit

The absence of a known vulnerability does not mean the project is secure. The project has engineering CycloneDX SBOMs but no complete third-party audit, no formal verification, no fuzzing campaign, no reproducible-build proof, and no signed-update chain.

## Disclosure and response

Maintainers will evaluate reports, prepare a fix, and coordinate public disclosure. There is no bug bounty and no guaranteed response deadline. Please avoid publishing exploit details before a fix is available.

## Security claim boundaries

SentinelLite's risk labels are derived from limited evidence. "No issue found in the inspected scope" applies only to that scope; it does not mean the object is safe. Access denied, missing fields, truncated results, unavailable providers, and disconnected sensors are risk boundaries, not normal states.

The overall protection state is **unknown** in 0.1.0. The UI must not present a green "protected" result, a 0-100 score, or a malware verdict.
