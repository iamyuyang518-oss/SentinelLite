# Historical third-party binary composition

Status: **historical record; not part of the SentinelLite 0.1.0 source-only release.**
No application or installer binary is distributed.

This is a crate/package-level composition analysis for an earlier SentinelLite 0.1.0
binary build. It is **not** a symbol-level linker proof and not a legal opinion. The
current public release is source-only; see `RELEASE_NOTES.md`,
`SOURCE_ONLY_LICENSE_NOTICE.md`, and `RELEASE_RISKS.md`.

## Historical binary artifacts

| Artifact | Size | SHA-256 |
| --- | ---: | --- |
| `sentinel-lite.exe` | 8,480,256 bytes | `0fc65ff304d9e89a85c1e7c0b145d9210fa268dce44c06350df0267fd1b483be` |
| `SentinelLite_0.1.0_x64-setup.exe` | 2,535,872 bytes | `2cb65812c71980dd9d127f3209b03c79f14ebfe2a6915cdfb8f26596959439df` |

The historical installer was an NSIS `currentUser` package. In that generated build the WebView2 bootstrapper define was empty, so the NSIS script packaged only `sentinel-lite.exe`; if a future bundle embeds a WebView2 bootstrapper, that Microsoft component needs separate review. The historical installer did not contain `SentinelLiteSensor.exe`. No installer is distributed in this source-only release.

## Method

- Cargo: `cargo tree -p sentinel-lite --target x86_64-pc-windows-msvc -e normal`.
- The raw normal tree contains 280 lines including duplicate feature entries and `(proc-macro)` entries. After normalizing name/version and excluding proc-macro entries, the binary SBOM records 196 external runtime crates.
- npm: `npm query '*'`; packages with `dev != true` are recorded as production candidates. Four packages are present: `react`, `react-dom`, `scheduler`, and `@tauri-apps/api`.
- Source/lock inventory: 403 Cargo external packages and 232 npm packages, recorded in `third-party-licenses/MANIFEST.csv`.
- The generated `SBOM.binary.cdx.json` contains 201 components and 198 dependency entries; `SBOM.source.cdx.json` contains 642 components and 643 dependency entries. The binary count includes the embedded Microsoft WebView2Loader native component.
- Both SBOMs pass the official CycloneDX 1.5 JSON schema validation with 0 errors; see `third-party-sbom/VALIDATION.md`.

## MPL-2.0 composition

| Package | Version | Binary role | Source material |
| --- | --- | --- | --- |
| `cssparser` | 0.37.0 | Runtime crate via `dom_query`/`tauri-utils`; included in binary upper bound | `third-party-source/mpl/archives/cssparser-0.37.0.crate` |
| `dtoa-short` | 0.3.5 | Runtime crate; included in binary upper bound | `third-party-source/mpl/archives/dtoa-short-0.3.5.crate` |
| `option-ext` | 0.2.0 | Runtime crate; included in binary upper bound | `third-party-source/mpl/archives/option-ext-0.2.0.crate` |
| `selectors` | 0.38.0 | Runtime crate via `dom_query`; included in binary upper bound | `third-party-source/mpl/archives/selectors-0.38.0.crate`; upstream README evidence in the package directory |
| `cssparser-macros` | 0.7.1 | Proc-macro used during compilation; not linked into the executable | `third-party-source/mpl/archives/cssparser-macros-0.7.1.crate` |
| `lightningcss` | 1.33.0 | npm build-time CSS tool; not in the Rust executable | `lightningcss-1.33.0-npm-package.tgz`, exact npm-source archive `lightningcss-1.33.0-npm-head-1d680fa-source.tar.gz`, and tag snapshot |
| `lightningcss-win32-x64-msvc` | 1.33.0 | npm build-time native binding; not in the Rust executable | Metadata only; native package has no source |

`selectors@0.38.0` has no standalone license file in its published crate or upstream tree; its Cargo metadata declares `MPL-2.0`, the upstream README states "Stylo is licensed under MPL 2.0", and the MPL text is in `third-party-licenses/MPL-2.0.txt`. The exact upstream evidence is in `third-party-licenses/packages/cargo/selectors@0.38.0/`.

## Embedded Microsoft WebView2Loader

`webview2-com-sys@0.39.1` statically links Microsoft `WebView2Loader` 1.0.3800.47 for MSVC targets. The final executable has no separate `WebView2Loader.dll` import. The x64 loader files in the crate are byte-identical to the Microsoft.Web.WebView2 1.0.3800.47 NuGet package. The Microsoft license and NOTICE are in `third-party-licenses/packages/native/webview2-loader@1.0.3800.47/`; the binary SBOM includes `pkg:generic/webview2-loader@1.0.3800.47`.

## Build-time-only dependencies

The source SBOM includes 403 Cargo external packages and 232 npm packages. These include platform-specific, build, dev, optional, and target-locked packages that are not in the Windows runtime graph. They are intentionally retained in the source inventory so that a lockfile audit is not mistaken for a binary composition list.

Examples of packages that are not in the final Rust executable:

- npm dev tooling (`typescript`, `vite`, `vitest`, ESLint, Tauri CLI packages, `lightningcss`, etc.);
- Cargo proc-macro and build tooling (`cssparser-macros`, `tauri-macros`, `tauri-codegen`, `tauri-build`, etc.);
- target-specific packages for macOS/Android/Linux (for example `objc2-*`, `ndk*`, GTK crates) that are present in the full lockfile but not selected for `x86_64-pc-windows-msvc`.

## Limitations

- Rust linker dead-code elimination and monomorphization can remove code that the crate graph suggests is present.
- `cargo tree` is feature-unified and target-filtered but is not a symbol-level inventory.
- Vite may tree-shake, inline, or transform production npm code; the four npm packages are a candidate set, not a byte-level composition proof.
- The generated installer has an empty WebView2 bootstrapper define; if a future bundle embeds a Microsoft WebView2 bootstrapper, that component is not represented as a SentinelLite dependency and would need separate review.
- Do not use this document as a legal conclusion that a particular MPL file is or is not distributed.
