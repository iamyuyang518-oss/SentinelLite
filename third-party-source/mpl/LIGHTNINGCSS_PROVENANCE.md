# lightningcss 1.33.0 source correspondence

This note resolves the apparent version mismatch between the npm package and the
upstream Rust workspace. It is an engineering provenance record, not a legal opinion.

The SentinelLite 0.1.0 public tree is source-only. It includes these source archives but
does **not** include the prebuilt `lightningcss-win32-x64-msvc` `.node` file. MPL-2.0
Section 3.2 executable-form mechanics are therefore not triggered by this release.

## Release identifiers

| Item | Value |
| --- | --- |
| npm package | `lightningcss@1.33.0` |
| npm registry tarball | `https://registry.npmjs.org/lightningcss/-/lightningcss-1.33.0.tgz` |
| npm registry integrity | `sha512-WkUDrojuJs0xkgGf2udWxa3yGBRxPtxUkB79i6aCZLRgc7PM8fZe9TosfPDcvEpQZbuFASnHYmRLBLUbmLOIIA==` |
| npm `gitHead` | `1d680fa14e9a089c92dc0929f869d6757ae91c30` |
| GitHub tag `v1.33.0` | `c6a0c3cebf3395635e61075d2c81a96a710d4910` |
| Tag commit parent | `1d680fa14e9a089c92dc0929f869d6757ae91c30` |
| Tag-to-npm difference | one commit, `bump rust toolchain`; only `rust-toolchain.toml` changed (`1.92.0` to `1.96.0`) |

## Manifest evidence

At both commits `1d680fa` and `c6a0c3c`, the upstream files are byte-identical:

| File | SHA-256 | Observed version |
| --- | --- | --- |
| `package.json` | `f964aa23ab45a046d05b56124eb052fe380b40110c04674efb4acdb0ad8a6279` | `"version": "1.33.0"` |
| root `Cargo.toml` | `bafcab91216086bb6147d5be2930b803ee5795a9a3045af1fb3516e72193bd2b` | `version = "1.0.0-alpha.72"` |

The `package.json` version is the npm wrapper release version. The root `Cargo.toml`
version is the internal Rust crate/workspace version and is not synchronized to the npm
version. The `1.0.0-alpha.72` value is therefore **not** evidence that the npm package
is from a different source release.

## Included source archives

| Archive | SHA-256 | Meaning |
| --- | --- | --- |
| `lightningcss-1.33.0-npm-head-1d680fa-source.tar.gz` | `ab377f42f5aa2f7f85058408ab45dbd03caa65fe6c00f4fdc32b9611c4e51bb4` | Exact GitHub source at the npm `gitHead` commit `1d680fa` |
| `lightningcss-1.33.0-upstream-source.tar.gz` | `3197aa5188b66d0d1e5026cbc8b65c4c0c53fdc949f9ed497fe75df2d13f722a` | GitHub tag `v1.33.0` snapshot at `c6a0c3c`; one toolchain-only commit after the npm source |
| `lightningcss-1.33.0-npm-package.tgz` | `d885d4655acd58b800a527c5b235081cbc09baf568ffff64ab17f579823ecc95` | Snapshot of the installed npm package (wrapper, license, metadata) |

The exact npm-published source archive is the primary source reference. The tag archive
is retained as the vendor's release-tag snapshot; it differs only in
`rust-toolchain.toml`.

## What this does and does not establish

- It establishes the npm package's `gitHead`, the tag commit, and their one-commit
  relationship from npm registry metadata and GitHub API data.
- It establishes that the root Cargo version and npm version are separate version
  lines, not a contradictory source identity.
- It does **not** establish that the native `lightningcss-win32-x64-msvc` npm binary is
  byte-reproducible from this source archive. That binary is not included in this
  source-only release.
- MPL-2.0 Section 3.2 applicability to a build-time-only package remains a question for
  a future binary release; it is not triggered by this source-only repository. See
  `MPL_SOURCE_OFFER.md` and `../../SOURCE_ONLY_LICENSE_NOTICE.md`.
