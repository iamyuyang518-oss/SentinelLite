# Microsoft WebView2Loader 1.0.3800.47 provenance

> **Historical binary record.** This directory is retained for license/NOTICE and
> provenance documentation. No WebView2Loader DLL, static library, or other binary is
> included in the SentinelLite 0.1.0 source-only release.

- Component: `WebView2Loader` (`WebView2LoaderStatic.lib`, `WebView2Loader.dll`,
  `WebView2Loader.dll.lib`)
- Version: `1.0.3800.47`
- Source package: Microsoft.Web.WebView2 1.0.3800.47 NuGet package
- Package URL: https://api.nuget.org/v3-flatcontainer/microsoft.web.webview2/1.0.3800.47/microsoft.web.webview2.1.0.3800.47.nupkg
- Package SHA-256: `56c9f26bdd07916a2d1949fb58a5c7e434dfa1173577dca879206050c4e718db`
- Package metadata: `Microsoft.Web.WebView2.nuspec` in this directory
- License: `LICENSE.txt` (Microsoft BSD-3-Clause-style terms)
- NOTICE: `NOTICE.txt`
- License SHA-256: `0af8f1b807512aae39c2ac1aa4d0cae65cabecb6fd554b8439a5162a0d6eca55`
- NOTICE SHA-256: `106423785c5b7eba0a8e61d1837f2132e9c828e20ad530f565d981c1df60dd90`

## How it enters SentinelLite

`webview2-com-sys@0.39.1` is a Windows runtime crate. Its `src/lib.rs` uses
`#[link(name = "WebView2LoaderStatic", kind = "static")]` for MSVC targets, and its
`build.rs` copies the x64/arm64/x86 loader files from the crate into `OUT_DIR`. The
historical SentinelLite executable had no separate `WebView2Loader.dll` import; the
loader was linked statically. The historical installer packaged only the main
`sentinel-lite.exe` binary. No such binary is part of this source-only release.

The crate's x64 loader files are byte-identical to the corresponding files in the
Microsoft.Web.WebView2 1.0.3800.47 NuGet package:

| File | SHA-256 | NuGet match |
| --- | --- | --- |
| `x64/WebView2Loader.dll` | `86545b66cdb0603bc26b626fb9ad610cb6e71f28d468f5ea66df23b03dda96d5` | identical |
| `x64/WebView2Loader.dll.lib` | `bfc8ccaaa056be95243a5b66a827e5849d2bb39676fca4dcc2053796d8e15c6` | identical |
| `x64/WebView2LoaderStatic.lib` | `89c6b872783b8f6c3cedbff618adb42082d455c615453ae10cfc753f1e8f25d8` | identical |

The `LICENSE.txt` and `NOTICE.txt` here are the exact files from that NuGet package.
They are not covered by the MIT license of the `webview2-com`/`webview2-com-sys` Rust
bindings. The NOTICE includes third-party notices for tools/components in the WebView2
SDK package; it is reproduced in full rather than filtered.
