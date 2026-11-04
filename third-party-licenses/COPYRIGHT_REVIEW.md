# Copyright extraction review

This is an engineering review of the 143 `MANIFEST.csv` rows that had no extracted
copyright line in the first bundle. It is not a legal opinion.

## Original 143 rows

| Category | Count | Meaning |
| --- | ---: | --- |
| No standalone license file | 44 | The installed package root had no `LICENSE*`/`COPYING*`/`NOTICE*` file. |
| License file present, but no specific copyright line extracted | 99 | The license text was present, but it contained only generic Apache/MIT/BSD/CC0 boilerplate or the extractor did not match a specific line. |

The 99 rows with license files break down as:

- **97**: the license text contains only generic references such as "copyright
  owner", "copyright notice", "copyright holders", or "Copyright and Related Rights";
  there is no named copyright holder in the file.
- **1**: `dunce` uses the CC0 `Copyright and Related Rights` phrase; this is generic
  CC0 wording, not a named copyright statement.
- **1**: `balanced-match` is a real extraction miss. Its `LICENSE.md` contains
  `Original code Copyright Julian Gruber ...` and `Port to TypeScript Copyright Isaac
  Z. Schlueter ...`; both are now in `MANIFEST.csv`.

No case was found where the extractor removed a line that named a holder and year but
was otherwise valid.

## Supplemental changes made

| Package | What was added | Result |
| --- | --- | --- |
| `alloc-stdlib@0.3.0` | Exact upstream `LICENSE` at the crate's `.cargo_vcs_info.json` commit | BSD-3-Clause text and `Copyright (c) 2016 Dropbox, Inc.` |
| `webview2-com@0.39.1` | Exact upstream `LICENSE` at the crate's commit | MIT text and `Copyright (c) 2021 Bill Avery` |
| `webview2-com-sys@0.39.1` | Exact upstream `LICENSE` at the crate's commit | MIT text and `Copyright (c) 2021 Bill Avery` |
| `esrecurse@4.3.0` | License section extracted verbatim from the published README | BSD-2-Clause text and `Copyright (C) 2014 Yusuke Suzuki` |
| `imurmurhash@0.1.4` | License section extracted verbatim from the published README | MIT text and `Copyright (c) 2013 Gary Court, Jens Taylor` |
| `natural-compare@1.4.0` | Exact README/source copyright lines recorded as evidence | Copyright line added; no standalone license text |
| `winapi-i686-pc-windows-gnu@0.4.0` | Exact source-header notice recorded as evidence | Copyright line added; no standalone license text |
| `winapi-x86_64-pc-windows-gnu@0.4.0` | Exact source-header notice recorded as evidence | Copyright line added; no standalone license text |
| `balanced-match@4.0.4` | Existing license text re-read; two holder lines recorded | Copyright lines added |

## Remaining 134 rows without a copyright line

The current manifest has 134 empty `copyright_lines` fields:

- **36** have no standalone license file and no specific copyright line in the
  package source scan.
- **98** have a license file whose text contains only generic license boilerplate;
  no named holder is present in the file.

The 36 no-license rows are:

- Cargo: `block2`, `dispatch2`, `dlopen2`, `dlopen2_derive`, `jni-sys-macros`,
  `libappindicator-sys`, `ndk`, `ndk-context`, `ndk-sys`, `objc2`, `objc2-app-kit`,
  `objc2-cloud-kit`, `objc2-core-data`, `objc2-core-foundation`,
  `objc2-core-graphics`, `objc2-core-image`, `objc2-core-location`,
  `objc2-core-text`, `objc2-encode`, `objc2-exception-helper`, `objc2-foundation`,
  `objc2-io-surface`, `objc2-quartz-core`, `objc2-ui-kit`,
  `objc2-user-notifications`, `objc2-web-kit`, `r-efi@5.3.0`, `r-efi@6.0.0`,
  `rsqlite-vfs`, `selectors`, `wasip2`, `webview2-com-macros`.
- npm: `@humanfs/types`, `@rolldown/binding-win32-x64-msvc`,
  `@tauri-apps/cli-win32-x64-msvc`, `saxes`.

All except `selectors` are outside the selected Windows runtime graph or are
build/target-specific. `selectors` is the runtime MPL case; its upstream README
license statement is recorded in its package directory, but upstream published no
standalone license or explicit copyright line.

## What was not done

- No copyright holder, year, or license text was invented.
- The `authors` metadata field was not converted into a copyright statement.
- The 98 generic-boilerplate rows were not assigned a holder that the upstream license
  did not state.
- No claim is made that every source file's copyright header has been collected; the
  review covers package-level license/NOTICE files plus the source scan described
  above.
