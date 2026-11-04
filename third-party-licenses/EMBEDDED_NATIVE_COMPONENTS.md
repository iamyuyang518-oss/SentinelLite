# Embedded native components (historical binary record)

**Status: historical record; not part of the SentinelLite 0.1.0 source-only release.**
No WebView2Loader binary is included in the public tree. The license/NOTICE metadata is
retained for attribution and provenance.

These components were in an earlier final Windows binary but are not separate Cargo/npm
package rows in `MANIFEST.csv`.

## Microsoft WebView2Loader 1.0.3800.47

- Path: `packages/native/webview2-loader@1.0.3800.47/`
- Origin: Microsoft.Web.WebView2 1.0.3800.47 NuGet package
- Package SHA-256: `56c9f26bdd07916a2d1949fb58a5c7e434dfa1173577dca879206050c4e718db`
- License: `LICENSE.txt` (Microsoft BSD-3-Clause-style terms)
- NOTICE: `NOTICE.txt`
- License SHA-256: `0af8f1b807512aae39c2ac1aa4d0cae65cabecb6fd554b8439a5162a0d6eca55`
- NOTICE SHA-256: `106423785c5b7eba0a8e61d1837f2132e9c828e20ad530f565d981c1df60dd90`

`webview2-com-sys@0.39.1` copies the x64/arm64/x86 loader files from the crate and
selects `WebView2LoaderStatic.lib` for MSVC static linking. The historical SentinelLite
executable had no `WebView2Loader.dll` import; the historical installer packaged only
the main executable. The x64 crate files were byte-identical to the NuGet 1.0.3800.47
files:

| File | SHA-256 | NuGet match |
| --- | --- | --- |
| `x64/WebView2Loader.dll` | `86545b66cdb0603bc26b626fb9ad610cb6e71f28d468f5ea66df23b03dda96d5` | identical |
| `x64/WebView2Loader.dll.lib` | `bfc8ccaaa056be95243a5b66a827e5849d2bb39676fca4dcc2053796d8e15c6` | identical |
| `x64/WebView2LoaderStatic.lib` | `89c6b872783b8f6c3cedbff618adb42082d455c615453ae10cfc753f1e8f25d8` | identical |

The crate's MIT license covers the Rust bindings, not this Microsoft component. The
Microsoft `NOTICE.txt` also contains third-party notices for WebView2 SDK
tools/components; it is reproduced in full rather than filtered.

## Scope note

The binary SBOM includes this component separately from the 196 Cargo runtime crates
and 4 npm production candidates. This is a crate/package-level composition record, not
a symbol-level linker proof.
