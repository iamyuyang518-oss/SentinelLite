# SentinelLite 0.1.0 source-only release risk record

Status: **SOURCE-ONLY GATE PASS (engineering assessment)**

GitHub publication: **not completed** — `gh`/GitHub credentials were not available in
the execution environment.

This record is an engineering gate, not a legal opinion. It distinguishes the
source-only distribution from the earlier binary-release position. No public GitHub
repository, tag, Release, or asset upload was created.

## Gate result

The public tree is a source-only distribution. It contains no application, installer,
sensor, native `.node`, or other executable build output. Under that scope:

- MPL-2.0 Section 3.2 executable-form source-availability is **not triggered** by this
  repository because no Executable Form is distributed.
- MPL-2.0 Section 3.1 is addressed by including the exact MPL source archives, the
  MPL-2.0 text, upstream license files where available, and the dedicated
  `SOURCE_ONLY_LICENSE_NOTICE.md`.
- The earlier binary-specific native binding and source-offer questions do not apply to
  this release. The native package directory is metadata/license only, with no `.node`.
- The remaining `selectors` standalone-license gap and the 39/134 inventory/copyright
  gaps are recorded as residual risks, not converted into invented notices or a claim
  of legal compliance.

This is a source-only engineering gate result. It is not a legal opinion and does not
mean every legal question is resolved.

## Residual license/source items

### R1. `selectors@0.38.0` has no standalone upstream license file

- The exact published crate and the upstream `servo/stylo` tree at commit
  `572ecba2d1600e7c3d490586692a209faf703baa` have no separate
  `LICENSE`/`COPYING`/`NOTICE`/`COPYRIGHT` file for `selectors`.
- The exact source archive is included at
  `third-party-source/mpl/archives/selectors-0.38.0.crate`.
- All 16 Rust source files in that archive carry the standard MPL-2.0 header.
- The crate metadata declares `MPL-2.0`; the upstream README states "Stylo is licensed
  under MPL 2.0".
- The standard MPL text is included at `third-party-licenses/MPL-2.0.txt`.
- Evidence is in `third-party-licenses/packages/cargo/selectors@0.38.0/`.
- Residual: no standalone upstream license file exists. This is a source/notice
  packaging gap that remains available for legal review. It is not treated as a
  source-only binary Section 3.2 obligation.

### R2. MPL Section 3.1 notice scope

- The repository distributes selected MPL-2.0 source archives. It does not distribute
  an Executable Form, so Section 3.2 is not triggered by this release.
- The source-only notice, MPL text, source archive manifest, and exact archives are
  included so recipients are informed that the source is MPL-2.0 and where the license
  text is available.
- Residual: whether the notice package is sufficient for every downstream
  redistribution scenario is a legal question. The project does not claim a legal
  conclusion.

### R3. `lightningcss-win32-x64-msvc` native package

- The `.node` binary is **not present** in the public tree.
- Only `LICENSE`, `README.md`, and `package.json` metadata are retained for attribution
  and provenance; see
  `third-party-source/mpl/lightningcss-win32-x64-msvc-1.33.0/`.
- Because no compiled native binding is distributed, its corresponding-source and
  reproducibility question does not apply to this source-only release.
- If a future release distributes a built binary or the `.node` package, that release
  must revisit MPL Section 3.2 and the native binding source position.

### R4. MPL written-offer template

- `third-party-source/mpl/MPL_SOURCE_OFFER.md` remains a template only.
- It is **not** an active offer for this source-only release. The exact source archives
  are included directly.
- If a future binary release uses a written offer, the template still needs legal review,
  a release hash, a source URL, a contact channel, and an effective period.

### R5. Remaining package inventory and copyright gaps

- 39 of 635 packages have no standalone license file in their installed package root.
  Only `selectors@0.38.0` was in the earlier runtime binary graph; the public tree does
  not distribute those other package sources.
- 134 rows still lack a specific copyright line: 36 among the 39 no-standalone-file
  packages and 98 with only generic license boilerplate.
- No copyright year, author, or license text was fabricated.
- The complete analysis is in `third-party-licenses/COPYRIGHT_REVIEW.md`,
  `third-party-licenses/MISSING_LICENSE_FILES.md`, and
  `third-party-licenses/COPYRIGHT_INDEX.md`.
- Residual: the license inventory is not a complete legal audit. For source-only
  distribution, the primary distributed MPL archives are covered by the source notice
  and exact files; the broader inventory gaps remain a documentation/legal-review item.

## Non-license release gaps

- No formal legal review, third-party security audit, penetration test, fuzzing campaign,
  or malware-sample evaluation has been completed.
- Windows 10, ARM64, enterprise-policy environments, multi-DPI automation, and clean-VM
  builds are not fully validated.
- The project is not an antivirus or EDR product and does not claim overall protection.
- The source tree's dependency advisories remain as recorded in the project
  documentation, including the RustSec/`npm audit` items.

## What was verified for this source-only tree

- Public tree contains no `.exe`, `.dll`, `.pdb`, `.obj`, `.lib`, `.node`, `.msi`,
  static-library, database, log, `target/`, or other executable build output.
- No `.git` directory or inherited development history is present.
- No detected real username, hostname, or development-machine path.
- Relative Markdown links resolve.
- Source/configuration/script content matches the isolated build tree (documentation
  edits excepted); no runtime code or lockfile changes were made.
- Source SBOM parses and passes the official CycloneDX 1.5 JSON schema validation with
  0 errors: 642 components, 643 dependency entries.
- Exact MPL source archives and the license bundle have checksum manifests.

## Shortest resolution path

1. **No source-only license clearance is required to prepare the local repository** under
   the engineering gate above; the residual items are recorded rather than silently
   accepted.
2. **Before any public push**, the owner/legal reviewer should record acceptance of R1
   and R5 or obtain a `selectors` upstream clarification and a formal license review.
3. If a public GitHub repository is created, publish the source tree only. Do not upload
   any app, installer, sensor, `.node`, or other executable artifact.
4. If a future binary release is desired, treat it as a new distribution event and redo
   MPL Section 3.2, native-binding, and written-offer review.
5. No rebuild is required for the source-only release. The public tree is the source of
   truth; GitHub's generated source ZIP/TAR.GZ is the release source artifact.

## Explicit non-claims

- This is not a legal opinion or a legal compliance certification.
- The `selectors` gap and the 39/134 inventory gaps are not "solved" by this notice.
- The source SBOM is not a file-by-file inventory of the repository.
- No binary release, installer, or prebuilt application is provided.
- No public GitHub repository or Release was created in this pass; GitHub tooling and
  credentials were unavailable.
