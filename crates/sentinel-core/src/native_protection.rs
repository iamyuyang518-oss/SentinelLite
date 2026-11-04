//! Read-only observation of Windows Defender and Windows Firewall state.
//!
//! This module never changes a protection setting. It returns an explicit "unknown" state for
//! every field it cannot read and never derives an overall "protected" verdict. The UI may show
//! an observed switch as enabled, but that is not a statement that the machine is protected.

use crate::com::ComApartment;
use crate::model::{
    DefenderStatus, FirewallProfile, FirewallProfileStatus, FirewallStatus, NativeProtectionIssue,
    NativeProtectionObservation, NativeProtectionProviderState, NativeProtectionStatus,
};
use crate::util::now_rfc3339;

const MAX_SIGNATURE_VERSION_CHARS: usize = 128;
const WMI_NEXT_TIMEOUT_MS: i32 = 5_000;

const LIMITATIONS: [&str; 3] = [
    "只读取 Windows Defender 与 Windows 防火墙的开关和签名状态，不验证实际拦截能力、规则完整性或策略绕过。",
    "查询失败、提供程序不可用或字段缺失时保持未知，不会回退为“已启用”，也不会推断整体保护正常。",
    "不修改、不关闭原生防护；不启动子进程；不改变 SentinelLite 的整体保护状态。",
];

