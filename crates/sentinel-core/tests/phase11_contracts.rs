use std::io;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use sentinel_core::limits::{
    MAX_REGISTRY_STRING_CHARS, MAX_SOFTWARE_ITEMS_PER_VIEW, MAX_SOFTWARE_ITEMS_TOTAL, SourceBudget,
    bound_text,
};
use sentinel_core::model::{
    FileHashState, PathState, RegistryValueState, RiskLevel, RiskScope, SoftwareClassification,
    StartupPathResolution, StartupSource,
};
use sentinel_core::rules::{
    AffinityFacts, SoftwareFacts, StartupFacts, assess_affinity, assess_process_overall,
    assess_software, assess_startup,
};
use sentinel_core::signature::{sha256_hex, verify_file};
use sentinel_core::software::{SoftwareClassificationFacts, classify_software};
use sentinel_core::util::{
    ParsedCommandPath, ProcessIdentity, parse_startup_command, path_state_from_io_error,
    system_time_to_rfc3339,
};

#[test]
fn affinity_success_is_scoped_and_never_becomes_an_overall_process_clean_result() {
    let affinity = assess_affinity(AffinityFacts {
        restricted: Some(false),
        process_mask: Some(0b1111),
        system_mask: Some(0b1111),
    });
    let overall = assess_process_overall();

    assert_eq!(affinity.scope, RiskScope::AffinityConfiguration);
    assert_eq!(affinity.level, RiskLevel::NoClearIssue);
    assert!(affinity.summary.contains("亲和性范围内未发现异常"));
    assert_eq!(overall.scope, RiskScope::ProcessOverall);
    assert_eq!(overall.level, RiskLevel::InsufficientEvidence);
    assert!(overall.summary.contains("整体"));
    assert!(overall.blind_spots.iter().any(|item| item.contains("行为")));
}

#[test]
fn no_clear_issue_is_explicitly_limited_to_the_checked_scope() {
    let risk = assess_software(SoftwareFacts {
        uninstall_command_present: true,
        uninstall_command_known: true,
        install_location_state: PathState::Present,
        classification: SoftwareClassification::UserSoftware,
    });

    assert_eq!(risk.level, RiskLevel::NoClearIssue);
    assert_eq!(risk.scope, RiskScope::SoftwareMetadata);
    assert!(risk.summary.contains("检查范围"));
    assert!(!risk.summary.contains("安全"));
}

#[test]
fn unreadable_uninstall_entry_stays_unknown_instead_of_a_configuration_anomaly() {
    let risk = assess_software(SoftwareFacts {
        uninstall_command_present: false,
        uninstall_command_known: false,
        install_location_state: PathState::Present,
        classification: SoftwareClassification::UserSoftware,
    });

    assert_eq!(risk.level, RiskLevel::InsufficientEvidence);
    assert!(risk.rule_ids.iter().any(|rule| rule == "INV-005"));
    assert!(!risk.rule_ids.iter().any(|rule| rule == "INV-001"));
    assert!(risk.summary.contains("读取失败"));
}

#[test]
fn system_component_entries_are_classified_and_not_treated_as_ordinary_software() {
    let facts = SoftwareClassificationFacts {
        system_component: Some(1),
        no_remove: None,
        parent_key_name: None,
        release_type: None,
        system_component_state: RegistryValueState::Present,
        no_remove_state: RegistryValueState::Missing,
        parent_key_name_state: RegistryValueState::Missing,
        release_type_state: RegistryValueState::Missing,
    };

    assert_eq!(
        classify_software(facts),
        SoftwareClassification::SystemComponent
    );

    let risk = assess_software(SoftwareFacts {
        uninstall_command_present: false,
        uninstall_command_known: true,
        install_location_state: PathState::Unknown,
        classification: SoftwareClassification::SystemComponent,
    });

    assert_eq!(risk.level, RiskLevel::NoClearIssue);
    assert!(!risk.rule_ids.iter().any(|rule| rule == "INV-001"));
    assert!(risk.summary.contains("系统组件") || risk.summary.contains("组件类"));
    assert_eq!(risk.scope, RiskScope::SoftwareMetadata);
}

#[test]
fn no_remove_entries_are_treated_as_non_user_software() {
    let facts = SoftwareClassificationFacts {
        system_component: None,
        no_remove: Some(1),
        parent_key_name: None,
        release_type: None,
        system_component_state: RegistryValueState::Missing,
        no_remove_state: RegistryValueState::Present,
        parent_key_name_state: RegistryValueState::Missing,
        release_type_state: RegistryValueState::Missing,
    };

    assert_eq!(
        classify_software(facts),
        SoftwareClassification::SystemComponent
    );
}

