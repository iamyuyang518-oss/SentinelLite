# MPL-2.0 Section 3.2 engineering assessment

This is an engineering assessment of source availability for SentinelLite 0.1.0. It is
not a legal opinion and does not conclude that Section 3.2 is satisfied.

## Source-only release scope

The public SentinelLite 0.1.0 repository is source-only. It does not distribute an
application, installer, sensor, native `.node`, or any other Executable Form. Therefore
the executable-form source-availability mechanics of MPL-2.0 Section 3.2 are not
triggered by this repository. The applicable source-distribution notice is
`SOURCE_ONLY_LICENSE_NOTICE.md`, which includes the MPL-2.0 text and exact archive
locations.

The native `lightningcss-win32-x64-msvc` `.node` binary is not included. Only its
license/README/package metadata is retained for provenance. A future release that
distributes that binary must revisit this assessment.

## Component-by-component

| Component | Version | Distribution role | Included source | Section 3.2 status / issue |
| --- | --- | --- | --- | --- |
| `cssparser` | 0.37.0 | Runtime Cargo crate; crate-level binary upper bound | Exact crates.io `.crate` | Source Code Form of the published crate is included; `.crate` checksum matches `Cargo.lock`. Whether the published `.crate` is sufficient for every downstream form is a legal question. |
| `dtoa-short` | 0.3.5 | Runtime Cargo crate | Exact `.crate` | Same as above. |
| `option-ext` | 0.2.0 | Runtime Cargo crate | Exact `.crate` | Same as above. |
| `selectors` | 0.38.0 | Runtime Cargo crate | Exact `.crate`; upstream README evidence | Source is included, but upstream publishes no standalone license file and no explicit copyright line. MPL text is included from the standard license set. Notice completeness remains a legal/review question. |
| `cssparser-macros` | 0.7.1 | Proc-macro used at compile time; not linked into the executable | Exact `.crate` | Included for completeness. If the proc-macro is not distributed in the binary package, Section 3.2 may not be triggered for it; that distribution characterization is a legal question. |
| `lightningcss` | 1.33.0 | npm build-time CSS tool; not in the Rust executable | npm package snapshot; exact npm-`gitHead` source archive; tag snapshot | Included for source completeness. If it is only a build tool and is not distributed, Section 3.2 may not be triggered. If a distribution channel treats the npm package as distributed Covered Software, source is available. Exact correspondence is documented in `LIGHTNINGCSS_PROVENANCE.md`. |
| `lightningcss-win32-x64-msvc` | 1.33.0 | Historical npm build-time native binding; not in the Rust executable and not included in the source-only release | Native package metadata/license only; no `.node` | The npm package contains a prebuilt `.node` and no source. The `.node` is not included in this source-only release, so Section 3.2 is not triggered here. If a future release distributes it, corresponding source/reproducibility must be revisited. |

## Distribution scenarios

0. **This source-only public release**
   - The repository distributes Source Code Form only and no Executable Form.
   - Section 3.2 is not triggered. Section 3.1 notice is in
     `SOURCE_ONLY_LICENSE_NOTICE.md`; exact archives and MPL text are included.
   - `selectors` has no standalone upstream license file, but its exact archive,
     MPL headers, metadata, README statement, and the included MPL text are the
     available evidence.

1. **Future Windows installer / portable executable only**
   - The MPL runtime crates are `cssparser`, `dtoa-short`, `option-ext`, and
     `selectors`.
   - The exact Cargo `.crate` archives are included in this directory, together with
     the MPL text and the upstream evidence for `selectors`.
   - `cssparser-macros`, `lightningcss`, and the native binding are build-time inputs
     and are not in the generated installer.
   - The source archive set is the primary source-availability mechanism. No written
     offer is executed.

2. **Source repository / npm build environment**
   - The source repository contains `package-lock.json` and the npm build toolchain;
     `lightningcss` and its native binding are installed as build dependencies.
   - The exact npm package snapshot and the exact npm-source archive are included.
   - The native binding still has no source in the npm package.

3. **Separate distribution of the npm package or native binding**
   - The npm wrapper's exact source is included.
   - The native `.node` binding has no source in its npm package; only the upstream
     source archive is available. If that binding is distributed separately, the
     corresponding-source question needs separate review.

## Section 3.2 mechanics

MPL-2.0 Section 3.2 requires the Source Code Form of Covered Software to be made
available in a reasonable manner. The preferred mechanism here is distribution of the
exact source archives alongside the binaries/materials. If a distributor instead uses a
written offer, the offer must state its effective period, access method, scope, and
contact channel. `MPL_SOURCE_OFFER.md` is only a template and is not executed.

## Open legal questions

- Whether the published `.crate` archives are sufficient Source Code Form for all
  downstream recipients in a future binary release.
- Whether the `selectors` upstream README statement, MPL source headers, exact archive,
  and standard MPL text are sufficient notice given the absence of a standalone
  upstream license/copyright file.
- Whether any MPL file was modified by SentinelLite. The project does not intentionally
  modify these dependencies.
- Whether a future binary release that distributes `lightningcss-win32-x64-msvc` needs
  corresponding source and a written offer.

For the source-only release, no Executable Form is distributed, so the executable-form
Section 3.2 questions above are not triggered by this repository. Do not cite this file
as a legal conclusion that any future binary distribution satisfies Section 3.2.
