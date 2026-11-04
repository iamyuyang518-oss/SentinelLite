# webview2-com-sys 0.39.1 upstream license provenance

- Package: `webview2-com-sys` 0.39.1
- Ecosystem: crates.io / Cargo
- Crate archive SHA-256: `b3a07132775117d6065853d9d1178157b8c90e228de47129d6bce2c7edebedfb`
- `.cargo_vcs_info.json` git commit: `edc2caf886175ccaebe86078c9cfe1ae2a187328`
- `.cargo_vcs_info.json` path in VCS: `crates/bindings`
- Upstream repository: https://github.com/wravery/webview2-rs
- License file at that commit: https://github.com/wravery/webview2-rs/blob/edc2caf886175ccaebe86078c9cfe1ae2a187328/LICENSE
- GitHub blob SHA: `de9a7e660e173136b351982a34f715af424ff812`
- Retrieved file SHA-256: `0dcf41516e608bbcb6cdc5229feb7b86fe4a643b85e7df251133c93408fdac73`
- Declared license: `MIT`
- Upstream copyright line: `Copyright (c) 2021 Bill Avery`

The published crate archive did not contain a standalone license file. `LICENSE.upstream`
is the exact file from the commit recorded by Cargo in `.cargo_vcs_info.json`.

## Embedded Microsoft WebView2Loader

This crate ships `WebView2Loader.dll`, `WebView2Loader.dll.lib`, and
`WebView2LoaderStatic.lib`. For MSVC targets its `src/lib.rs` selects
`WebView2LoaderStatic` with `kind = "static"`; the final SentinelLite executable has no
separate `WebView2Loader.dll` dependency. The loader files are byte-identical to the
x64 files in the Microsoft.Web.WebView2 1.0.3800.47 NuGet package. Their Microsoft
license and NOTICE are recorded separately at
`../../native/webview2-loader@1.0.3800.47/`.
