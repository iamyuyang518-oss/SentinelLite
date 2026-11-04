# Privacy

SentinelLite is local-first. The project does not operate a cloud service, does not include telemetry or analytics, and does not upload scan results, process events, software lists, or file metadata.

This page describes what the 0.1.0 preview reads, where it stores data, and how display-boundary redaction works. It is a factual description of the current code, not a claim of complete anonymity.

## Summary

- No account, sign-in, advertising, telemetry, crash reporting, or update check is built in.
- Scans run only when the user triggers them. The main application does not continuously scan in the background.
- The optional sensor runs only when an administrator starts it manually outside the app. 0.1.0 does not provide an in-app start or UAC action. It reads Kernel-Process ETW start/stop events.
- Process history is stored in a local SQLite database under the current user's profile.
- The application does not send data over the network. Signature verification is configured not to use the network and does not check certificate revocation.
- The main application runs as a standard user. Only the separate sensor runs elevated.

## Data read by the main application

When the user starts a scan, the main application may read:

- Traditional Uninstall registry entries: display name, version, publisher, install date, install location, DisplayIcon path, WindowsInstaller flag, uninstall-entry presence, and classification fields. Full uninstall command text is not returned, stored, or executed.
- Run/RunOnce values and startup folders: value names, value types, target paths, and whether `.lnk` arguments/working directories are present. Argument and working-directory text is not retained.
- Services: name, display name, status, start type, service type, account, and executable path. Service parameters are discarded.
- Scheduled tasks: task path/name, status, enabled flag, author, run-as identity, action type, and executable path. Task arguments, working directory, triggers, schedules, and last-run results are not read.
- Processes: PID, parent PID, process name, readable executable path, creation-time identity token, and, for a selected process, affinity mask, signature state, certificate subject, file size, modification time, and a bounded SHA-256 hash.
- Native protection: Windows Defender antivirus/realtime/antimalware-service switches, signature version/age, and the enabled state of the Domain/Private/Public firewall profiles.
- Asset associations: static path comparisons derived from the software, process, service, task, and startup snapshots above.
- File and registry existence/access metadata: present, missing, access denied, or unknown.

The application does not read keyboard input, screen content, browser passwords, browsing history, document contents, network payloads, or full command lines.

## Data read by the optional sensor

The separate administrator sensor reads Kernel-Process ETW start/stop events:

- PID and parent PID.
- Process creation time.
- Image name.
- ETW event ID/version/sequence and timestamps.
- Sensor receive/normalization times.
- Derived pairing state, health counters, queue/delivery statistics, and bounded diagnostic samples.

The sensor does not read full command lines, file contents, keyboard input, screen content, browser data, or network content.

The sensor keeps normalized events in a bounded in-memory delivery window. The main application can read event batches through a restricted local named pipe and can write them to the local SQLite database. The sensor writes a JSON summary only when the user explicitly passes `--output PATH`; that file can contain event counts, image base names, recent event samples, and diagnostic errors. The user controls the path. The application does not automatically delete or upload that file.

## Local storage

Process event history is stored at:

```text
%LOCALAPPDATA%\app.sentinellite.desktop\SentinelLite\events.db
```

The database stores process-event sessions, normalized events (image path, PID/parent PID, creation time, ETW/receive/normalization timestamps, pairing state), undecodable-event records, pairing revisions, delivery gaps, and `SessionEnd` records. It does not store command lines or file contents.

The database is protected by the current user's Windows ACL. It is **not encrypted**, has no integrity signature, and is not a tamper-proof audit chain. A local administrator, SYSTEM, or the current user may be able to read or modify it. The default retention policy is 30 days, 1,000,000 events, and 512 MiB.

Other scan results (software, startup, services, tasks, processes, associations, posture) are kept in memory and are lost when the window closes.

The project does not currently provide an export, delete, or secure-erasure UI. To delete the local history, close the application and remove the database file and its `-wal`/`-shm` companions through normal Windows file management.

## Network behavior

- The application has no telemetry, analytics, advertising, cloud API, account system, or update check.
- No HTTP client plugin is exposed to the WebView. The frontend CSP allows only local IPC and packaged assets.
- Signature verification is configured to avoid network access and does not check certificate revocation.
- Startup-folder, install-location, and signature probes skip UNC/device paths and relative paths. Windows itself may resolve reparse points on local paths; the application does not claim control over operating-system-level behavior.
- WebView2, Windows, and other system components may have their own network behavior. The project does not claim to control all operating-system communication.

## Redaction

The Rust scanners keep raw paths in memory for matching and local checks. Before returning inventory data to the WebView, the Tauri command layer applies presentation-boundary redaction:

- A user-profile path segment such as `C:\Users\<name>\...` is shown as `C:\Users\***\...`. Device-prefixed forms such as `\\?\C:\Users\<name>\...` are handled as well.
- Personal service accounts and scheduled-task run-as identities are shown as `DOMAIN\***`. Well-known system and group identities such as `NT AUTHORITY\SYSTEM`, `NT SERVICE\TrustedInstaller`, `BUILTIN\Administrators`, and SIDs are preserved.
- Scheduled task names, paths, and IDs have the current user's account name replaced with `***`.
- Software install locations, DisplayIcon paths, startup targets, risk evidence text, and association evidence values are redacted before display.
- The frontend also applies a fallback redaction to displayed paths and account values.

The raw paths are still used inside the Rust process for association matching. Redaction is a display-boundary measure; it does not encrypt or delete the underlying local data.

**Process-view caveat:** the process snapshot displays executable paths as evidence and is not part of the inventory command-boundary redaction. A process path may therefore contain a user-profile name. This is a known limitation and is documented in [docs/KNOWN_LIMITATIONS.md](docs/KNOWN_LIMITATIONS.md).

## User control

- Every inventory scan, native-protection query, association request, event query, and analysis run is user-triggered.
- The sensor is not started automatically. 0.1.0 provides no in-app start or UAC action; an administrator must start the sensor manually outside the app.
- The application does not provide stop/restart controls for the sensor. Closing the main window does not stop the sensor. Use Task Manager to end `SentinelLiteSensor.exe` if needed.
- There are no remediation actions, so there is no "delete/uninstall by mistake" path in 0.1.0.
- The application does not have an automatic update mechanism.

## Third-party components

Dependency versions are locked by `package-lock.json` and `Cargo.lock`. License and inventory information is in [docs/THIRD_PARTY.md](docs/THIRD_PARTY.md) and [docs/THIRD_PARTY_INVENTORY.md](docs/THIRD_PARTY_INVENTORY.md). Any dependency that adds network, upload, or telemetry behavior must receive a separate privacy review.

## Changes

A change in actual data behavior must be committed together with this document and reflected in the release notes. Do not replace specific statements with absolute claims such as "completely anonymous" or "never uploads anything" unless the code and verification support them.
