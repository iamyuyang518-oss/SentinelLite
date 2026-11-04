# Packages without a standalone license file

This report records packages whose installed roots did not contain a `LICENSE*`/`COPYING*`/`NOTICE*` file. No license text was invented. The inventory covers the locked source dependency graph; the source-only public tree does not distribute every locked package's source.

Resolved in the supplemental pass: `alloc-stdlib`, `webview2-com`, and `webview2-com-sys` now carry exact upstream `LICENSE` files; `esrecurse` and `imurmurhash` now carry license text extracted from their upstream READMEs. `selectors` still has no standalone upstream license file; its upstream README license statement and exact source archive are recorded in `packages/cargo/selectors@0.38.0/UPSTREAM_LICENSE_EVIDENCE.md`.

| Ecosystem | Package | Version | License expression | Distribution scope | Source |
| --- | --- | --- | --- | --- | --- |
| cargo | block2 | 0.6.2 | `MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | dispatch2 | 0.3.1 | `Zlib OR Apache-2.0 OR MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | dlopen2 | 0.8.2 | `MIT` | `locked-not-selected` | https://github.com/OpenByteDev/dlopen2 |
| cargo | dlopen2_derive | 0.4.3 | `MIT` | `locked-not-selected` | https://github.com/OpenByteDev/dlopen2 |
| cargo | jni-sys-macros | 0.4.1 | `MIT OR Apache-2.0` | `locked-not-selected` | https://github.com/jni-rs/jni-sys |
| cargo | libappindicator-sys | 0.9.0 | `Apache-2.0 OR MIT` | `locked-not-selected` | https://crates.io/crates/libappindicator-sys/0.9.0 |
| cargo | ndk | 0.9.0 | `MIT OR Apache-2.0` | `locked-not-selected` | https://github.com/rust-mobile/ndk |
| cargo | ndk-context | 0.1.1 | `MIT OR Apache-2.0` | `locked-not-selected` | https://github.com/rust-windowing/android-ndk-rs |
| cargo | ndk-sys | 0.6.0+11769913 | `MIT OR Apache-2.0` | `locked-not-selected` | https://github.com/rust-mobile/ndk |
| cargo | objc2 | 0.6.5 | `MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | objc2-app-kit | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | objc2-cloud-kit | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | objc2-core-data | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | objc2-core-foundation | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | objc2-core-graphics | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | objc2-core-image | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | objc2-core-location | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | objc2-core-text | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | objc2-encode | 4.1.0 | `MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | objc2-exception-helper | 0.1.1 | `Zlib OR Apache-2.0 OR MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | objc2-foundation | 0.3.2 | `MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | objc2-io-surface | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | objc2-quartz-core | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | objc2-ui-kit | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | objc2-user-notifications | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | objc2-web-kit | 0.3.2 | `Zlib OR Apache-2.0 OR MIT` | `locked-not-selected` | https://github.com/madsmtm/objc2 |
| cargo | r-efi | 5.3.0 | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | `locked-not-selected` | https://github.com/r-efi/r-efi |
| cargo | r-efi | 6.0.0 | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | `locked-not-selected` | https://github.com/r-efi/r-efi |
| cargo | rsqlite-vfs | 0.2.0 | `MIT` | `locked-not-selected` | https://github.com/Spxg/sqlite-wasm-rs |
| cargo | selectors | 0.38.0 | `MPL-2.0` | `runtime` | https://github.com/servo/stylo |
| cargo | wasip2 | 1.0.1+wasi-0.2.4 | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | `locked-not-selected` | https://github.com/bytecodealliance/wasi-rs |
| cargo | webview2-com-macros | 0.8.1 | `MIT` | `runtime-graph-proc-macro` | https://github.com/wravery/webview2-rs |
| cargo | winapi-i686-pc-windows-gnu | 0.4.0 | `MIT/Apache-2.0` | `locked-not-selected` | https://github.com/retep998/winapi-rs |
| cargo | winapi-x86_64-pc-windows-gnu | 0.4.0 | `MIT/Apache-2.0` | `locked-not-selected` | https://github.com/retep998/winapi-rs |
| npm | @humanfs/types | 0.15.0 | `Apache-2.0` | `dev` | https://registry.npmjs.org/@humanfs/types/-/types-0.15.0.tgz |
| npm | @rolldown/binding-win32-x64-msvc | 1.2.15 | `MIT` | `dev` | https://registry.npmjs.org/@rolldown/binding-win32-x64-msvc/-/binding-win32-x64-msvc-1.2.15.tgz |
| npm | @tauri-apps/cli-win32-x64-msvc | 2.12.4 | `Apache-2.0 OR MIT` | `dev` | https://registry.npmjs.org/@tauri-apps/cli-win32-x64-msvc/-/cli-win32-x64-msvc-2.12.4.tgz |
| npm | natural-compare | 1.4.0 | `MIT` | `dev` | https://registry.npmjs.org/natural-compare/-/natural-compare-1.4.0.tgz |
| npm | saxes | 6.0.0 | `ISC` | `dev` | https://registry.npmjs.org/saxes/-/saxes-6.0.0.tgz |

Total: **39** of **635** packages. This count excludes the embedded Microsoft WebView2Loader native component, which is documented separately.