#[test]
fn parent_key_name_without_more_evidence_is_unknown_not_runtime_guessed() {
    let facts = SoftwareClassificationFacts {
        system_component: None,
        no_remove: None,
        parent_key_name: Some("ParentProduct"),
        release_type: None,
        system_component_state: RegistryValueState::Missing,
        no_remove_state: RegistryValueState::Missing,
        parent_key_name_state: RegistryValueState::Present,
        release_type_state: RegistryValueState::Missing,
    };
    let classification = classify_software(facts);

    assert_eq!(classification, SoftwareClassification::Unknown);

    let risk = assess_software(SoftwareFacts {
        uninstall_command_present: false,
        uninstall_command_known: true,
        install_location_state: PathState::Missing,
        classification,
    });

    assert_eq!(risk.level, RiskLevel::InsufficientEvidence);
    assert!(!risk.rule_ids.iter().any(|rule| rule == "INV-001"));
    assert!(!risk.rule_ids.iter().any(|rule| rule == "INV-002"));
}

#[test]
fn generic_release_type_is_unknown_but_explicit_driver_release_is_classified() {
    let generic = SoftwareClassificationFacts {
        system_component: None,
        no_remove: None,
        parent_key_name: None,
        release_type: Some("Security Update"),
        system_component_state: RegistryValueState::Missing,
        no_remove_state: RegistryValueState::Missing,
        parent_key_name_state: RegistryValueState::Missing,
        release_type_state: RegistryValueState::Present,
    };
    let driver = SoftwareClassificationFacts {
        release_type: Some("Driver"),
        ..generic
    };

    assert_eq!(classify_software(generic), SoftwareClassification::Unknown);
    assert_eq!(
        classify_software(driver),
        SoftwareClassification::DriverOrHardwareComponent
    );
}

#[test]
fn ordinary_metadata_stays_a_user_software_entry() {
    let facts = SoftwareClassificationFacts {
        system_component: None,
        no_remove: None,
        parent_key_name: None,
        release_type: None,
        system_component_state: RegistryValueState::Missing,
        no_remove_state: RegistryValueState::Missing,
        parent_key_name_state: RegistryValueState::Missing,
        release_type_state: RegistryValueState::Missing,
    };

    assert_eq!(
        classify_software(facts),
        SoftwareClassification::UserSoftware
    );
}

#[test]
fn inaccessible_user_software_path_is_insufficient_evidence_not_missing() {
    let risk = assess_software(SoftwareFacts {
        uninstall_command_present: true,
        uninstall_command_known: true,
        install_location_state: PathState::AccessDenied,
        classification: SoftwareClassification::UserSoftware,
    });

    assert_eq!(risk.level, RiskLevel::InsufficientEvidence);
    assert!(risk.rule_ids.iter().any(|rule| rule == "INV-003"));
    assert!(
        risk.evidence
            .iter()
            .any(|item| item.statement.contains("访问被拒绝"))
    );
    assert!(!risk.summary.contains("不存在"));
}

#[test]
fn unquoted_ambiguous_startup_path_is_not_missing_file_evidence() {
    let risk = assess_startup(StartupFacts {
        target_path: None,
        target_state: PathState::Unknown,
        path_resolution: StartupPathResolution::AmbiguousUnquoted,
    });

    assert_eq!(risk.level, RiskLevel::InsufficientEvidence);
    assert!(risk.rule_ids.iter().any(|rule| rule == "STARTUP-005"));
    assert!(!risk.rule_ids.iter().any(|rule| rule == "STARTUP-002"));
    assert!(
        risk.evidence
            .iter()
            .any(|item| item.code == "STARTUP-UNQUOTED-PATH")
    );
}

#[test]
fn startup_access_denied_is_insufficient_evidence() {
    let risk = assess_startup(StartupFacts {
        target_path: Some(r"C:\Program Files\Example\app.exe"),
        target_state: PathState::AccessDenied,
        path_resolution: StartupPathResolution::QuotedExecutable,
    });

    assert_eq!(risk.level, RiskLevel::InsufficientEvidence);
    assert!(risk.rule_ids.iter().any(|rule| rule == "STARTUP-006"));
    assert!(
        risk.evidence
            .iter()
            .any(|item| item.statement.contains("访问被拒绝"))
    );
    assert!(!risk.summary.contains("不存在"));
}

#[test]
fn bounded_text_truncates_unicode_on_character_boundaries() {
    let ascii = bound_text(
        &"a".repeat(MAX_REGISTRY_STRING_CHARS + 10),
        MAX_REGISTRY_STRING_CHARS,
    );
    let unicode = bound_text(
        &"界".repeat(MAX_REGISTRY_STRING_CHARS + 3),
        MAX_REGISTRY_STRING_CHARS,
    );
    let exact = bound_text("short", 99);

    assert!(ascii.truncated);
    assert!(unicode.truncated);
    assert_eq!(ascii.value.chars().count(), MAX_REGISTRY_STRING_CHARS);
    assert_eq!(unicode.value.chars().count(), MAX_REGISTRY_STRING_CHARS);
    assert!(ascii.value.ends_with('…'));
    assert!(unicode.value.ends_with('…'));
    assert!(!exact.truncated);
    assert_eq!(exact.value, "short");
}

