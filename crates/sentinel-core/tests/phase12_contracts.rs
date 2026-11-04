use sentinel_core::limits::{
    MAX_PROCESS_ITEMS, MAX_PROCESS_NAME_CHARS, MAX_PROCESS_PATH_CHARS, bound_text,
};
use sentinel_core::model::{RegistryValueState, SoftwareClassification};
use sentinel_core::registry::{RegistryValueKind, parse_dword, parse_string};
use sentinel_core::software::{SoftwareClassificationFacts, classify_software};
use sentinel_core::startup::is_startup_folder_metadata;

const _: () = {
    assert!(MAX_PROCESS_ITEMS > 0);
    assert!(MAX_PROCESS_NAME_CHARS > 0);
    assert!(MAX_PROCESS_PATH_CHARS > MAX_PROCESS_NAME_CHARS);
};

#[test]
fn bounded_process_limits_are_explicit() {
    assert!(
        bound_text(
            &"p".repeat(MAX_PROCESS_PATH_CHARS + 1),
            MAX_PROCESS_PATH_CHARS
        )
        .truncated
    );
}

#[test]
fn missing_registry_classification_fields_are_explicit_and_do_not_default_to_unknown() {
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
fn dword_values_are_parsed_only_from_exact_dword_bytes() {
    let valid = parse_dword(Some((RegistryValueKind::Dword, &[1, 0, 0, 0])));
    let string_type = parse_dword(Some((RegistryValueKind::String, b"1\0")));
    let malformed = parse_dword(Some((RegistryValueKind::Dword, &[1, 0, 0])));
    let missing = parse_dword(None);

    assert_eq!(valid.value, Some(1));
    assert_eq!(valid.state, RegistryValueState::Present);
    assert_eq!(string_type.value, None);
    assert_eq!(string_type.state, RegistryValueState::TypeMismatch);
    assert_eq!(malformed.value, None);
    assert_eq!(malformed.state, RegistryValueState::Invalid);
    assert_eq!(missing.value, None);
    assert_eq!(missing.state, RegistryValueState::Missing);
}

fn utf16le(value: &str) -> Vec<u8> {
    value.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

#[test]
fn string_type_mismatch_and_invalid_utf16_are_visible_states() {
    let normal_bytes = utf16le("%SystemRoot%\\x\0");
    let normal = parse_string(Some((RegistryValueKind::ExpandString, &normal_bytes)), 256);
    let wrong_type = parse_string(Some((RegistryValueKind::Dword, &[1, 0, 0, 0])), 256);
    let invalid_utf16 = parse_string(Some((RegistryValueKind::String, &[0x00, 0xD8])), 256);

    assert_eq!(normal.value.as_deref(), Some("%SystemRoot%\\x"));
    assert_eq!(normal.state, RegistryValueState::Present);
    assert_eq!(wrong_type.value, None);
    assert_eq!(wrong_type.state, RegistryValueState::TypeMismatch);
    assert_eq!(invalid_utf16.value, None);
    assert_eq!(invalid_utf16.state, RegistryValueState::Invalid);
}

#[test]
fn string_parsing_reports_truncation_separately_from_validity() {
    let raw = utf16le(&"界".repeat(300));
    let parsed = parse_string(Some((RegistryValueKind::String, &raw)), 16);

    assert_eq!(parsed.state, RegistryValueState::Present);
    assert!(parsed.truncated);
    assert_eq!(parsed.value.expect("value").chars().count(), 16);
}

#[test]
fn unreadable_component_field_prevents_silent_user_software_classification() {
    let facts = SoftwareClassificationFacts {
        system_component: None,
        no_remove: None,
        parent_key_name: None,
        release_type: None,
        system_component_state: RegistryValueState::TypeMismatch,
        no_remove_state: RegistryValueState::Missing,
        parent_key_name_state: RegistryValueState::Missing,
        release_type_state: RegistryValueState::Missing,
    };

    assert_eq!(classify_software(facts), SoftwareClassification::Unknown);
}

#[test]
fn invalid_release_type_field_also_prevents_a_user_software_guess() {
    let facts = SoftwareClassificationFacts {
        system_component: None,
        no_remove: None,
        parent_key_name: None,
        release_type: None,
        system_component_state: RegistryValueState::Missing,
        no_remove_state: RegistryValueState::Missing,
        parent_key_name_state: RegistryValueState::Missing,
        release_type_state: RegistryValueState::Invalid,
    };

    assert_eq!(classify_software(facts), SoftwareClassification::Unknown);
}

#[test]
fn a_strong_system_flag_wins_over_an_unrelated_field_parse_failure() {
    let facts = SoftwareClassificationFacts {
        system_component: Some(1),
        no_remove: None,
        parent_key_name: None,
        release_type: None,
        system_component_state: RegistryValueState::Present,
        no_remove_state: RegistryValueState::TypeMismatch,
        parent_key_name_state: RegistryValueState::Missing,
        release_type_state: RegistryValueState::Missing,
    };

    assert_eq!(
        classify_software(facts),
        SoftwareClassification::SystemComponent
    );
}

#[test]
fn desktop_ini_and_thumbs_db_are_not_startup_programs() {
    assert!(is_startup_folder_metadata("desktop.ini"));
    assert!(is_startup_folder_metadata("DESKTOP.INI"));
    assert!(is_startup_folder_metadata("Thumbs.db"));
    assert!(!is_startup_folder_metadata("fictional-helper.exe"));
}
