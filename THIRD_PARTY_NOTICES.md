# Third-party notices

This is an engineering-prepared third-party notice and source bundle for the
SentinelLite 0.1.0 **source-only** release. It is **not** a legal opinion, a legal
compliance certification, or a complete proof that every distribution obligation is
satisfied. The bundle intentionally contains upstream copyright notices, author names,
and project contact addresses from dependency license files; those are third-party
attribution data, not the local build user's account or machine data.

This repository does not distribute an application, installer, sensor, native `.node`,
or other executable form. The MPL-2.0 source notice for this source-only release is in
[`SOURCE_ONLY_LICENSE_NOTICE.md`](SOURCE_ONLY_LICENSE_NOTICE.md).

## Materials

| Material | Path | Contents |
| --- | --- | --- |
| Per-package manifest | [`third-party-licenses/MANIFEST.csv`](third-party-licenses/MANIFEST.csv) | 635 rows: 403 Cargo + 232 npm; version, license expression, scope, direct/transitive depth, source URL, license/NOTICE files, copyright fields |
| Original package files | [`third-party-licenses/packages/`](third-party-licenses/packages/) | Per-package `LICENSE*`, `COPYING*`, `NOTICE*`, `COPYRIGHT*` files plus exact-upstream supplemental evidence |
| Deduplicated texts | [`third-party-licenses/texts/`](third-party-licenses/texts/) | 361 unique license/NOTICE text files, separated from per-package copyright files |
| Copyright index | [`third-party-licenses/COPYRIGHT_INDEX.md`](third-party-licenses/COPYRIGHT_INDEX.md) | Human-readable package/copyright index; missing lines are recorded, not invented |
| Copyright review | [`third-party-licenses/COPYRIGHT_REVIEW.md`](third-party-licenses/COPYRIGHT_REVIEW.md) | Analysis of the 143 rows that initially lacked a specific copyright line |
| Missing-file report | [`third-party-licenses/MISSING_LICENSE_FILES.md`](third-party-licenses/MISSING_LICENSE_FILES.md) | 39 packages without a standalone license file; only `selectors` is in the runtime binary graph |
| NOTICE scan | [`third-party-licenses/NOTICE_SCAN.md`](third-party-licenses/NOTICE_SCAN.md) | 0 separate NOTICE files in the 635 Cargo/npm roots; 1 embedded native NOTICE |
| Embedded native components (historical) | [`third-party-licenses/EMBEDDED_NATIVE_COMPONENTS.md`](third-party-licenses/EMBEDDED_NATIVE_COMPONENTS.md) | Microsoft WebView2Loader 1.0.3800.47 license/NOTICE and provenance; the binary itself is not in this source-only release |
| MPL source | [`third-party-source/mpl/`](third-party-source/mpl/) | Exact-version Cargo `.crate` archives, npm package snapshot, exact npm-source archive, tag snapshot, hashes, source-offer template |
| Source-only license notice | [`SOURCE_ONLY_LICENSE_NOTICE.md`](SOURCE_ONLY_LICENSE_NOTICE.md) | MPL Section 3.1 notice and source-only distribution scope |
| Lightningcss provenance | [`third-party-source/mpl/LIGHTNINGCSS_PROVENANCE.md`](third-party-source/mpl/LIGHTNINGCSS_PROVENANCE.md) | npm `gitHead`, tag commit, one-commit difference, manifest hashes |
| MPL Section 3.2 assessment | [`third-party-source/mpl/MPL_SECTION_3_2_ASSESSMENT.md`](third-party-source/mpl/MPL_SECTION_3_2_ASSESSMENT.md) | Per-component distribution/source analysis and open legal questions |
| Source SBOM | [`third-party-sbom/SBOM.source.cdx.json`](third-party-sbom/SBOM.source.cdx.json) | CycloneDX 1.5; 642 components (403 Cargo + 232 npm + 7 first-party) |
| Binary SBOM (historical) | [`third-party-sbom/SBOM.binary.cdx.json`](third-party-sbom/SBOM.binary.cdx.json) | CycloneDX 1.5; 201 components; retained only as a historical binary build record, not a source-only release asset list |
| SBOM validation | [`third-party-sbom/VALIDATION.md`](third-party-sbom/VALIDATION.md) | Official CycloneDX 1.5 schema validation: 0 errors for both files |
| Binary composition (historical) | [`docs/THIRD_PARTY_BINARY_COMPOSITION.md`](docs/THIRD_PARTY_BINARY_COMPOSITION.md) | Historical crate/package-level composition method; not part of this release |
| Installer checklist (historical) | [`docs/INSTALLER_LICENSE_CHECKLIST.md`](docs/INSTALLER_LICENSE_CHECKLIST.md) | Historical binary/installer verification status; no installer is provided |
| Binary release notes (historical) | [`docs/BINARY_RELEASE_NOTES.md`](docs/BINARY_RELEASE_NOTES.md) | Historical binary build records; not downloadable assets |
| Release notes | [`RELEASE_NOTES.md`](RELEASE_NOTES.md) | Source-only scope, limitations, verification, and source checksums |
| Release risk record | [`RELEASE_RISKS.md`](RELEASE_RISKS.md) | Source-only gate result, residual license/source items, shortest resolution path |
| Checksums | `third-party-licenses/SHA256SUMS`, `third-party-source/mpl/SHA256SUMS`, `third-party-sbom/SHA256SUMS` | Integrity manifests for the license, MPL source, and SBOM bundles |

