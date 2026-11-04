# SBOM files

Both files are CycloneDX 1.5 JSON. They were generated from the locked dependency
graph and installed package metadata, not from a legal review.

This SentinelLite 0.1.0 public tree is a **source-only** release. The source SBOM is the
current release inventory. The binary SBOM is retained only as a historical record of
an earlier binary build and is **not** a list of downloadable assets for this release.

| File | Scope | Components | What it means |
| --- | --- | ---: | --- |
| `SBOM.source.cdx.json` | Source dependency inventory | 642 (403 Cargo external + 232 npm + 7 first-party workspace crates) | Everything in `Cargo.lock` and `package-lock.json`, including build/dev/target-locked packages. This is a dependency inventory, not a file-by-file list of every archive in this repository. |
| `SBOM.binary.cdx.json` | Historical binary composition upper bound | 201 (196 Cargo runtime crates + 4 npm production candidates + 1 embedded native component) | A crate-level record of an earlier Windows `x86_64-pc-windows-msvc` build. It is not part of the source-only release, not an asset list, and not a symbol-level linker proof. |

The source SBOM includes locked packages that are not distributed as source in this
repository. That is intentional: it records the dependency resolution graph. The
repository itself contains only selected exact-version MPL source archives, not every
locked package's source.

The binary SBOM's historical artifact hashes are retained only for traceability:

- app: `0fc65ff304d9e89a85c1e7c0b145d9210fa268dce44c06350df0267fd1b483be`
- installer: `2cb65812c71980dd9d127f3209b03c79f14ebfe2a6915cdfb8f26596959439df`

No app or installer is included in or published by this source-only release.

The historical binary SBOM represents the embedded WebView2Loader as
`pkg:generic/webview2-loader@1.0.3800.47`; `webview2-com-sys@0.39.1` has a dependency
edge to it. Its Microsoft license/NOTICE materials are in
`../third-party-licenses/packages/native/webview2-loader@1.0.3800.47/`. No
WebView2Loader binary is included in this source-only release.

## Generation

```powershell
npm ci
python -I scripts/generate-compliance.py --root . --output .
```

The generator uses `cargo metadata --locked`, `cargo tree` for the Windows target,
`npm query`, `Cargo.lock`, and `package-lock.json`. It does not upgrade dependencies or
modify runtime code. The native WebView2Loader component is added as curated evidence
after generation; see `VALIDATION.md`. For a source-only release, do not publish newly
generated binary SBOMs as release assets without repeating the binary-distribution
license review.

## Validation performed

- JSON parsed successfully.
- `bomFormat` is `CycloneDX` and `specVersion` is `1.5`.
- Every `dependsOn` reference resolves to a component or the root application reference.
- Direct/transitive relationships are present for Cargo and npm.
- Both files were validated on 2026-11-02 against the official CycloneDX 1.5 JSON
  schema with `jsonschema` 4.26.0 / JSON Schema draft-07: **0 validation errors**.
- `SHA256SUMS` records the checksums of the two SBOM files, this README, and `VALIDATION.md`.

## Limitations

- Cargo crate-level graph is an upper bound; a linker may drop unused code.
- npm production packages are candidates; Vite output may tree-shake or inline parts.
- Target-specific and build-only packages are intentionally excluded from the binary
  SBOM but remain in the source SBOM.
- The source SBOM does not establish license compliance or legal sufficiency. See
  `THIRD_PARTY_NOTICES.md`, `third-party-licenses/MANIFEST.csv`,
  `SOURCE_ONLY_LICENSE_NOTICE.md`, and `third-party-source/mpl/`.
- The binary SBOM is a historical build record, not a source-only release asset list.
