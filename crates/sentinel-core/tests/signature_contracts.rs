use std::path::Path;

use sentinel_core::model::SignatureState;
use sentinel_core::signature::{classify_hresult, verify_file};

#[test]
fn bad_digest_is_invalid_not_unsigned() {
    assert_eq!(
        classify_hresult(0x8009_6010_u32 as i32),
        SignatureState::Invalid
    );
}

#[test]
fn explicit_distrust_and_untrusted_root_stay_distinct_from_invalid_digest() {
    assert_eq!(
        classify_hresult(0x800B_0111_u32 as i32),
        SignatureState::Untrusted
    );
    assert_eq!(
        classify_hresult(0x800B_0109_u32 as i32),
        SignatureState::Untrusted
    );
    assert_eq!(
        classify_hresult(0x800B_010A_u32 as i32),
        SignatureState::Untrusted
    );
}

#[test]
fn no_signature_is_unsigned() {
    assert_eq!(
        classify_hresult(0x800B_0100_u32 as i32),
        SignatureState::Unsigned
    );
}

#[test]
fn remote_or_relative_paths_are_not_probed_for_signatures() {
    let info = verify_file(Path::new(r"\\server\share\payload.exe"));

    assert_eq!(info.state, SignatureState::Unavailable);
    assert!(!info.network_used);
    assert!(info.detail.contains("未执行签名验证"));
}

#[test]
fn file_access_and_unknown_failures_are_unavailable() {
    assert_eq!(
        classify_hresult(0x8007_0002_u32 as i32),
        SignatureState::Unavailable
    );
    assert_eq!(
        classify_hresult(0x8007_0005_u32 as i32),
        SignatureState::Unavailable
    );
    assert_eq!(
        classify_hresult(0xDEAD_BEEF_u32 as i32),
        SignatureState::Unavailable
    );
}