/// Queries the local Windows protection providers. This function is intentionally infallible:
/// failures are represented in the returned status so the UI cannot lose the distinction between
/// "disabled" and "unknown".
pub fn query_native_protection() -> NativeProtectionStatus {
    let (defender, firewall) = match ComApartment::initialize() {
        Ok(_apartment) => (
            build_defender_status(query_defender()),
            build_firewall_status(query_firewall()),
        ),
        Err(code) => {
            let failure = ProviderFailure::from_hresult("NATIVE-PROTECTION", "原生防护", code);
            (
                build_defender_status(Err(failure.clone())),
                build_firewall_status(vec![
                    (FirewallProfile::Domain, Err(failure.clone())),
                    (FirewallProfile::Private, Err(failure.clone())),
                    (FirewallProfile::Public, Err(failure)),
                ]),
            )
        }
    };

    NativeProtectionStatus {
        checked_at: now_rfc3339(),
        observation: observation_for(defender.state, firewall.state),
        defender,
        firewall,
        limitations: LIMITATIONS.iter().map(|value| value.to_string()).collect(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderFailureState {
    Unavailable,
    AccessDenied,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderFailure {
    state: ProviderFailureState,
    code: String,
    message: String,
}

impl ProviderFailure {
    fn from_hresult(prefix: &str, label: &str, hresult: u32) -> Self {
        let (suffix, state) = match hresult {
            0x8007_0005 | 0x8004_1003 => ("ACCESS-DENIED", ProviderFailureState::AccessDenied),
            0x8004_100e | 0x8004_1010 | 0x8004_0154 | 0x8007_0424 => {
                ("UNAVAILABLE", ProviderFailureState::Unavailable)
            }
            _ => ("QUERY-FAILED", ProviderFailureState::Error),
        };
        let message = match state {
            ProviderFailureState::AccessDenied => {
                format!("没有权限读取{label}状态（HRESULT 0x{hresult:08X}）。当前状态未知。")
            }
            ProviderFailureState::Unavailable => {
                format!("{label}提供程序不可用（HRESULT 0x{hresult:08X}）。当前状态未知。")
            }
            ProviderFailureState::Error => {
                format!("无法读取{label}状态（HRESULT 0x{hresult:08X}）。当前状态未知。")
            }
        };
        Self {
            state,
            code: format!("{prefix}-{suffix}"),
            message,
        }
    }

    fn unavailable(code: &str, message: impl Into<String>) -> Self {
        Self {
            state: ProviderFailureState::Unavailable,
            code: code.to_string(),
            message: message.into(),
        }
    }

    fn provider_state(&self) -> NativeProtectionProviderState {
        match self.state {
            ProviderFailureState::Unavailable => NativeProtectionProviderState::Unavailable,
            ProviderFailureState::AccessDenied => NativeProtectionProviderState::AccessDenied,
            ProviderFailureState::Error => NativeProtectionProviderState::Error,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RawDefenderValues {
    antivirus_enabled: Option<bool>,
    real_time_protection_enabled: Option<bool>,
    am_service_enabled: Option<bool>,
    signature_version: Option<String>,
    signature_age_days: Option<u32>,
}

fn issue(code: &str, message: impl Into<String>) -> NativeProtectionIssue {
    NativeProtectionIssue {
        code: code.to_string(),
        message: message.into(),
    }
}

fn profile_label(profile: FirewallProfile) -> &'static str {
    match profile {
        FirewallProfile::Domain => "域网络",
        FirewallProfile::Private => "专用网络",
        FirewallProfile::Public => "公用网络",
    }
}

fn build_defender_status(result: Result<RawDefenderValues, ProviderFailure>) -> DefenderStatus {
    match result {
        Err(failure) => DefenderStatus {
            state: failure.provider_state(),
            antivirus_enabled: None,
            real_time_protection_enabled: None,
            am_service_enabled: None,
            signature_version: None,
            signature_age_days: None,
            issues: vec![issue(&failure.code, failure.message)],
        },
        Ok(raw) => {
            let mut issues = Vec::new();
            for (field, missing) in [
                ("AntivirusEnabled", raw.antivirus_enabled.is_none()),
                (
                    "RealTimeProtectionEnabled",
                    raw.real_time_protection_enabled.is_none(),
                ),
                ("AMServiceEnabled", raw.am_service_enabled.is_none()),
                ("AntivirusSignatureVersion", raw.signature_version.is_none()),
                ("AntivirusSignatureAge", raw.signature_age_days.is_none()),
            ] {
                if missing {
                    issues.push(issue(
                        "DEFENDER-FIELD-MISSING",
                        format!("Defender 状态缺少 {field} 字段；该项保持未知。"),
                    ));
                }
            }
            DefenderStatus {
                state: if issues.is_empty() {
                    NativeProtectionProviderState::Observed
                } else {
                    NativeProtectionProviderState::Partial
                },
                antivirus_enabled: raw.antivirus_enabled,
                real_time_protection_enabled: raw.real_time_protection_enabled,
                am_service_enabled: raw.am_service_enabled,
                signature_version: raw.signature_version,
                signature_age_days: raw.signature_age_days,
                issues,
            }
        }
    }
}

fn build_firewall_status(
    results: Vec<(FirewallProfile, Result<bool, ProviderFailure>)>,
) -> FirewallStatus {
    let mut profiles = Vec::with_capacity(3);
    let mut issues = Vec::new();
    let mut failures = Vec::new();
    let mut observed = 0usize;

    for profile in [
        FirewallProfile::Domain,
        FirewallProfile::Private,
        FirewallProfile::Public,
    ] {
        match results
            .iter()
            .find(|(candidate, _)| *candidate == profile)
            .map(|(_, result)| result)
        {
            Some(Ok(enabled)) => {
                observed += 1;
                profiles.push(FirewallProfileStatus {
                    profile,
                    enabled: Some(*enabled),
                });
            }
            Some(Err(failure)) => {
                issues.push(issue(&failure.code, failure.message.clone()));
                failures.push(failure.clone());
                profiles.push(FirewallProfileStatus {
                    profile,
                    enabled: None,
                });
            }
            None => {
                let failure = ProviderFailure::unavailable(
                    "FIREWALL-PROFILE-UNAVAILABLE",
                    format!(
                        "{}防火墙配置文件未返回状态；该项保持未知。",
                        profile_label(profile)
                    ),
                );
                issues.push(issue(&failure.code, failure.message.clone()));
                failures.push(failure);
                profiles.push(FirewallProfileStatus {
                    profile,
                    enabled: None,
                });
            }
        }
    }

    let state = if observed == 3 {
        NativeProtectionProviderState::Observed
    } else if observed > 0 {
        NativeProtectionProviderState::Partial
    } else if !failures.is_empty()
        && failures
            .iter()
            .all(|failure| failure.state == ProviderFailureState::AccessDenied)
    {
        NativeProtectionProviderState::AccessDenied
    } else if !failures.is_empty()
        && failures
            .iter()
            .all(|failure| failure.state == ProviderFailureState::Unavailable)
    {
        NativeProtectionProviderState::Unavailable
    } else {
        NativeProtectionProviderState::Error
    };

    FirewallStatus {
        state,
        profiles,
        issues,
    }
}

fn observation_for(
    defender: NativeProtectionProviderState,
    firewall: NativeProtectionProviderState,
) -> NativeProtectionObservation {
    use NativeProtectionProviderState::{AccessDenied, Error, Observed, Partial, Unavailable};
    match (defender, firewall) {
        (Observed, Observed) => NativeProtectionObservation::Complete,
        (Unavailable | AccessDenied | Error, Unavailable | AccessDenied | Error) => {
            NativeProtectionObservation::Unavailable
        }
        (Observed | Partial | Unavailable | AccessDenied | Error, _) => {
            NativeProtectionObservation::Partial
        }
    }
}

#[cfg(windows)]
mod windows_query {
    use super::{
        MAX_SIGNATURE_VERSION_CHARS, ProviderFailure, RawDefenderValues, WMI_NEXT_TIMEOUT_MS,
    };
    use crate::model::FirewallProfile;
    use windows::Win32::Foundation::VARIANT_BOOL;
    use windows::Win32::NetworkManagement::WindowsFirewall::{
        INetFwPolicy2, NET_FW_PROFILE2_DOMAIN, NET_FW_PROFILE2_PRIVATE, NET_FW_PROFILE2_PUBLIC,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, CoCreateInstance, CoSetProxyBlanket,
        EOLE_AUTHENTICATION_CAPABILITIES, RPC_C_AUTHN_LEVEL_DEFAULT, RPC_C_IMP_LEVEL_IMPERSONATE,
    };
    use windows::Win32::System::Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_DEFAULT};
    use windows::Win32::System::Variant::{
        VARIANT, VT_BOOL, VT_BSTR, VT_I2, VT_I4, VT_I8, VT_NULL, VT_UI2, VT_UI4, VT_UI8,
        VariantClear,
    };
    use windows::Win32::System::Wmi::{
        IEnumWbemClassObject, IWbemClassObject, IWbemLocator, WBEM_FLAG_FORWARD_ONLY,
        WBEM_FLAG_RETURN_IMMEDIATELY, WbemLocator,
    };
    use windows::core::{BSTR, GUID, IUnknown, PCWSTR};

    const CLSID_NETFW_POLICY2: GUID = GUID::from_u128(0xe2b3c97f_6ae1_41ac_817a_f6f92166d7dd);

    pub(super) fn query_defender() -> Result<RawDefenderValues, ProviderFailure> {
        unsafe { query_defender_inner() }
    }

    unsafe fn query_defender_inner() -> Result<RawDefenderValues, ProviderFailure> {
        let locator: IWbemLocator =
            unsafe { CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER) }
                .map_err(|error| failure("DEFENDER", "Defender", error))?;

        let services = unsafe {
            locator.ConnectServer(
                &BSTR::from(r"ROOT\Microsoft\Windows\Defender"),
                &BSTR::new(),
                &BSTR::new(),
                &BSTR::new(),
                0,
                &BSTR::new(),
                None,
            )
        }
        .map_err(|error| failure("DEFENDER", "Defender", error))?;

        unsafe {
            CoSetProxyBlanket(
                &services,
                RPC_C_AUTHN_WINNT,
                RPC_C_AUTHZ_DEFAULT,
                PCWSTR::null(),
                RPC_C_AUTHN_LEVEL_DEFAULT,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOLE_AUTHENTICATION_CAPABILITIES(0),
            )
        }
        .map_err(|error| failure("DEFENDER", "Defender", error))?;

        let query = unsafe {
            services.ExecQuery(
                &BSTR::from("WQL"),
                &BSTR::from(
                    "SELECT AntivirusEnabled, RealTimeProtectionEnabled, AMServiceEnabled, \
                     AntivirusSignatureVersion, AntivirusSignatureAge FROM MSFT_MpComputerStatus",
                ),
                WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                None,
            )
        }
        .map_err(|error| failure("DEFENDER", "Defender", error))?;

        let Some(object) = unsafe { first_object(&query) }? else {
            return Err(ProviderFailure::unavailable(
                "DEFENDER-NOT-OBSERVED",
                "Defender WMI 提供程序未返回状态实例；可能由第三方安全软件替代或已被禁用。当前状态未知。",
            ));
        };

        Ok(RawDefenderValues {
            antivirus_enabled: read_bool(&object, windows::core::w!("AntivirusEnabled")),
            real_time_protection_enabled: read_bool(
                &object,
                windows::core::w!("RealTimeProtectionEnabled"),
            ),
            am_service_enabled: read_bool(&object, windows::core::w!("AMServiceEnabled")),
            signature_version: read_string(&object, windows::core::w!("AntivirusSignatureVersion")),
            signature_age_days: read_u32(&object, windows::core::w!("AntivirusSignatureAge")),
        })
    }

    unsafe fn first_object(
        query: &IEnumWbemClassObject,
    ) -> Result<Option<IWbemClassObject>, ProviderFailure> {
        let mut objects: [Option<IWbemClassObject>; 1] = [None];
        let mut returned = 0u32;
        let result = unsafe { query.Next(WMI_NEXT_TIMEOUT_MS, &mut objects, &mut returned) };
        if result.is_err() {
            return Err(ProviderFailure::from_hresult(
                "DEFENDER",
                "Defender",
                result.0 as u32,
            ));
        }
        if returned == 0 {
            return Ok(None);
        }
        Ok(objects[0].take())
    }

    pub(super) fn query_firewall() -> Vec<(FirewallProfile, Result<bool, ProviderFailure>)> {
        let policy: INetFwPolicy2 = match unsafe {
            CoCreateInstance(
                &CLSID_NETFW_POLICY2,
                None::<&IUnknown>,
                CLSCTX_INPROC_SERVER,
            )
        } {
            Ok(policy) => policy,
            Err(error) => {
                let failure = failure("FIREWALL", "Windows 防火墙", error);
                return all_profiles_failed(failure);
            }
        };

        [
            (FirewallProfile::Domain, NET_FW_PROFILE2_DOMAIN),
            (FirewallProfile::Private, NET_FW_PROFILE2_PRIVATE),
            (FirewallProfile::Public, NET_FW_PROFILE2_PUBLIC),
        ]
        .into_iter()
        .map(|(profile, profile_type)| {
            let enabled = unsafe { policy.get_FirewallEnabled(profile_type) }
                .map(|VARIANT_BOOL(value)| value != 0)
                .map_err(|error| failure("FIREWALL", "Windows 防火墙", error));
            (profile, enabled)
        })
        .collect()
    }

    fn all_profiles_failed(
        failure: ProviderFailure,
    ) -> Vec<(FirewallProfile, Result<bool, ProviderFailure>)> {
        [
            FirewallProfile::Domain,
            FirewallProfile::Private,
            FirewallProfile::Public,
        ]
        .into_iter()
        .map(|profile| (profile, Err(failure.clone())))
        .collect()
    }

    fn failure(prefix: &str, label: &str, error: windows::core::Error) -> ProviderFailure {
        ProviderFailure::from_hresult(prefix, label, error.code().0 as u32)
    }

    struct OwnedVariant(VARIANT);

    impl Drop for OwnedVariant {
        fn drop(&mut self) {
            unsafe {
                let _ = VariantClear(&mut self.0);
            }
        }
    }

    fn read_variant(object: &IWbemClassObject, name: PCWSTR) -> Option<OwnedVariant> {
        // Construct the guard before the call so a partially initialized VARIANT is cleared even
        // when WMI returns an error after writing a value.
        let mut value = OwnedVariant(VARIANT::default());
        let result = unsafe { object.Get(name, 0, &mut value.0, None, None) };
        if result.is_ok() { Some(value) } else { None }
    }

    fn read_bool(object: &IWbemClassObject, name: PCWSTR) -> Option<bool> {
        let value = read_variant(object, name)?;
        let inner = unsafe { &value.0.Anonymous.Anonymous };
        match inner.vt {
            VT_BOOL => Some(unsafe { inner.Anonymous.boolVal.0 != 0 }),
            VT_I4 => Some(unsafe { inner.Anonymous.lVal != 0 }),
            VT_UI4 => Some(unsafe { inner.Anonymous.ulVal != 0 }),
            VT_NULL => None,
            _ => None,
        }
    }

    fn read_u32(object: &IWbemClassObject, name: PCWSTR) -> Option<u32> {
        let value = read_variant(object, name)?;
        let inner = unsafe { &value.0.Anonymous.Anonymous };
        match inner.vt {
            VT_I4 => u32::try_from(unsafe { inner.Anonymous.lVal }).ok(),
            VT_UI4 => Some(unsafe { inner.Anonymous.ulVal }),
            VT_I2 => u32::try_from(unsafe { inner.Anonymous.iVal }).ok(),
            VT_UI2 => Some(u32::from(unsafe { inner.Anonymous.uiVal })),
            VT_I8 => u32::try_from(unsafe { inner.Anonymous.llVal }).ok(),
            VT_UI8 => u32::try_from(unsafe { inner.Anonymous.ullVal }).ok(),
            VT_BSTR => unsafe { inner.Anonymous.bstrVal.to_string() }.parse().ok(),
            _ => None,
        }
    }

    fn read_string(object: &IWbemClassObject, name: PCWSTR) -> Option<String> {
        let value = read_variant(object, name)?;
        let inner = unsafe { &value.0.Anonymous.Anonymous };
        if inner.vt != VT_BSTR {
            return None;
        }
        let text = unsafe { inner.Anonymous.bstrVal.to_string() };
        sanitize_text(&text)
    }

    fn sanitize_text(value: &str) -> Option<String> {
        let cleaned: String = value
            .trim()
            .chars()
            .filter(|character| !character.is_control())
            .take(MAX_SIGNATURE_VERSION_CHARS)
            .collect();
        if cleaned.is_empty() {
            None
        } else {
            Some(cleaned)
        }
    }
}

#[cfg(windows)]
use windows_query::{query_defender, query_firewall};

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_defender() -> RawDefenderValues {
        RawDefenderValues {
            antivirus_enabled: Some(true),
            real_time_protection_enabled: Some(true),
            am_service_enabled: Some(true),
            signature_version: Some("1.2.3.4".to_string()),
            signature_age_days: Some(0),
        }
    }

    #[test]
    fn complete_defender_values_are_observed() {
        let status = build_defender_status(Ok(complete_defender()));

        assert_eq!(status.state, NativeProtectionProviderState::Observed);
        assert_eq!(status.antivirus_enabled, Some(true));
        assert_eq!(status.real_time_protection_enabled, Some(true));
        assert_eq!(status.am_service_enabled, Some(true));
        assert_eq!(status.signature_version.as_deref(), Some("1.2.3.4"));
        assert_eq!(status.signature_age_days, Some(0));
        assert!(status.issues.is_empty());
    }

    #[test]
    fn missing_defender_field_is_partial_and_stays_unknown() {
        let mut raw = complete_defender();
        raw.am_service_enabled = None;

        let status = build_defender_status(Ok(raw));

        assert_eq!(status.state, NativeProtectionProviderState::Partial);
        assert_eq!(status.am_service_enabled, None);
        assert_eq!(status.antivirus_enabled, Some(true));
        assert!(
            status
                .issues
                .iter()
                .any(|issue| issue.code == "DEFENDER-FIELD-MISSING"
                    && issue.message.contains("AMServiceEnabled"))
        );
    }

    #[test]
    fn defender_access_denied_never_reports_enabled() {
        let status = build_defender_status(Err(ProviderFailure::from_hresult(
            "DEFENDER",
            "Defender",
            0x8007_0005,
        )));

        assert_eq!(status.state, NativeProtectionProviderState::AccessDenied);
        assert_eq!(status.antivirus_enabled, None);
        assert_eq!(status.real_time_protection_enabled, None);
        assert_eq!(status.am_service_enabled, None);
        assert_eq!(status.signature_version, None);
        assert_eq!(status.signature_age_days, None);
        assert_eq!(status.issues.len(), 1);
        assert_eq!(status.issues[0].code, "DEFENDER-ACCESS-DENIED");
        assert!(!status.issues[0].message.contains("已启用"));
    }

    #[test]
    fn defender_without_an_instance_is_unavailable() {
        let status = build_defender_status(Err(ProviderFailure::unavailable(
            "DEFENDER-NOT-OBSERVED",
            "Defender WMI 提供程序未返回状态实例。",
        )));

        assert_eq!(status.state, NativeProtectionProviderState::Unavailable);
        assert_eq!(status.antivirus_enabled, None);
        assert_eq!(status.issues[0].code, "DEFENDER-NOT-OBSERVED");
    }

    #[test]
    fn all_firewall_profiles_observed() {
        let status = build_firewall_status(vec![
            (FirewallProfile::Domain, Ok(true)),
            (FirewallProfile::Private, Ok(true)),
            (FirewallProfile::Public, Ok(false)),
        ]);

        assert_eq!(status.state, NativeProtectionProviderState::Observed);
        assert_eq!(status.profiles.len(), 3);
        assert_eq!(status.profiles[0].enabled, Some(true));
        assert_eq!(status.profiles[1].enabled, Some(true));
        assert_eq!(status.profiles[2].enabled, Some(false));
        assert!(status.issues.is_empty());
    }

    #[test]
    fn one_failed_firewall_profile_is_partial_and_keeps_the_others() {
        let status = build_firewall_status(vec![
            (FirewallProfile::Domain, Ok(true)),
            (
                FirewallProfile::Private,
                Err(ProviderFailure::from_hresult(
                    "FIREWALL",
                    "Windows 防火墙",
                    0x8007_0005,
                )),
            ),
            (FirewallProfile::Public, Ok(true)),
        ]);

        assert_eq!(status.state, NativeProtectionProviderState::Partial);
        assert_eq!(status.profiles[1].enabled, None);
        assert_eq!(status.profiles[0].enabled, Some(true));
        assert!(
            status
                .issues
                .iter()
                .any(|issue| issue.code == "FIREWALL-ACCESS-DENIED")
        );
    }

    #[test]
    fn all_firewall_profiles_denied_is_access_denied() {
        let status = build_firewall_status(vec![
            (
                FirewallProfile::Domain,
                Err(ProviderFailure::from_hresult(
                    "FIREWALL",
                    "Windows 防火墙",
                    0x8007_0005,
                )),
            ),
            (
                FirewallProfile::Private,
                Err(ProviderFailure::from_hresult(
                    "FIREWALL",
                    "Windows 防火墙",
                    0x8007_0005,
                )),
            ),
            (
                FirewallProfile::Public,
                Err(ProviderFailure::from_hresult(
                    "FIREWALL",
                    "Windows 防火墙",
                    0x8007_0005,
                )),
            ),
        ]);

        assert_eq!(status.state, NativeProtectionProviderState::AccessDenied);
        assert!(
            status
                .profiles
                .iter()
                .all(|profile| profile.enabled.is_none())
        );
        assert_eq!(status.issues.len(), 3);
    }

    #[test]
    fn missing_firewall_profile_is_not_treated_as_enabled() {
        let status = build_firewall_status(vec![
            (FirewallProfile::Domain, Ok(true)),
            (FirewallProfile::Private, Ok(true)),
        ]);

        assert_eq!(status.state, NativeProtectionProviderState::Partial);
        assert_eq!(status.profiles[2].profile, FirewallProfile::Public);
        assert_eq!(status.profiles[2].enabled, None);
        assert!(
            status
                .issues
                .iter()
                .any(|issue| issue.code == "FIREWALL-PROFILE-UNAVAILABLE")
        );
    }

    #[test]
    fn observation_is_complete_only_when_both_providers_are_fully_observed() {
        use NativeProtectionProviderState::{AccessDenied, Error, Observed, Partial, Unavailable};

        assert_eq!(
            observation_for(Observed, Observed),
            NativeProtectionObservation::Complete
        );
        assert_eq!(
            observation_for(Observed, Partial),
            NativeProtectionObservation::Partial
        );
        assert_eq!(
            observation_for(Partial, Error),
            NativeProtectionObservation::Partial
        );
        assert_eq!(
            observation_for(AccessDenied, Error),
            NativeProtectionObservation::Unavailable
        );
        assert_eq!(
            observation_for(Unavailable, AccessDenied),
            NativeProtectionObservation::Unavailable
        );
    }

    #[test]
    fn provider_failure_mapping_preserves_unknown_and_access_denied() {
        let access_denied = ProviderFailure::from_hresult("FIREWALL", "防火墙", 0x8004_1003);
        assert_eq!(
            access_denied.provider_state(),
            NativeProtectionProviderState::AccessDenied
        );
        assert_eq!(access_denied.code, "FIREWALL-ACCESS-DENIED");

        let unavailable = ProviderFailure::from_hresult("DEFENDER", "Defender", 0x8004_100e);
        assert_eq!(
            unavailable.provider_state(),
            NativeProtectionProviderState::Unavailable
        );
        assert_eq!(unavailable.code, "DEFENDER-UNAVAILABLE");

        let other = ProviderFailure::from_hresult("DEFENDER", "Defender", 0x8004_2000);
        assert_eq!(other.provider_state(), NativeProtectionProviderState::Error);
        assert_eq!(other.code, "DEFENDER-QUERY-FAILED");
    }

    #[test]
    fn status_serializes_with_camel_case_fields_and_no_protected_verdict() {
        let status = NativeProtectionStatus {
            checked_at: "2026-11-01T00:00:00Z".to_string(),
            observation: observation_for(
                NativeProtectionProviderState::Observed,
                NativeProtectionProviderState::Observed,
            ),
            defender: build_defender_status(Ok(complete_defender())),
            firewall: build_firewall_status(vec![
                (FirewallProfile::Domain, Ok(true)),
                (FirewallProfile::Private, Ok(true)),
                (FirewallProfile::Public, Ok(false)),
            ]),
            limitations: LIMITATIONS.iter().map(|value| value.to_string()).collect(),
        };

        let value = serde_json::to_value(&status).expect("status serializes");
        assert_eq!(value["observation"], "complete");
        assert_eq!(value["defender"]["realTimeProtectionEnabled"], true);
        assert_eq!(value["defender"]["signatureAgeDays"], 0);
        assert_eq!(value["firewall"]["profiles"][0]["profile"], "domain");
        assert_eq!(value["firewall"]["profiles"][2]["enabled"], false);
        assert!(value.get("protected").is_none());
        assert!(value.get("overallState").is_none());
    }

    #[test]
    fn status_limitations_never_claim_protection() {
        let status = build_defender_status(Err(ProviderFailure::unavailable(
            "DEFENDER-NOT-OBSERVED",
            "not observed",
        )));
        assert_eq!(status.antivirus_enabled, None);

        let text = LIMITATIONS.join(" ");
        assert!(text.contains("不修改"));
        assert!(text.contains("保持未知"));
        assert!(text.contains("不会推断整体保护正常"));
    }
}
