use serde::Serialize;

/// Health states available to the UI.
///
/// `Protected` is intentionally absent until a real sensor-health model exists. Adding it back
/// requires the Phase 2 health model and must not be derived from the absence of findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProtectionState {
    RiskDetected,
    Degraded,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SensorState {
    Active,
    Degraded,
    NotImplemented,
    NotChecked,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RiskLevel {
    ConfirmedThreat,
    HighRiskSuspicious,
    PotentialUnwanted,
    UserUnwanted,
    ConfigurationAnomaly,
    NoClearIssue,
    InsufficientEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    High,
    Medium,
    Low,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallScope {
    Machine,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RegistryView {
    Registry32,
    Registry64,
    /// HKEY_CURRENT_USER locations that Windows does not redirect by WOW64 registry view.
    Shared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PathState {
    Present,
    Missing,
    AccessDenied,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SoftwareClassification {
    UserSoftware,
    SystemComponent,
    DriverOrHardwareComponent,
    RuntimeOrSharedComponent,
    SecuritySoftware,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StartupPathResolution {
    QuotedExecutable,
    UnquotedExecutable,
    AmbiguousUnquoted,
    StartupFolderFile,
    ShortcutTargetResolved,
    ShortcutTargetUnresolved,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StartupTargetKind {
    RegistryCommand,
    StartupFolderFile,
    Shortcut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StartupValueType {
    RegSz,
    RegExpandSz,
    RegMultiSz,
    RegDword,
    RegQword,
    RegBinary,
    RegNone,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RiskScope {
    ProcessOverall,
    AffinityConfiguration,
    SoftwareMetadata,
    StartupMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileHashState {
    Computed,
    SkippedTooLarge,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RegistryValueState {
    Present,
    Missing,
    TypeMismatch,
    Invalid,
    ReadError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StartupSource {
    RegistryRun,
    RegistryRunOnce,
    StartupFolder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StartupApprovalState {
    NotChecked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SignatureState {
    Valid,
    Unsigned,
    Invalid,
    Untrusted,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessPathAccess {
    Available,
    Denied,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InspectionState {
    Complete,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeepDefenseState {
    NotImplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub code: String,
    pub statement: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskAssessment {
    pub scope: RiskScope,
    pub level: RiskLevel,
    pub confidence: Confidence,
    pub rule_ids: Vec<String>,
    pub summary: String,
    pub evidence: Vec<Evidence>,
    pub blind_spots: Vec<String>,
    pub recommendation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataIssue {
    pub code: String,
    pub message: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionSensor {
    pub id: String,
    pub name: String,
    pub state: SensorState,
    pub description: String,
    pub coverage: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadOnlyCheck {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepDefenseStatus {
    pub state: DeepDefenseState,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeProtectionProviderState {
    Observed,
    Partial,
    Unavailable,
    AccessDenied,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeProtectionObservation {
    Complete,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FirewallProfile {
    Domain,
    Private,
    Public,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeProtectionIssue {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefenderStatus {
    pub state: NativeProtectionProviderState,
    pub antivirus_enabled: Option<bool>,
    pub real_time_protection_enabled: Option<bool>,
    pub am_service_enabled: Option<bool>,
    pub signature_version: Option<String>,
    pub signature_age_days: Option<u32>,
    pub issues: Vec<NativeProtectionIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirewallProfileStatus {
    pub profile: FirewallProfile,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirewallStatus {
    pub state: NativeProtectionProviderState,
    pub profiles: Vec<FirewallProfileStatus>,
    pub issues: Vec<NativeProtectionIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeProtectionStatus {
    pub checked_at: String,
    pub observation: NativeProtectionObservation,
    pub defender: DefenderStatus,
    pub firewall: FirewallStatus,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionOverview {
    pub generated_at: String,
    pub app_version: String,
    pub overall_state: ProtectionState,
    pub headline: String,
    pub explanation: String,
    pub sensors: Vec<ProtectionSensor>,
    pub read_only_checks: Vec<ReadOnlyCheck>,
    pub deep_defense: DeepDefenseStatus,
    pub boundaries: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareSourceStatus {
    pub id: String,
    pub scope: InstallScope,
    pub registry_view: RegistryView,
    pub registry_key: String,
    pub observation: InventoryObservation,
    /// `Some(false)` means the Uninstall root was confirmed absent; `None` means
    /// absence itself could not be verified.
    pub exists: Option<bool>,
    pub item_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSoftware {
    pub id: String,
    pub display_name: String,
    pub version: Option<String>,
    pub version_state: RegistryValueState,
    pub publisher: Option<String>,
    pub publisher_state: RegistryValueState,
    pub install_location: Option<String>,
    pub install_location_state: PathState,
    pub install_location_value_state: RegistryValueState,
    pub install_date: Option<String>,
    pub install_date_state: RegistryValueState,
    /// Parsed static icon path clue. The raw DisplayIcon value is never returned.
    pub display_icon_path: Option<String>,
    pub display_icon_state: RegistryValueState,
    pub windows_installer: Option<u32>,
    pub windows_installer_state: RegistryValueState,
    /// Presence only; the uninstall command text is never read into the result.
    pub uninstall_command_present: bool,
    pub quiet_uninstall_command_present: bool,
    pub scope: InstallScope,
    pub registry_view: RegistryView,
    pub registry_key: String,
    pub system_component: Option<u32>,
    pub system_component_state: RegistryValueState,
    pub no_remove: Option<u32>,
    pub no_remove_state: RegistryValueState,
    pub parent_key_name: Option<String>,
    pub parent_key_name_state: RegistryValueState,
    pub release_type: Option<String>,
    pub release_type_state: RegistryValueState,
    pub classification: SoftwareClassification,
    pub observation: InventoryObservation,
    pub issues: Vec<DataIssue>,
    pub risk: RiskAssessment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareScanResult {
    pub checked_at: String,
    pub observation: InventoryObservation,
    pub sources: Vec<SoftwareSourceStatus>,
    pub items: Vec<InstalledSoftware>,
    pub issues: Vec<DataIssue>,
    pub limitations: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupSourceStatus {
    pub id: String,
    pub source: StartupSource,
    pub scope: InstallScope,
    pub registry_view: Option<RegistryView>,
    /// Registry key or environment-qualified Startup folder. Expanded user-profile
    /// paths are deliberately not used as the display location.
    pub location: String,
    pub observation: InventoryObservation,
    /// `Some(false)` means the location was confirmed absent; `None` means the
    /// absence itself could not be verified (for example, access denied).
    pub exists: Option<bool>,
    pub item_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupItem {
    pub id: String,
    pub name: String,
    pub value_name: Option<String>,
    pub value_type: Option<StartupValueType>,
    pub raw_value_readable: bool,
    pub source: StartupSource,
    pub scope: InstallScope,
    pub registry_view: Option<RegistryView>,
    pub registry_key: Option<String>,
    pub target_kind: StartupTargetKind,
    pub target_path: Option<String>,
    pub target_candidate: Option<String>,
    pub target_state: PathState,
    pub path_resolution: StartupPathResolution,
    /// Arguments are never returned, only whether a non-empty argument string existed.
    pub shortcut_arguments_present: Option<bool>,
    /// Working directory is never returned, only whether a non-empty value existed.
    pub shortcut_working_directory_present: Option<bool>,
    pub approval_state: StartupApprovalState,
    pub observation: InventoryObservation,
    pub issues: Vec<DataIssue>,
    pub risk: RiskAssessment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupScanResult {
    pub checked_at: String,
    pub observation: InventoryObservation,
    pub sources: Vec<StartupSourceStatus>,
    pub items: Vec<StartupItem>,
    pub issues: Vec<DataIssue>,
    pub limitations: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InventoryObservation {
    Observed,
    Partial,
    Unavailable,
    AccessDenied,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceState {
    Stopped,
    StartPending,
    StopPending,
    Running,
    ContinuePending,
    PausePending,
    Paused,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceStartType {
    Boot,
    System,
    Auto,
    Demand,
    Disabled,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceType {
    KernelDriver,
    FileSystemDriver,
    Adapter,
    RecognizerDriver,
    OwnProcess,
    ShareProcess,
    InteractiveProcess,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSummary {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub state: ServiceState,
    pub start_type: ServiceStartType,
    pub service_types: Vec<ServiceType>,
    pub account: Option<String>,
    pub executable_path: Option<String>,
    pub observation: InventoryObservation,
    pub issues: Vec<DataIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInventoryResult {
    pub checked_at: String,
    pub observation: InventoryObservation,
    pub items: Vec<ServiceSummary>,
    pub issues: Vec<DataIssue>,
    pub limitations: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ScheduledTaskState {
    Unknown,
    Disabled,
    Queued,
    Ready,
    Running,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ScheduledTaskActionType {
    Exec,
    ComHandler,
    Email,
    ShowMessage,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTaskAction {
    pub action_type: ScheduledTaskActionType,
    pub executable_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTaskSummary {
    pub id: String,
    pub name: String,
    pub path: String,
    pub state: ScheduledTaskState,
    pub enabled: Option<bool>,
    pub author: Option<String>,
    pub run_as: Option<String>,
    pub actions: Vec<ScheduledTaskAction>,
    pub actions_truncated: bool,
    pub observation: InventoryObservation,
    pub issues: Vec<DataIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTaskInventoryResult {
    pub checked_at: String,
    pub observation: InventoryObservation,
    pub items: Vec<ScheduledTaskSummary>,
    pub issues: Vec<DataIssue>,
    pub limitations: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSummary {
    pub pid: u32,
    pub parent_pid: u32,
    pub name: String,
    pub executable_path: Option<String>,
    pub path_access: ProcessPathAccess,
    pub parent_name: Option<String>,
    /// Opaque, high-precision creation-time token used to bind a later inspection to this process.
    pub identity: Option<String>,
    pub start_time: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessListResult {
    pub checked_at: String,
    pub processes: Vec<ProcessSummary>,
    pub issues: Vec<DataIssue>,
    pub limitations: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AssociationSource {
    SoftwareInventory,
    ProcessSnapshot,
    ServiceInventory,
    ScheduledTaskInventory,
    StartupInventory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AssociationTargetKind {
    Process,
    Service,
    ScheduledTask,
    StartupItem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AssociationRelationType {
    SoftwareProcess,
    SoftwareService,
    SoftwareScheduledTask,
    SoftwareStartupItem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AssociationEvidenceType {
    DisplayIconPath,
    InstallLocationPath,
    ExecutableFileName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AssociationMatchMethod {
    ExactPath,
    ContainedInInstallLocation,
    SameFileNameDifferentDirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AssociationCertainty {
    Exact,
    Strong,
    Weak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SoftwareAssociationState {
    Linked,
    WeakOnly,
    NoMatch,
    InsufficientEvidence,
    SourceUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssociationSourceStatus {
    pub id: String,
    pub source: AssociationSource,
    pub observation: InventoryObservation,
    pub item_count: usize,
    /// Number of items that supplied a comparable local path for this association pass.
    pub addressable_count: usize,
    /// Number of items whose path was absent, relative, remote, or otherwise not comparable.
    pub unresolved_path_count: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssociationTarget {
    pub kind: AssociationTargetKind,
    pub id: String,
    pub name: String,
    /// Human-readable target context (PID, service name, task path, startup source).
    pub detail: String,
    /// Redacted static path evidence; command lines and arguments are never returned.
    pub path: Option<String>,
    pub observation: InventoryObservation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRelation {
    pub id: String,
    pub software_id: String,
    pub software_name: String,
    pub target: AssociationTarget,
    pub relation_type: AssociationRelationType,
    pub evidence_type: AssociationEvidenceType,
    pub match_method: AssociationMatchMethod,
    /// Redacted software path clue used for this match.
    pub software_evidence_path: Option<String>,
    /// Redacted target path or file name used for this match; never a full command line.
    pub evidence_value: String,
    pub certainty: AssociationCertainty,
    pub observation: InventoryObservation,
    pub ambiguous: bool,
    pub candidate_software_count: usize,
    pub issues: Vec<DataIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareAssociationSummary {
    pub software_id: String,
    pub software_name: String,
    pub state: SoftwareAssociationState,
    pub path_evidence_count: usize,
    pub relation_count: usize,
    pub strong_relation_count: usize,
    pub weak_relation_count: usize,
    pub issues: Vec<DataIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetAssociationResult {
    pub checked_at: String,
    pub observation: InventoryObservation,
    pub sources: Vec<AssociationSourceStatus>,
    pub software: Vec<SoftwareAssociationSummary>,
    pub relations: Vec<AssetRelation>,
    pub issues: Vec<DataIssue>,
    pub limitations: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEvidence {
    pub checked_path: Option<String>,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<String>,
    pub sha256: Option<String>,
    pub hash_state: FileHashState,
    pub note: String,
}

impl FileEvidence {
    pub fn unavailable(note: impl Into<String>) -> Self {
        Self {
            checked_path: None,
            size_bytes: None,
            modified_at: None,
            sha256: None,
            hash_state: FileHashState::Unavailable,
            note: note.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureInfo {
    pub state: SignatureState,
    pub signer_name: Option<String>,
    pub detail: String,
    pub verification_scope: String,
    pub memory_verified: bool,
    pub network_used: bool,
    pub revocation_checked: bool,
    pub file_evidence: FileEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuMask {
    pub mask_hex: String,
    pub cpu_indices: Vec<u32>,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinitySourceTrace {
    pub available: bool,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityResult {
    pub process_mask: CpuMask,
    pub system_mask: CpuMask,
    pub restricted: bool,
    pub comparison_scope: String,
    pub source_trace: AffinitySourceTrace,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuUsageSample {
    pub percent_of_total_capacity: f64,
    pub sample_duration_ms: u64,
    pub sampled_at: String,
    pub method: String,
    pub caveat: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AffinityInspection {
    pub checked_at: String,
    pub pid: u32,
    pub identity: String,
    pub name: String,
    pub executable_path: String,
    pub start_time: Option<String>,
    pub inspection_state: InspectionState,
    pub signature: SignatureInfo,
    pub affinity: Option<AffinityResult>,
    pub cpu_usage: Option<CpuUsageSample>,
    pub issues: Vec<DataIssue>,
    pub affinity_risk: RiskAssessment,
    pub overall_risk: RiskAssessment,
    pub limitations: Vec<String>,
}
