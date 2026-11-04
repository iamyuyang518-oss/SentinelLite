# Historical installer license verification checklist

Status: **historical record; not part of the SentinelLite 0.1.0 source-only release.**
No installer is distributed. This is an engineering checklist, not a legal approval.

The binary/installer checks below describe an earlier build and are retained for
traceability only. For the current release, see `RELEASE_NOTES.md`,
`SOURCE_ONLY_LICENSE_NOTICE.md`, and `RELEASE_RISKS.md`.

## Artifact identity

- [x] App: `sentinel-lite.exe`, 8,480,256 bytes, SHA-256 `0fc65ff304d9e89a85c1e7c0b145d9210fa268dce44c06350df0267fd1b483be`, FileVersion/ProductVersion `0.1.0`, `NotSigned`.
- [x] Installer: `SentinelLite_0.1.0_x64-setup.exe`, 2,535,872 bytes, SHA-256 `2cb65812c71980dd9d127f3209b03c79f14ebfe2a6915cdfb8f26596959439df`, FileVersion/ProductVersion `0.1.0`, `NotSigned`.
- [x] `SHA256SUMS.txt` verifies both assets.
- [x] Installer NSIS script packages only the main binary; the WebView2 bootstrapper define is empty in this generated build.
- [x] Installer does not contain `SentinelLiteSensor.exe`.
- [x] No app-internal `start_sensor`/`runas` launch path; `ShellExecuteW` is an unused import symbol with no application call site.
- [x] Final app/installer contain no build user's real profile path; build remaps it to `C:\Users\redacted`.

## License materials

- [x] Per-package manifest: `third-party-licenses/MANIFEST.csv`, 635 rows (403 Cargo, 232 npm).
- [x] Installed package license/copyright files plus supplemental exact-upstream evidence: `third-party-licenses/packages/` (895 files: 876 original + 19 supplemental, including the native component).
- [x] Deduplicated license/NOTICE texts: `third-party-licenses/texts/` (361 files).
- [x] Human-readable copyright index: `third-party-licenses/COPYRIGHT_INDEX.md`.
- [x] Copyright extraction review: `third-party-licenses/COPYRIGHT_REVIEW.md` (143 original gaps analyzed; 134 remain without a specific line, 36 of which lack a standalone license file).
- [x] Missing standalone license files explicitly recorded: `third-party-licenses/MISSING_LICENSE_FILES.md` (39 packages).
- [x] NOTICE scan recorded: `third-party-licenses/NOTICE_SCAN.md` (0 separate NOTICE files in the 635 Cargo/npm roots; 1 embedded native NOTICE).
- [x] Embedded native component license/NOTICE: `third-party-licenses/EMBEDDED_NATIVE_COMPONENTS.md` and `third-party-licenses/packages/native/webview2-loader@1.0.3800.47/`.
- [x] Special-license texts exist and are readable: MPL-2.0, CC-BY-4.0, Unicode-3.0, Zlib, BlueOak-1.0.0, ISC.
- [x] MPL source archives exist for the locked versions: `third-party-source/mpl/`.
- [x] MPL archive hashes are recorded: `third-party-source/mpl/SHA256SUMS`.
- [x] `lightningcss` npm `gitHead` and tag relationship documented: `third-party-source/mpl/LIGHTNINGCSS_PROVENANCE.md`.
- [x] MPL source-offer template exists but is **not executed**: `third-party-source/mpl/MPL_SOURCE_OFFER.md`.
- [x] Source and binary SBOMs exist: `third-party-sbom/SBOM.source.cdx.json`, `third-party-sbom/SBOM.binary.cdx.json`.
- [x] SBOM JSON parses and dependency references are closed.
- [x] Both SBOMs pass official CycloneDX 1.5 schema validation with 0 errors: `third-party-sbom/VALIDATION.md`.
- [x] `THIRD_PARTY_NOTICES.md` is linked from the README and `docs/THIRD_PARTY.md`.

## Partial or pending

- [ ] **39 packages have no standalone license file** in the installed package root. Their package metadata license expression is recorded in `MANIFEST.csv`; upstream source review is still required. Only `selectors@0.38.0` is in the runtime binary graph. `alloc-stdlib@0.3.0`, `webview2-com@0.39.1`, and `webview2-com-sys@0.39.1` now have exact upstream license files.
- [ ] `selectors@0.38.0` has no separate license file in its published crate or upstream tree at the exact commit; MPL-2.0 text and the upstream README statement are included. Upstream packaging still lacks a standalone license/notice.
- [ ] `lightningcss@1.33.0` now has the exact npm `gitHead` source archive and the tag snapshot; the tag is one toolchain-only commit after the npm source. The root Cargo version `1.0.0-alpha.72` is the internal Rust crate version, not the npm wrapper version. `lightningcss-win32-x64-msvc` ships a native binary and no source; its reproducibility from the included source is not proven.
- [ ] Full per-package legal review has not been performed.
- [ ] MPL Section 3.2 applicability to build-time-only packages and the sufficiency of the source archives/native binding need legal review.
- [ ] The final installer has not been installed on a clean VM as part of this license pass.
- [ ] Binaries are unsigned; no code-signing or update-chain review.

## Release decision

This checklist is historical. The current release is source-only and does not provide
the installer or portable executable. Do not describe any local binary build as
"license-compliant" or "legally reviewed." A future binary release must repeat the
binary distribution gate and MPL Section 3.2 review before any executable is shared.