The project itself is MIT-licensed; see [LICENSE](LICENSE).

## Scope of the manifest

`MANIFEST.csv` covers the locked source dependency graph:

- 403 external Cargo packages from the full `Cargo.lock` graph;
- 232 npm packages from the installed `package-lock.json`/`npm query` graph;
- direct and transitive depth where resolvable;
- distribution scope (`runtime`, `runtime-graph-proc-macro`, `build`, `dev`,
  `locked-not-selected`, npm `dev`/`optional`);
- whether the package is in the Windows-normal Cargo graph or npm production set.

It does **not** claim that every locked package is linked into an executable.
`SBOM.binary.cdx.json` and `docs/THIRD_PARTY_BINARY_COMPOSITION.md` are retained only
as historical binary build records. They are not part of the source-only release and do
not describe downloadable files.

The Microsoft WebView2Loader is **not** a separate Cargo/npm row. It was an embedded
native component in the earlier binary build; it is recorded as historical provenance in
`EMBEDDED_NATIVE_COMPONENTS.md` and the binary SBOM. No WebView2Loader binary is
included in this source-only release.

## Special licenses

### MPL-2.0

MPL-2.0 is file-level copyleft. The full text is at
[`third-party-licenses/MPL-2.0.txt`](third-party-licenses/MPL-2.0.txt).

| Package | Version | Role |
| --- | --- | --- |
| `cssparser` | 0.37.0 | Runtime Cargo crate via `dom_query`/`tauri-utils` |
| `dtoa-short` | 0.3.5 | Runtime Cargo crate |
| `option-ext` | 0.2.0 | Runtime Cargo crate |
| `selectors` | 0.38.0 | Runtime Cargo crate via `dom_query` |
| `cssparser-macros` | 0.7.1 | Proc-macro at build time; not linked into the executable |
| `lightningcss` | 1.33.0 | npm build-time CSS tool; not in the Rust executable |
| `lightningcss-win32-x64-msvc` | 1.33.0 | npm build-time native binding; not in the Rust executable |

Exact-version source archives and hashes are in
[`third-party-source/mpl/`](third-party-source/mpl/). The source-only release
distributes Source Code Form and does not distribute an Executable Form, so MPL-2.0
Section 3.2 is not triggered by this repository; the Section 3.1 notice and exact
archives are in [`SOURCE_ONLY_LICENSE_NOTICE.md`](SOURCE_ONLY_LICENSE_NOTICE.md) and
`third-party-source/mpl/`.

`selectors@0.38.0` does not ship a standalone license file in its published crate; all
16 Rust source files in the exact archive carry the standard MPL-2.0 header, its Cargo
metadata declares MPL-2.0, the upstream README at the exact commit states "Stylo is
licensed under MPL 2.0", and the MPL text is included. The source-offer template is a
template only and is not executed.

For `lightningcss`, the npm registry `gitHead` is `1d680fa...`, while GitHub tag
`v1.33.0` points to `c6a0c3c...`, one commit later; the only change is
`rust-toolchain.toml`. The exact npm-published source archive is included. The root
Cargo version `1.0.0-alpha.72` is the internal Rust crate version, not the npm wrapper
version. See `LIGHTNINGCSS_PROVENANCE.md`.

### Microsoft WebView2Loader

In the earlier binary build, `webview2-com-sys@0.39.1` statically linked Microsoft
`WebView2Loader` 1.0.3800.47 for MSVC targets. The Microsoft `LICENSE.txt`
(BSD-3-Clause-style terms) and `NOTICE.txt` are retained at
[`third-party-licenses/packages/native/webview2-loader@1.0.3800.47/`](third-party-licenses/packages/native/webview2-loader@1.0.3800.47/).
No WebView2Loader binary is included in this source-only release. The crate's MIT
license covers the Rust bindings, not this Microsoft component.

### CC-BY-4.0

`caniuse-lite@1.0.30001819` is used by the frontend build toolchain. The full text is at
[`third-party-licenses/CC-BY-4.0.txt`](third-party-licenses/CC-BY-4.0.txt).

Attribution notice to retain with distributions of the data:

