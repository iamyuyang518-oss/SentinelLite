# MPL-2.0 source materials

This directory is an engineering-prepared source package for the MPL-2.0 dependencies
identified in `third-party-licenses/MANIFEST.csv`. It supports the SentinelLite 0.1.0
**source-only** release. It is not a legal opinion.

This release distributes Source Code Form only and does not distribute an Executable
Form. MPL-2.0 Section 3.2 executable-form source-availability is therefore not triggered
by this repository. Section 3.1 notice is in `../../SOURCE_ONLY_LICENSE_NOTICE.md`; the
component-by-component analysis remains in `MPL_SECTION_3_2_ASSESSMENT.md` for a future
binary release.

The exact versions below come from the SentinelLite 0.1.0 `Cargo.lock`,
`package-lock.json`, and the isolated release build. The Cargo `.crate` files are the
published crates.io source archives; `SHA256SUMS` covers every archive.

## Packages

| Package | Version | Role in SentinelLite | Source archive | SHA-256 |
| --- | --- | --- | --- | --- |
| `cssparser` | 0.37.0 | Runtime crate, normal graph via `dom_query`/`tauri-utils` | `archives/cssparser-0.37.0.crate` | `8c9cdaae...` |
| `cssparser-macros` | 0.7.1 | Proc-macro used at build time; not linked into the executable | `archives/cssparser-macros-0.7.1.crate` | `d045de69...` |
| `dtoa-short` | 0.3.5 | Runtime crate, normal graph | `archives/dtoa-short-0.3.5.crate` | `cd1511a7...` |
| `option-ext` | 0.2.0 | Runtime crate, normal graph | `archives/option-ext-0.2.0.crate` | `04744f49...` |
| `selectors` | 0.38.0 | Runtime crate, normal graph via `dom_query` | `archives/selectors-0.38.0.crate` | `8adfa1c2...` |
| `lightningcss` | 1.33.0 | npm build-time CSS tool; not in the Rust executable | `lightningcss-1.33.0-npm-package.tgz` | `d885d465...` |
| `lightningcss` npm source | 1.33.0 | Exact source at npm `gitHead` `1d680fa...`; build-time only | `lightningcss-1.33.0-npm-head-1d680fa-source.tar.gz` | `ab377f42...` |
| `lightningcss` tag source | 1.33.0 | Upstream tag `v1.33.0` at `c6a0c3c...`; one toolchain-only commit after npm source | `lightningcss-1.33.0-upstream-source.tar.gz` | `3197aa51...` |
| `lightningcss-win32-x64-msvc` | 1.33.0 | Historical npm build-time native binding; **not included** in this source-only release | `lightningcss-win32-x64-msvc-1.33.0/` (license/package metadata only; no `.node`) | n/a |

See `LIGHTNINGCSS_PROVENANCE.md` for the npm `gitHead`, tag commit, one-commit
difference, and manifest hashes. The npm-published source commit is `1d680fa`; the
GitHub tag points to `c6a0c3c`, whose only change is `rust-toolchain.toml`
(`1.92.0` to `1.96.0`). The root `Cargo.toml` version `1.0.0-alpha.72` is the internal
Rust crate version, not the npm wrapper version; both commits have byte-identical
`package.json` and root `Cargo.toml`.

## What each item is

- The five Cargo `.crate` files are the exact published source archives for the locked
  versions. They include the package source and, where upstream published one, the
  upstream license file.
- `selectors@0.38.0` does **not** contain a standalone license file in its published
  crate and the upstream repository has no root/`selectors` license file at the exact
  commit. Its Cargo metadata declares `MPL-2.0`; the upstream README states "Stylo is
  licensed under MPL 2.0". The exact README license excerpt and full-README hash are in
  `../third-party-licenses/packages/cargo/selectors@0.38.0/`; the MPL-2.0 text is at
  `../third-party-licenses/MPL-2.0.txt`.
- `cssparser-macros@0.7.1` is a proc-macro used while compiling `cssparser`; its source
  is included for completeness, but it is not linked into the final executable.
- `lightningcss@1.33.0` is the published npm package for the JS wrapper. The exact
  npm-source archive and the tag snapshot are both included; see
  `LIGHTNINGCSS_PROVENANCE.md`.
- `lightningcss-win32-x64-msvc@1.33.0` is a native binary npm package. Its prebuilt
  `.node` file is **not** included in this source-only release. Only `LICENSE`,
  `package.json`, and `README.md` metadata are retained for attribution/provenance; see
  `lightningcss-win32-x64-msvc-1.33.0/NO_BINARY_INCLUDED.md`.

## Source-only MPL note

This repository distributes exact-version Source Code Form. It does not distribute an
Executable Form, so the Section 3.2 mechanism is not triggered by this release.
Section 3.1 is addressed by `../../SOURCE_ONLY_LICENSE_NOTICE.md`, the exact archives,
the MPL-2.0 text, and the license files inside the archives where upstream provides
them.

`MPL_SOURCE_OFFER.md` is retained only as a template for a future binary release. It is
not an active offer for this source-only release, is not executed, and has no filled-in
contact.

## What remains uncertain

- Whether the exact source archive set is sufficient for every downstream
  redistribution scenario; upstream repositories and the exact lockfile versions are the
  authoritative references.
- Whether `selectors@0.38.0` has sufficient standalone notice despite having no separate
  upstream `LICENSE` file. The exact source archive, its 16 MPL headers, Cargo metadata,
  README statement, and MPL text are included; no license file was fabricated.
- Whether any MPL file is modified by SentinelLite. The project does not intentionally
  modify these dependencies.
- Whether a future binary release needs a written offer in addition to included source.

This directory is not a completed legal compliance package. The native binding
reproducibility question applies only to a future release that distributes the `.node`
binary; it is not part of this source-only release.
