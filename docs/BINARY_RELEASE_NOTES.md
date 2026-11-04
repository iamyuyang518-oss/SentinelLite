# Historical SentinelLite 0.1.0 binary build notes

Status: **historical record; not part of the source-only release.** No installer or
prebuilt application is distributed. See [RELEASE_NOTES.md](../RELEASE_NOTES.md) and
[RELEASE_RISKS.md](../RELEASE_RISKS.md).

The binary hashes below are retained only as build evidence. They are not download
links, release assets, or a promise that any binary is available.

## Historical artifacts

| Artifact | Size | SHA-256 | Version | Signature |
| --- | ---: | --- | --- | --- |
| `sentinel-lite.exe` | 8,480,256 bytes | `0fc65ff304d9e89a85c1e7c0b145d9210fa268dce44c06350df0267fd1b483be` | `0.1.0` | `NotSigned` |
| `SentinelLite_0.1.0_x64-setup.exe` | 2,535,872 bytes | `2cb65812c71980dd9d127f3209b03c79f14ebfe2a6915cdfb8f26596959439df` | `0.1.0` | `NotSigned` |

- Installer: NSIS, `currentUser`, `RequestExecutionLevel user`, default `%LOCALAPPDATA%\SentinelLite`.
- The historical installer and app did not contain `SentinelLiteSensor.exe`.
- The app has no in-app sensor start/stop or UAC launch path. The sensor remains a separate administrator-only component, built separately from source.
- The final build remaps the builder's Windows profile path out of panic metadata; the app contains `C:\Users\redacted` as a placeholder, not a real account.

## License and SBOM materials

- `THIRD_PARTY_NOTICES.md` — entry point and special-license obligations.
- `third-party-licenses/MANIFEST.csv` — 635 per-package rows (403 Cargo, 232 npm).
- `third-party-licenses/packages/` — original per-package license/copyright files plus supplemental exact-upstream evidence, including the native WebView2Loader.
- `third-party-licenses/texts/` — deduplicated license texts.
- `third-party-licenses/MISSING_LICENSE_FILES.md` — 39 packages without a standalone license file.
- `third-party-licenses/COPYRIGHT_REVIEW.md` — review of the 143 rows that initially lacked a specific copyright line.
- `third-party-licenses/NOTICE_SCAN.md` — NOTICE-file scan result.
- `third-party-licenses/EMBEDDED_NATIVE_COMPONENTS.md` — Microsoft WebView2Loader license/NOTICE and provenance.
- `third-party-source/mpl/` — exact-version MPL source archives, exact npm-source archive, SHA-256, and source-offer template.
- `third-party-sbom/SBOM.source.cdx.json` — source dependency inventory (642 components).
- `third-party-sbom/SBOM.binary.cdx.json` — historical binary composition upper bound (201 components, including the native loader); not part of the source-only release.
- `third-party-sbom/VALIDATION.md` — official CycloneDX 1.5 schema validation result.
- `docs/THIRD_PARTY_BINARY_COMPOSITION.md` — method and limits.
- `docs/INSTALLER_LICENSE_CHECKLIST.md` — installer-specific checklist.

## Known limitations

- The binaries are unsigned; SmartScreen may warn.
- The license bundle is engineering-prepared, not legally reviewed.
- 39 packages have no standalone license file in the installed source tree; expressions are recorded, but upstream review remains. Only `selectors` is in the runtime binary graph; `alloc-stdlib`, `webview2-com`, and `webview2-com-sys` now have exact upstream license files.
- `selectors@0.38.0` has no separate license file in its crate or upstream tree; MPL text and the exact upstream README statement are included.
- `webview2-com-sys` statically linked Microsoft WebView2Loader 1.0.3800.47 in the historical binary; its Microsoft license/NOTICE is included separately from the crate's MIT license. No loader binary is included in the source-only release.
- `lightningcss` and its native binding were build-time only in the historical build; the exact npm-published source archive and tag snapshot are included. The tag is one toolchain-only commit after the npm source; the root Cargo version `1.0.0-alpha.72` is the internal Rust crate version, not the npm wrapper version.
- The native `lightningcss-win32-x64-msvc` `.node` is not included in the source-only release; the metadata directory documents why no binary is present.
- Both SBOMs pass official CycloneDX 1.5 schema validation with 0 errors, but that is not a legal or conformance certification. The binary SBOM is historical only.
- The sensor is not bundled and cannot be started or stopped by the app.
- No code-signing, auto-update, reproducible-build proof, or clean-VM install test is included.

## Release gate

This historical binary record is not a release gate for the source-only release. Do not upload any app, installer, sensor, native `.node`, or other executable asset. See `RELEASE_RISKS.md` for the source-only position.
