use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

use sha2::{Digest, Sha256};
use verifysign::{CodeSignVerifier, Error as VerifyError, VerifyPolicy};

use crate::limits::MAX_FILE_HASH_BYTES;
use crate::model::{FileEvidence, FileHashState, SignatureInfo, SignatureState};
use crate::util::{is_local_absolute_path, system_time_to_rfc3339};

const TRUST_E_NOSIGNATURE: i32 = 0x800B_0100_u32 as i32;
const TRUST_E_PROVIDER_UNKNOWN: i32 = 0x800B_0001_u32 as i32;
const TRUST_E_SUBJECT_FORM_UNKNOWN: i32 = 0x800B_0003_u32 as i32;
const TRUST_E_BAD_DIGEST: i32 = 0x8009_6010_u32 as i32;
const TRUST_E_CERT_SIGNATURE: i32 = 0x8009_6004_u32 as i32;
const TRUST_E_EXPLICIT_DISTRUST: i32 = 0x800B_0111_u32 as i32;
const CERT_E_UNTRUSTEDROOT: i32 = 0x800B_0109_u32 as i32;
const CERT_E_CHAINING: i32 = 0x800B_010A_u32 as i32;
const CERT_E_EXPIRED: i32 = 0x800B_0101_u32 as i32;
const CERT_E_REVOKED: i32 = 0x800B_010C_u32 as i32;

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

/// Classifies WinVerifyTrust HRESULTs without treating every failure as malicious.
pub fn classify_hresult(code: i32) -> SignatureState {
    match code {
        TRUST_E_NOSIGNATURE | TRUST_E_PROVIDER_UNKNOWN | TRUST_E_SUBJECT_FORM_UNKNOWN => {
            SignatureState::Unsigned
        }
        TRUST_E_BAD_DIGEST | TRUST_E_CERT_SIGNATURE | CERT_E_EXPIRED | CERT_E_REVOKED => {
            SignatureState::Invalid
        }
        TRUST_E_EXPLICIT_DISTRUST | CERT_E_UNTRUSTEDROOT | CERT_E_CHAINING => {
            SignatureState::Untrusted
        }
        _ => SignatureState::Unavailable,
    }
}

fn detail_for_state(state: SignatureState, code: Option<i32>) -> String {
    match state {
        SignatureState::Valid => {
            "磁盘文件签名验证通过。仅代表文件身份与完整性检查结果，不代表软件行为或内存映像安全。"
                .to_string()
        }
        SignatureState::Unsigned => {
            "磁盘文件未发现可验证的 Authenticode 或系统目录签名。未签名不等于恶意。".to_string()
        }
        SignatureState::Invalid => format!(
            "磁盘文件签名未通过完整性或有效期验证（错误码 {}）。这可能来自文件损坏或证书问题，不能单独证明恶意。",
            format_hresult(code)
        ),
        SignatureState::Untrusted => format!(
            "磁盘文件签名存在，但证书链或系统信任策略未通过（错误码 {}）。受信任状态未知，不能单独证明恶意。",
            format_hresult(code)
        ),
        SignatureState::Unavailable => format!(
            "系统未能完成磁盘文件签名验证（{}）。结果为未知，不能解释为已签名、未签名或安全。",
            format_hresult(code)
        ),
    }
}

fn format_hresult(code: Option<i32>) -> String {
    match code {
        Some(value) => format!("HRESULT {:#010x}", value as u32),
        None => "无错误码".to_string(),
    }
}

fn signature_info(
    state: SignatureState,
    signer_name: Option<String>,
    detail: String,
    file_evidence: FileEvidence,
) -> SignatureInfo {
    SignatureInfo {
        state,
        signer_name,
        detail,
        verification_scope: "fileOnDiskOnly".to_string(),
        memory_verified: false,
        network_used: false,
        revocation_checked: false,
        file_evidence,
    }
}

fn bounded_file_hash(path: &Path) -> Result<Option<String>, io::Error> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total: u64 = 0;

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > MAX_FILE_HASH_BYTES {
            return Ok(None);
        }
        hasher.update(&buffer[..read]);
    }

    let digest = hasher.finalize();
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    Ok(Some(output))
}

