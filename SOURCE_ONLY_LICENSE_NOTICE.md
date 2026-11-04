# Source-only license notice

SentinelLite 0.1.0 is being published as a **source-only** release. This repository
contains source code, configuration, documentation, license materials, selected
third-party source archives, and SBOM records. It does **not** contain a Windows
installer, a prebuilt `sentinel-lite.exe`, a `SentinelLiteSensor.exe`, a WebView2
binary, a `lightningcss-win32-x64-msvc` prebuilt `.node` file, or any other executable
build output.

This notice is an engineering statement, not legal advice and not a legal opinion.

## MPL-2.0 source distribution

The following MPL-2.0 source archives are distributed in this repository:

- `third-party-source/mpl/archives/cssparser-0.37.0.crate`
- `third-party-source/mpl/archives/dtoa-short-0.3.5.crate`
- `third-party-source/mpl/archives/option-ext-0.2.0.crate`
- `third-party-source/mpl/archives/selectors-0.38.0.crate`
- `third-party-source/mpl/archives/cssparser-macros-0.7.1.crate`
- `third-party-source/mpl/lightningcss-1.33.0-npm-package.tgz`
- `third-party-source/mpl/lightningcss-1.33.0-npm-head-1d680fa-source.tar.gz`
- `third-party-source/mpl/lightningcss-1.33.0-upstream-source.tar.gz`

The MPL-2.0 license text is at `third-party-licenses/MPL-2.0.txt`. The exact archive
provenance and SHA-256 values are in `third-party-source/mpl/MANIFEST.csv`,
`third-party-source/mpl/SHA256SUMS`, and `third-party-source/mpl/LICENSE*` files where
the upstream archive provides one.

Except for the source archives identified above, SentinelLite does not distribute the
source code of its locked third-party dependencies. `Cargo.lock` and
`package-lock.json` are dependency-resolution metadata, not source distributions.

## `selectors@0.38.0`

The upstream `selectors` crate has no standalone `LICENSE` file in its published crate
or at the exact upstream commit. The exact source archive is included. Its 16 Rust
source files carry the standard MPL-2.0 header (verified during the source-only gate):

```text
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */
```

The crate metadata declares `MPL-2.0`, the upstream README states "Stylo is licensed
under MPL 2.0", and the exact evidence is in
`third-party-licenses/packages/cargo/selectors@0.38.0/`. No separate upstream license
file was fabricated.

## MPL Section 3.1 and 3.2

This release is Source Code Form only. It does not distribute an Executable Form, so
MPL-2.0 Section 3.2's executable-form source-availability mechanics are not triggered
by this repository. Section 3.1 is addressed by including the exact source archives,
the MPL-2.0 text, the upstream license files where available, and this notice.

The native `lightningcss-win32-x64-msvc` metadata directory is retained for license and
provenance documentation. Its prebuilt `.node` file is not included. The
`third-party-source/mpl/MPL_SOURCE_OFFER.md` template is retained only as a historical
template for a future binary release; it is not an active offer for this source-only
release.

## Remaining review

The following are still engineering/legal review items, not claims of legal
compliance:

- `selectors@0.38.0` has no standalone upstream license file; the header, metadata,
  README statement, MPL text, and exact source archive are the available evidence.
- 39 of 635 packages in the license inventory have no standalone license file in their
  installed package root, and 134 rows lack a specific copyright line. These findings
  are recorded, not hidden; see `third-party-licenses/COPYRIGHT_REVIEW.md` and
  `third-party-licenses/MISSING_LICENSE_FILES.md`.
- This repository has not received a formal legal review. Do not describe it as
  "license-compliant" solely because this notice and the source bundle exist.

## License text and provenance

- MPL-2.0: `third-party-licenses/MPL-2.0.txt`
- Per-package manifest: `third-party-licenses/MANIFEST.csv`
- Source archive manifest: `third-party-source/mpl/MANIFEST.csv`
- Source archive checksums: `third-party-source/mpl/SHA256SUMS`
- Source-only gate record: `RELEASE_RISKS.md`
