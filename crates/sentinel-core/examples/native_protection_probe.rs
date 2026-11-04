//! Read-only real-machine probe for the native protection status slice.
//!
//! Usage:
//!   cargo run -p sentinel-core --example native_protection_probe
//!   cargo run -p sentinel-core --example native_protection_probe -- --output <path>

fn main() {
    let status = sentinel_core::native_protection::query_native_protection();
    let json =
        serde_json::to_string_pretty(&status).expect("native protection status is serializable");

    let mut args = std::env::args().skip(1);
    if let Some(flag) = args.next() {
        if flag == "--output" {
            let path = args.next().expect("--output requires a path");
            std::fs::write(&path, format!("{json}\n")).expect("write probe output");
            println!("wrote {path}");
            return;
        }
    }

    println!("{json}");
}