#[test]
fn simulated_registry_rows_stop_at_per_view_and_total_limits() {
    let mut budget = SourceBudget::new(3, 5);
    let mut processed = 0;

    for _ in 0..10 {
        if !budget.can_process("hklm:Uninstall:registry64") {
            break;
        }
        budget.record_processed("hklm:Uninstall:registry64");
        budget.record_item();
        processed += 1;
    }

    assert_eq!(processed, 3);
    assert!(!budget.can_process("hklm:Uninstall:registry64"));
    assert!(budget.total_items() < 5);

    let mut total_budget = SourceBudget::new(MAX_SOFTWARE_ITEMS_PER_VIEW, MAX_SOFTWARE_ITEMS_TOTAL);
    let mut added = 0;
    for index in 0..(MAX_SOFTWARE_ITEMS_TOTAL + 100) {
        let source = if index % 2 == 0 { "view-a" } else { "view-b" };
        if !total_budget.can_process(source) {
            break;
        }
        total_budget.record_processed(source);
        total_budget.record_item();
        added += 1;
    }
    assert_eq!(added, MAX_SOFTWARE_ITEMS_TOTAL);
    assert!(!total_budget.can_process("view-c"));
}

#[test]
fn path_error_mapping_distinguishes_missing_access_denied_and_unknown() {
    let missing = io::Error::from(io::ErrorKind::NotFound);
    let denied = io::Error::from_raw_os_error(5);
    let unknown = io::Error::from_raw_os_error(1234);

    assert_eq!(path_state_from_io_error(&missing), PathState::Missing);
    assert_eq!(path_state_from_io_error(&denied), PathState::AccessDenied);
    assert_eq!(path_state_from_io_error(&unknown), PathState::Unknown);
}

#[test]
fn startup_command_parser_handles_quoted_paths_and_flags_ambiguous_unquoted_paths() {
    assert_eq!(
        parse_startup_command(r#""C:\Program Files\Example\app.exe" --silent"#),
        ParsedCommandPath::Quoted(r"C:\Program Files\Example\app.exe".to_string())
    );
    assert_eq!(
        parse_startup_command(r"C:\Windows\System32\cmd.exe /c start"),
        ParsedCommandPath::Unquoted(r"C:\Windows\System32\cmd.exe".to_string())
    );
    assert_eq!(
        parse_startup_command(r"C:\Program Files\Example\app.exe -run"),
        ParsedCommandPath::AmbiguousUnquoted {
            candidate: Some(r"C:\Program Files\Example\app.exe".to_string())
        }
    );
    assert_eq!(parse_startup_command("   "), ParsedCommandPath::Unresolved);
    assert_eq!(
        parse_startup_command(r#""C:\unterminated"#),
        ParsedCommandPath::Unresolved
    );
}

#[test]
fn process_identity_tokens_round_trip_and_detect_mismatch() {
    let identity = ProcessIdentity::from_ticks(133_000_000_000_000_000).expect("valid identity");
    let token = identity.token();
    let parsed = ProcessIdentity::parse(&token).expect("round trip");

    assert_eq!(parsed, identity);
    assert!(identity.matches(133_000_000_000_000_000));
    assert!(!identity.matches(133_000_000_000_000_001));
    assert!(ProcessIdentity::parse("not-a-token").is_none());
    assert!(ProcessIdentity::from_ticks(0).is_none());
}

#[test]
fn sha256_evidence_is_reproducible() {
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn remote_signature_checks_never_report_a_disk_hash() {
    let info = verify_file(Path::new(r"\\server\share\payload.exe"));

    assert_eq!(
        info.state,
        sentinel_core::model::SignatureState::Unavailable
    );
    assert_eq!(info.file_evidence.hash_state, FileHashState::Unavailable);
    assert!(info.file_evidence.checked_path.is_none());
    assert!(!info.network_used);
}

#[test]
fn system_time_formatting_is_available_for_file_evidence() {
    assert_eq!(
        system_time_to_rfc3339(UNIX_EPOCH),
        Some("1970-01-01T00:00:00Z".to_string())
    );
}

#[test]
fn risk_scope_and_classification_values_serialize_stably() {
    assert_eq!(
        serde_json::to_value(RiskScope::ProcessOverall).unwrap(),
        "processOverall"
    );
    assert_eq!(
        serde_json::to_value(SoftwareClassification::SystemComponent).unwrap(),
        "systemComponent"
    );
    assert_eq!(
        serde_json::to_value(StartupPathResolution::AmbiguousUnquoted).unwrap(),
        "ambiguousUnquoted"
    );
    assert_eq!(
        serde_json::to_value(StartupSource::RegistryRunOnce).unwrap(),
        "registryRunOnce"
    );
}

#[test]
fn startup_normal_case_still_has_scoped_language() {
    let risk = assess_startup(StartupFacts {
        target_path: Some(r"C:\Program Files\Example\app.exe"),
        target_state: PathState::Present,
        path_resolution: StartupPathResolution::QuotedExecutable,
    });

    assert_eq!(risk.level, RiskLevel::NoClearIssue);
    assert_eq!(risk.scope, RiskScope::StartupMetadata);
    assert!(risk.summary.contains("检查范围"));
}

#[test]
fn cpu_percent_contract_remains_stable() {
    let percent = sentinel_core::util::calculate_cpu_percent(
        Duration::from_millis(500),
        Duration::from_secs(1),
        20,
    )
    .expect("valid sample");
    assert!((percent - 2.5).abs() < 0.001);
}
