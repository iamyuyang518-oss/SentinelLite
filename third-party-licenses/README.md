# Third-party license bundle

This directory is an engineering-produced license/copyright bundle for the SentinelLite
0.1.0 **source-only** release. It is not a legal opinion. The public tree does not
distribute an application, installer, sensor, native `.node`, or other executable
build output.

- Source dependency packages: **635** (Cargo: 403, npm: 232).
- Embedded native component (historical binary record): **1**
  (`packages/native/webview2-loader@1.0.3800.47/`); no WebView2Loader binary is included
  in this source-only release.
- Unique license/NOTICE texts in `texts/`: **361**.
- Packages with no standalone license file: **39**; see `MISSING_LICENSE_FILES.md`.
- Packages with a separate NOTICE file in Cargo/npm roots: **0**; see `NOTICE_SCAN.md`.
- The historical WebView2Loader record has a separate Microsoft NOTICE; see
  `EMBEDDED_NATIVE_COMPONENTS.md`. It is retained for attribution/provenance only.

Layout:

- `MANIFEST.csv`: one row per Cargo/npm package/version with license, scope, direct/transitive depth, source URL, license/NOTICE files and copyright fields.
- `packages/<ecosystem>/<name>@<version>/`: original upstream license/NOTICE/copyright files and supplemental exact-upstream evidence.
- `packages/native/webview2-loader@1.0.3800.47/`: Microsoft WebView2Loader license, NOTICE and provenance.
- `texts/`: deduplicated license/NOTICE text files, separated from per-package copyright files.
- `COPYRIGHT_INDEX.md`, `COPYRIGHT_REVIEW.md`: per-package copyright index and analysis of the 143 rows without extracted copyright lines.
- `MISSING_LICENSE_FILES.md`, `NOTICE_SCAN.md`: explicit negative findings; these are not invented notices.
- `EMBEDDED_NATIVE_COMPONENTS.md`: historical binary component record; the component
  itself is not included in this source-only release.
- `SHA256SUMS`: checksums for the bundle files.

Run `python -I scripts/generate-compliance.py --root . --output .` after `npm ci` and Cargo fetch to regenerate the base snapshot. The supplemental exact-upstream files in this directory are curated evidence added after that snapshot; preserve them when regenerating. The snapshot is tied to `Cargo.lock`/`package-lock.json`; it is not proof that every locked package is linked into an executable.

See `../SOURCE_ONLY_LICENSE_NOTICE.md`, `../THIRD_PARTY_NOTICES.md`,
`../third-party-source/mpl/`, and `../third-party-sbom/` for the source-only
special-license notice, MPL source materials, and SBOM records.