> Can I use / caniuse-lite data, https://caniuse.com/, licensed under CC-BY-4.0,
> https://creativecommons.org/licenses/by/4.0/.

### Unicode-3.0

The Unicode License V3 text is at
[`third-party-licenses/Unicode-3.0.txt`](third-party-licenses/Unicode-3.0.txt). The
manifest includes the ICU4X-related crates and `unicode-ident`; `unicode-ident` also
offers MIT/Apache-2.0, but its license expression includes Unicode-3.0.

### Zlib

The Zlib notices are at [`third-party-licenses/Zlib.txt`](third-party-licenses/Zlib.txt)
and [`third-party-licenses/Zlib-bytemuck.txt`](third-party-licenses/Zlib-bytemuck.txt).
The manifest records `bytemuck`, `dispatch2`, `foldhash`, `miniz_oxide`, `objc2-*`,
`raw-window-handle`, `static_vcruntime`, and `zlib-rs`. Individual packages may carry
different copyright lines; the original package files are retained.

### BlueOak-1.0.0

`lru-cache@11.5.3` and `minimatch@10.2.6` are toolchain packages under Blue Oak Model
License 1.0.0. The text is at
[`third-party-licenses/BlueOak-1.0.0.txt`](third-party-licenses/BlueOak-1.0.0.txt). The
license requires recipients to receive the text or a link to
<https://blueoakcouncil.org/license/1.0.0>.

### ISC

`lru-cache@5.1.1` is ISC-licensed; the notice is at
[`third-party-licenses/ISC-lru-cache-5.1.1.txt`](third-party-licenses/ISC-lru-cache-5.1.1.txt).

### LGPL option

`r-efi@5.3.0` and `r-efi@6.0.0` are offered under
`MIT OR Apache-2.0 OR LGPL-2.1-or-later`. This project selects the MIT/Apache-2.0
option and does not intentionally rely on the LGPL branch. Re-check if the dependency
graph changes.

### Other licenses

MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, 0BSD, MIT-0, Unlicense, and dual/triple
license expressions are recorded per package in `MANIFEST.csv` and represented by the
original files under `packages/`. No GPL, AGPL, SSPL, Commons Clause, Business Source,
or Elastic License expression was detected by the inventory script.

## Explicit partial results

- **39 packages** have no standalone license file in the installed source tree. Their
  license expression is recorded in `MANIFEST.csv`; upstream review remains necessary.
  Only `selectors@0.38.0` was in the earlier runtime binary graph. `alloc-stdlib@0.3.0`,
  `webview2-com@0.39.1`, and `webview2-com-sys@0.39.1` have exact upstream license
  files; `selectors` has the MPL headers in its exact source archive, the upstream
  README statement, and the MPL text.
- **134 packages** still have no specific copyright line extracted after supplemental
  review. Of these, 36 have no standalone license file and 98 have license texts with
  only generic boilerplate and no named holder. The original license files are still
  copied; no copyright line was invented. See `COPYRIGHT_REVIEW.md`.
- **0 separate NOTICE files** were found in the 635 Cargo/npm package roots. The
  embedded WebView2Loader has a separate Microsoft `NOTICE.txt`. This is a
  snapshot-specific scan, not proof that upstream has no NOTICE.
- The `lightningcss` npm-to-source correspondence is resolved: npm `gitHead`
  `1d680fa...`, tag `v1.33.0` at `c6a0c3c...`, one toolchain-only commit apart, with
  byte-identical `package.json` and root `Cargo.toml`. The root Cargo version
  `1.0.0-alpha.72` is an independent internal Rust version. The native binding is not
  distributed in this source-only release; its metadata directory contains no `.node`.
- Both SBOMs were validated against the official CycloneDX 1.5 schema with 0 errors.
  The source SBOM describes the locked source dependency graph. The binary SBOM is a
  historical build record, not a source-only release asset list. This is schema
  validation, not a CycloneDX conformance certification.
- No legal review or legal opinion has been obtained.

## Source-only release checklist

Before publishing this repository:

1. Confirm that the public tree contains no installer, application binary, sensor,
   native `.node`, or other executable build output.
2. Keep `SOURCE_ONLY_LICENSE_NOTICE.md`, `THIRD_PARTY_NOTICES.md`,
   `third-party-licenses/`, `third-party-source/mpl/`, and the source SBOM in the public
   tree.
3. Record the residual `selectors` standalone-license gap and the 39/134 inventory
   findings in `RELEASE_RISKS.md`; do not claim they are legally resolved.
4. Do not include the historical binary SBOM as if it described downloadable assets.
5. Do not describe the result as legally compliant solely because the automated
   generator or schema validator passed.

A future binary release is a separate distribution event. It must revisit MPL Section
3.2, the native binding source position, and the written-offer template before any
executable is uploaded.

This file is an engineering artifact. It is not legal advice and does not establish
compliance by itself.