/// Records bounded, on-demand disk-file evidence. It never probes UNC/device paths.
fn collect_file_evidence(path: &Path, display_path: &str) -> FileEvidence {
    if !is_local_absolute_path(display_path) {
        return FileEvidence::unavailable("路径不是本地绝对路径；未读取文件元数据或计算哈希。");
    }

    let initial_metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return FileEvidence {
                checked_path: Some(display_path.to_string()),
                size_bytes: None,
                modified_at: None,
                sha256: None,
                hash_state: FileHashState::Unavailable,
                note: format!("无法读取文件元数据：{error}"),
            };
        }
    };

    let size_bytes = initial_metadata.len();
    let modified_at = initial_metadata
        .modified()
        .ok()
        .and_then(system_time_to_rfc3339);

    if size_bytes > MAX_FILE_HASH_BYTES {
        return FileEvidence {
            checked_path: Some(display_path.to_string()),
            size_bytes: Some(size_bytes),
            modified_at,
            sha256: None,
            hash_state: FileHashState::SkippedTooLarge,
            note: format!(
                "文件超过 {} MiB 哈希上限，未计算 SHA-256。",
                MAX_FILE_HASH_BYTES / (1024 * 1024)
            ),
        };
    }

    let hash = match bounded_file_hash(path) {
        Ok(Some(hash)) => hash,
        Ok(None) => {
            return FileEvidence {
                checked_path: Some(display_path.to_string()),
                size_bytes: Some(size_bytes),
                modified_at,
                sha256: None,
                hash_state: FileHashState::SkippedTooLarge,
                note: format!(
                    "读取期间文件超过 {} MiB 哈希上限，未保留 SHA-256。",
                    MAX_FILE_HASH_BYTES / (1024 * 1024)
                ),
            };
        }
        Err(error) => {
            return FileEvidence {
                checked_path: Some(display_path.to_string()),
                size_bytes: Some(size_bytes),
                modified_at,
                sha256: None,
                hash_state: FileHashState::Unavailable,
                note: format!("无法读取文件用于哈希：{error}"),
            };
        }
    };

    let after_metadata = fs::metadata(path).ok();
    let changed = after_metadata.as_ref().is_some_and(|after| {
        after.len() != initial_metadata.len()
            || after.modified().ok() != initial_metadata.modified().ok()
    });

    if changed {
        return FileEvidence {
            checked_path: Some(display_path.to_string()),
            size_bytes: Some(size_bytes),
            modified_at,
            sha256: None,
            hash_state: FileHashState::Unavailable,
            note: "文件在哈希期间发生变化，已丢弃哈希结果。".to_string(),
        };
    }

    FileEvidence {
        checked_path: Some(display_path.to_string()),
        size_bytes: Some(size_bytes),
        modified_at,
        sha256: Some(hash),
        hash_state: FileHashState::Computed,
        note: "SHA-256 仅对应检查时的磁盘文件；不证明内存映像或进程行为。".to_string(),
    }
}

/// Verifies only a local on-disk file, without network access or revocation checks.
pub fn verify_file(path: &Path) -> SignatureInfo {
    let display_path = path.to_string_lossy();
    let file_evidence = collect_file_evidence(path, &display_path);

    if !is_local_absolute_path(&display_path) {
        return signature_info(
            SignatureState::Unavailable,
            None,
            "路径不是本地绝对路径；为避免访问网络或设备，未执行签名验证。".to_string(),
            file_evidence,
        );
    }

    let policy = VerifyPolicy::new().allow_network(false);
    let verifier = match CodeSignVerifier::new(policy) {
        Ok(verifier) => verifier,
        Err(error) => {
            return signature_info(
                SignatureState::Unavailable,
                None,
                format!("无法初始化签名验证器：{error}"),
                file_evidence,
            );
        }
    };

    match verifier.verify_file(path) {
        Ok(context) => {
            let subject = context.subject_name();
            let signer_name = subject.organization.or(subject.common_name);
            signature_info(
                SignatureState::Valid,
                signer_name,
                detail_for_state(SignatureState::Valid, None),
                file_evidence,
            )
        }
        Err(VerifyError::LeafCertNotFound) => signature_info(
            SignatureState::Valid,
            None,
            "磁盘文件签名验证通过，但无法读取签名证书主体。签名有效不代表行为安全。".to_string(),
            file_evidence,
        ),
        Err(VerifyError::Unsigned) => signature_info(
            SignatureState::Unsigned,
            None,
            detail_for_state(SignatureState::Unsigned, None),
            file_evidence,
        ),
        Err(VerifyError::OsError(code)) => {
            let state = classify_hresult(code);
            signature_info(
                state,
                None,
                detail_for_state(state, Some(code)),
                file_evidence,
            )
        }
        Err(VerifyError::InvalidPath) => signature_info(
            SignatureState::Unavailable,
            None,
            detail_for_state(SignatureState::Unavailable, None),
            file_evidence,
        ),
        Err(error) => signature_info(
            SignatureState::Unavailable,
            None,
            format!("签名验证未完成：{error}"),
            file_evidence,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn remote_paths_are_not_probed_for_file_evidence() {
        let evidence = collect_file_evidence(
            Path::new(r"\\server\share\payload.exe"),
            r"\\server\share\payload.exe",
        );
        assert_eq!(evidence.hash_state, FileHashState::Unavailable);
        assert!(evidence.checked_path.is_none());
    }
}
