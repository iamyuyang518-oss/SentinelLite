/**
 * IPC contracts shared with the Rust backend.
 *
 * Rust serializes enums in camelCase and struct fields in camelCase. These types are
 * intentionally explicit instead of generated so that any contract change is reviewed in both
 * places. All timestamps are RFC 3339 strings in UTC.
 */

// `protected` is intentionally absent until a real sensor-health model exists.
export type ProtectionState = "riskDetected" | "degraded" | "unknown";

export type SensorState =
  | "active"
  | "degraded"
  | "notImplemented"
  | "notChecked"
  | "unknown";

/**
 * The live sensor IPC contract.
 *
 * `SensorStatusReport` is camelCase at the top level. The nested `SensorStatusDetails`
 * intentionally stays snake_case because it is the versioned IPC payload serialized by
 * `sentinel-ipc`; renaming it here would silently break the command boundary.
 */
export type SensorLinkState =
  | "running"
  | "degraded"
  | "stopped"
  | "unavailable"
  | "communicationFailed";

export type SensorLifecycleState =
  | "not_started"
  | "starting"
  | "active"
  | "degraded"
  | "failed"
  | "stopped"
  | "unavailable";

export type SensorPermissionState = "unknown" | "granted" | "limited" | "denied";

export interface SensorStatusDetails {
  sensor_state: SensorLifecycleState;
  permission_state: SensorPermissionState;
  events_received: number;
  events_processed: number;
  events_dropped: number;
  events_lost: number | null;
  realtime_buffers_lost: number | null;
  decode_errors: number;
  last_event_at: string | null;
  uptime_ms: number;
  health_ok: boolean;
  last_error: string | null;
}

export interface SensorStatusReport {
  linkState: SensorLinkState;
  sensor: SensorStatusDetails | null;
  error: string | null;
}

export type RiskLevel =
  | "confirmedThreat"
  | "highRiskSuspicious"
  | "potentialUnwanted"
  | "userUnwanted"
  | "configurationAnomaly"
  | "noClearIssue"
  | "insufficientEvidence";

export type Confidence = "high" | "medium" | "low" | "unknown";

export type RiskScope =
  | "processOverall"
  | "affinityConfiguration"
  | "softwareMetadata"
  | "startupMetadata";

export type PathState = "present" | "missing" | "accessDenied" | "unknown";

export type SoftwareClassification =
  | "userSoftware"
  | "systemComponent"
  | "driverOrHardwareComponent"
  | "runtimeOrSharedComponent"
  | "securitySoftware"
  | "unknown";

export type StartupPathResolution =
  | "quotedExecutable"
  | "unquotedExecutable"
  | "ambiguousUnquoted"
  | "startupFolderFile"
  | "shortcutTargetResolved"
  | "shortcutTargetUnresolved"
  | "unresolved";

export type StartupTargetKind =
  | "registryCommand"
  | "startupFolderFile"
  | "shortcut";

export type StartupValueType =
  | "regSz"
  | "regExpandSz"
  | "regMultiSz"
  | "regDword"
  | "regQword"
  | "regBinary"
  | "regNone"
  | "other";

export type FileHashState = "computed" | "skippedTooLarge" | "unavailable";

export type RegistryValueState =
  | "present"
  | "missing"
  | "typeMismatch"
  | "invalid"
  | "readError";

export type InstallScope = "machine" | "user";

export type RegistryView = "registry32" | "registry64" | "shared";

export type StartupSource = "registryRun" | "registryRunOnce" | "startupFolder";

export type SignatureState =
  | "valid"
  | "unsigned"
  | "invalid"
  | "untrusted"
  | "unavailable";

export type ProcessPathAccess = "available" | "denied" | "unavailable";

export interface Evidence {
  code: string;
  statement: string;
  source: string;
}

export interface RiskAssessment {
  scope: RiskScope;
  level: RiskLevel;
  confidence: Confidence;
  ruleIds: string[];
  summary: string;
  evidence: Evidence[];
  blindSpots: string[];
  recommendation: string;
}

export interface DataIssue {
  code: string;
  message: string;
  source: string;
}

export interface ProtectionSensor {
  id: string;
  name: string;
  state: SensorState;
  description: string;
  coverage: string[];
  limitations: string[];
}

export interface ReadOnlyCheck {
  id: "softwareInventory" | "startupItems" | "affinityInspection";
  name: string;
  description: string;
}

export interface DeepDefenseStatus {
  state: "notImplemented";
  explanation: string;
}

export interface ProtectionOverview {
  generatedAt: string;
  appVersion: string;
  overallState: ProtectionState;
  headline: string;
  explanation: string;
  sensors: ProtectionSensor[];
  readOnlyChecks: ReadOnlyCheck[];
  deepDefense: DeepDefenseStatus;
  boundaries: string[];
}

export type NativeProtectionProviderState =
  | "observed"
  | "partial"
  | "unavailable"
  | "accessDenied"
  | "error";

export type NativeProtectionObservation = "complete" | "partial" | "unavailable";

export type FirewallProfile = "domain" | "private" | "public";

export interface NativeProtectionIssue {
  code: string;
  message: string;
}

export interface DefenderStatus {
  state: NativeProtectionProviderState;
  antivirusEnabled: boolean | null;
  realTimeProtectionEnabled: boolean | null;
  amServiceEnabled: boolean | null;
  signatureVersion: string | null;
  signatureAgeDays: number | null;
  issues: NativeProtectionIssue[];
}

export interface FirewallProfileStatus {
  profile: FirewallProfile;
  enabled: boolean | null;
}

export interface FirewallStatus {
  state: NativeProtectionProviderState;
  profiles: FirewallProfileStatus[];
  issues: NativeProtectionIssue[];
}

export interface NativeProtectionStatus {
  checkedAt: string;
  observation: NativeProtectionObservation;
  defender: DefenderStatus;
  firewall: FirewallStatus;
  limitations: string[];
}

export type InventoryObservation =
  | "observed"
  | "partial"
  | "unavailable"
  | "accessDenied"
  | "error";

export type ServiceState =
  | "stopped"
  | "startPending"
  | "stopPending"
  | "running"
  | "continuePending"
  | "pausePending"
  | "paused"
  | "unknown";

export type ServiceStartType =
  | "boot"
  | "system"
  | "auto"
  | "demand"
  | "disabled"
  | "unknown";

export type ServiceType =
  | "kernelDriver"
  | "fileSystemDriver"
  | "adapter"
  | "recognizerDriver"
  | "ownProcess"
  | "shareProcess"
  | "interactiveProcess"
  | "other";

export interface ServiceSummary {
  id: string;
  name: string;
  displayName: string;
  state: ServiceState;
  startType: ServiceStartType;
  serviceTypes: ServiceType[];
  account: string | null;
  executablePath: string | null;
  observation: InventoryObservation;
  issues: DataIssue[];
}

export interface ServiceInventoryResult {
  checkedAt: string;
  observation: InventoryObservation;
  items: ServiceSummary[];
  issues: DataIssue[];
  limitations: string[];
  truncated: boolean;
}

export type ScheduledTaskState =
  | "unknown"
  | "disabled"
  | "queued"
  | "ready"
  | "running";

export type ScheduledTaskActionType =
  | "exec"
  | "comHandler"
  | "email"
  | "showMessage"
  | "unknown";

export interface ScheduledTaskAction {
  actionType: ScheduledTaskActionType;
  executablePath: string | null;
}

export interface ScheduledTaskSummary {
  id: string;
  name: string;
  path: string;
  state: ScheduledTaskState;
  enabled: boolean | null;
  author: string | null;
  runAs: string | null;
  actions: ScheduledTaskAction[];
  actionsTruncated: boolean;
  observation: InventoryObservation;
  issues: DataIssue[];
}

export interface ScheduledTaskInventoryResult {
  checkedAt: string;
  observation: InventoryObservation;
  items: ScheduledTaskSummary[];
  issues: DataIssue[];
  limitations: string[];
  truncated: boolean;
}

export interface SoftwareSourceStatus {
  id: string;
  scope: InstallScope;
  registryView: RegistryView;
  registryKey: string;
  observation: InventoryObservation;
  exists: boolean | null;
  itemCount: number;
}

export interface InstalledSoftware {
  id: string;
  displayName: string;
  version: string | null;
  versionState: RegistryValueState;
  publisher: string | null;
  publisherState: RegistryValueState;
  installLocation: string | null;
  installLocationState: PathState;
  installLocationValueState: RegistryValueState;
  installDate: string | null;
  installDateState: RegistryValueState;
  displayIconPath: string | null;
  displayIconState: RegistryValueState;
  windowsInstaller: number | null;
  windowsInstallerState: RegistryValueState;
  uninstallCommandPresent: boolean;
  quietUninstallCommandPresent: boolean;
  scope: InstallScope;
  registryView: RegistryView;
  registryKey: string;
  systemComponent: number | null;
  systemComponentState: RegistryValueState;
  noRemove: number | null;
  noRemoveState: RegistryValueState;
  parentKeyName: string | null;
  parentKeyNameState: RegistryValueState;
  releaseType: string | null;
  releaseTypeState: RegistryValueState;
  classification: SoftwareClassification;
  observation: InventoryObservation;
  issues: DataIssue[];
  risk: RiskAssessment;
}

export interface SoftwareScanResult {
  checkedAt: string;
  observation: InventoryObservation;
  sources: SoftwareSourceStatus[];
  items: InstalledSoftware[];
  issues: DataIssue[];
  limitations: string[];
  truncated: boolean;
}

export interface StartupSourceStatus {
  id: string;
  source: StartupSource;
  scope: InstallScope;
  registryView: RegistryView | null;
  location: string;
  observation: InventoryObservation;
  exists: boolean | null;
  itemCount: number;
}

export interface StartupItem {
  id: string;
  name: string;
  valueName: string | null;
  valueType: StartupValueType | null;
  rawValueReadable: boolean;
  source: StartupSource;
  scope: InstallScope;
  registryView: RegistryView | null;
  registryKey: string | null;
  targetKind: StartupTargetKind;
  targetPath: string | null;
  targetCandidate: string | null;
  targetState: PathState;
  pathResolution: StartupPathResolution;
  shortcutArgumentsPresent: boolean | null;
  shortcutWorkingDirectoryPresent: boolean | null;
  approvalState: "notChecked";
  observation: InventoryObservation;
  issues: DataIssue[];
  risk: RiskAssessment;
}

export interface StartupScanResult {
  checkedAt: string;
  observation: InventoryObservation;
  sources: StartupSourceStatus[];
  items: StartupItem[];
  issues: DataIssue[];
  limitations: string[];
  truncated: boolean;
}

export interface ProcessSummary {
  pid: number;
  parentPid: number;
  name: string;
  executablePath: string | null;
  pathAccess: ProcessPathAccess;
  parentName: string | null;
  identity: string | null;
  startTime: string | null;
}

export interface ProcessListResult {
  checkedAt: string;
  processes: ProcessSummary[];
  issues: DataIssue[];
  limitations: string[];
  truncated: boolean;
}

export type AssociationSource =
  | "softwareInventory"
  | "processSnapshot"
  | "serviceInventory"
  | "scheduledTaskInventory"
  | "startupInventory";

export type AssociationTargetKind = "process" | "service" | "scheduledTask" | "startupItem";

export type AssociationRelationType =
  | "softwareProcess"
  | "softwareService"
  | "softwareScheduledTask"
  | "softwareStartupItem";

export type AssociationEvidenceType =
  | "displayIconPath"
  | "installLocationPath"
  | "executableFileName";

export type AssociationMatchMethod =
  | "exactPath"
  | "containedInInstallLocation"
  | "sameFileNameDifferentDirectory";

export type AssociationCertainty = "exact" | "strong" | "weak";

export type SoftwareAssociationState =
  | "linked"
  | "weakOnly"
  | "noMatch"
  | "insufficientEvidence"
  | "sourceUnavailable";

export interface AssociationSourceStatus {
  id: string;
  source: AssociationSource;
  observation: InventoryObservation;
  itemCount: number;
  addressableCount: number;
  unresolvedPathCount: number;
  truncated: boolean;
}

export interface AssociationTarget {
  kind: AssociationTargetKind;
  id: string;
  name: string;
  detail: string;
  path: string | null;
  observation: InventoryObservation;
}

export interface AssetRelation {
  id: string;
  softwareId: string;
  softwareName: string;
  target: AssociationTarget;
  relationType: AssociationRelationType;
  evidenceType: AssociationEvidenceType;
  matchMethod: AssociationMatchMethod;
  softwareEvidencePath: string | null;
  evidenceValue: string;
  certainty: AssociationCertainty;
  observation: InventoryObservation;
  ambiguous: boolean;
  candidateSoftwareCount: number;
  issues: DataIssue[];
}

export interface SoftwareAssociationSummary {
  softwareId: string;
  softwareName: string;
  state: SoftwareAssociationState;
  pathEvidenceCount: number;
  relationCount: number;
  strongRelationCount: number;
  weakRelationCount: number;
  issues: DataIssue[];
}

export interface AssetAssociationResult {
  checkedAt: string;
  observation: InventoryObservation;
  sources: AssociationSourceStatus[];
  software: SoftwareAssociationSummary[];
  relations: AssetRelation[];
  issues: DataIssue[];
  limitations: string[];
  truncated: boolean;
}

export interface FileEvidence {
  checkedPath: string | null;
  sizeBytes: number | null;
  modifiedAt: string | null;
  sha256: string | null;
  hashState: FileHashState;
  note: string;
}

export interface SignatureInfo {
  state: SignatureState;
  signerName: string | null;
  detail: string;
  verificationScope: "fileOnDiskOnly";
  memoryVerified: false;
  networkUsed: false;
  revocationChecked: false;
  fileEvidence: FileEvidence;
}

export interface CpuMask {
  maskHex: string;
  cpuIndices: number[];
  count: number;
}

export interface AffinityResult {
  processMask: CpuMask;
  systemMask: CpuMask;
  restricted: boolean;
  comparisonScope: "currentProcessorGroup";
  sourceTrace: {
    available: false;
    explanation: string;
  };
}

export interface CpuUsageSample {
  percentOfTotalCapacity: number;
  sampleDurationMs: number;
  sampledAt: string;
  method: string;
  caveat: string;
}

/* Phase 2.5-2.7 process event contracts.
 *
 * Event payloads use the same snake_case field names as the versioned IPC and
 * SQLite records. Only Tauri-specific wrappers are camelCase. These are
 * intentionally explicit so a wire change is reviewed on both sides.
 */

export type FieldStateWire = "present" | "partial" | "missing";
export type ProcessEventTypeWire = "started" | "stopped";

export type DataIssueWire =
  | "missing_parent_pid"
  | "missing_image_name"
  | "partial_image_name"
  | "image_name_truncated"
  | "missing_event_timestamp"
  | "event_timestamp_unrepresentable"
  | "missing_receive_time"
  | "missing_normalize_time"
  | "indeterminate_process_create_time"
  | "process_start_time_unrepresentable";

export interface ProcessIdentityWire {
  pid: number;
  create_time: number;
}

export interface DataCompletenessWire {
  identity_certain: boolean;
  event_time_known: boolean;
  received_time_known: boolean;
  normalized_time_known: boolean;
  parent_pid_available: boolean;
  image_name: FieldStateWire;
  etw_sequence_available: boolean;
  issues: DataIssueWire[];
}

export interface EtwEventInfoWire {
  event_id: number;
  version: number;
  provider_sequence: number | null;
  timestamp_100ns: number | null;
  occurred_at: string | null;
}

export interface UnpairedReasonWire {
  reason:
    | "stop_without_observed_start"
    | "orphan_stop_not_retained"
    | "duplicate_start"
    | "duplicate_stop"
    | "identity_mismatch"
    | "start_not_retained";
  first_event_id?: string;
  open_create_time?: number;
}

export type PairingStatusWire =
  | {
      state: "paired";
      start_event_id: string;
      stop_event_id: string;
      order: "in_order" | "out_of_order";
      sequence_match: boolean | null;
    }
  | { state: "waiting" }
  | { state: "unpaired"; reason: UnpairedReasonWire }
  | {
      state: "indeterminate";
      reason: "missing_process_create_time" | "unsupported_event_type";
    };

export interface ProcessEventWire {
  event_id: string;
  sequence: number;
  session_id: string;
  event_type: ProcessEventTypeWire;
  source: "microsoft_windows_kernel_process";
  identity: ProcessIdentityWire;
  identity_certainty: "exact" | "indeterminate";
  parent_pid: number | null;
  image_name: string | null;
  etw: EtwEventInfoWire;
  process_started_at: string | null;
  received_at: string | null;
  normalized_at: string | null;
  completeness: DataCompletenessWire;
  pairing: PairingStatusWire;
}

export interface InvalidProcessEventWire {
  event_id: string;
  sequence: number;
  session_id: string;
  etw_event_id: number;
  etw_version: number;
  payload_len: number;
  error: string;
  etw_timestamp_100ns: number | null;
  occurred_at: string | null;
  received_at: string | null;
  normalized_at: string | null;
}

export interface PairingRevisionWire {
  session_id: string;
  stop_event_id: string;
  start_event_id: string;
  sequence_match: boolean | null;
  reason: string;
  normalized_at: string | null;
}

export interface PairingSnapshotWire {
  paired: number;
  unpaired_starts: number;
  orphan_stops: number;
  duplicate_starts: number;
  duplicate_stops: number;
  pid_reuse_detected: number;
  sequence_mismatches: number;
  start_overflow: number;
  orphan_stop_overflow: number;
  identity_mismatched_stops: number;
  out_of_order_pairs: number;
  indeterminate_identities: number;
  closed_identity_evictions: number;
  pending_orphan_stops: number;
  starts_seen: number;
  stops_seen: number;
  observed_start_image_names: string[];
  observed_start_image_names_truncated: boolean;
}

export interface SessionEndStatsWire {
  events_lost: number | null;
  realtime_buffers_lost: number | null;
  pipeline_capacity: number;
  pipeline_received: number;
  pipeline_enqueued: number;
  pipeline_consumed: number;
  pipeline_normalized: number;
  pipeline_invalid: number;
  pipeline_dropped_queue_full: number;
  pipeline_dropped_consumer_gone: number;
  pipeline_filtered_ignored: number;
  pipeline_backlog: number;
  pipeline_peak_backlog: number;
  pipeline_backlog_at_shutdown_timeout: number | null;
  pipeline_shutdown_timed_out: boolean;
  pipeline_sink_errors: number;
}

export interface SessionEndWire {
  session_id: string;
  ended_at: string | null;
  reason: "completed" | "interrupted" | "error" | "shutdown";
  error: string | null;
  open_start_events: string[];
  pairing: PairingSnapshotWire;
  final_stats: SessionEndStatsWire | null;
}

export type PipelineRecordWire =
  | ({ record: "process_event" } & ProcessEventWire)
  | ({ record: "invalid_event" } & InvalidProcessEventWire)
  | ({ record: "pairing_revision" } & PairingRevisionWire)
  | ({ record: "session_end" } & SessionEndWire);

export interface EventEnvelopeWire {
  cursor: number;
  record: PipelineRecordWire;
}

export interface EventBatchWire {
  session_id: string;
  oldest_cursor: number;
  latest_cursor: number;
  next_cursor: number;
  cursor_expired: boolean;
  missed_records: number;
  cursor_ahead: boolean;
  evictions: number;
  session_closed: boolean;
  has_more: boolean;
  records: EventEnvelopeWire[];
}

export interface SensorEventsReport {
  linkState: SensorLinkState;
  batch: EventBatchWire | null;
  error: string | null;
}

export interface StoredEventWire {
  event_id: string;
  session_id: string;
  sequence: number;
  event_type: ProcessEventTypeWire;
  pid: number;
  create_time: number;
  image_name: string | null;
  event_time: string | null;
  occurred_at: string | null;
  received_at: string | null;
  normalized_at: string | null;
  pairing_state: "paired" | "waiting" | "unpaired" | "indeterminate";
  pairing_order: "in_order" | "out_of_order" | null;
  paired_event_id: string | null;
  record: ProcessEventWire;
}

export interface EventQueryResultWire {
  total: number;
  limit: number;
  offset: number;
  events: StoredEventWire[];
}

export interface EventDetailWire {
  event: StoredEventWire;
  paired_event: StoredEventWire | null;
  revisions: PairingRevisionWire[];
  session_end: SessionEndWire | null;
}

export interface StoredEventFilter {
  from?: string;
  to?: string;
  image?: string;
  pid?: number;
  eventType?: ProcessEventTypeWire;
  sessionId?: string;
  limit?: number;
  offset?: number;
}

export interface RetentionPolicyWire {
  max_age_days: number;
  max_events: number;
  max_bytes: number;
}

export interface StorageStatusWire {
  path: string | null;
  schema_version: number;
  event_count: number;
  invalid_count: number;
  revision_count: number;
  gap_count: number;
  session_count: number;
  oldest_event_at: string | null;
  newest_event_at: string | null;
  database_bytes: number;
  retention: RetentionPolicyWire;
  last_ingest_at: string | null;
  last_retention_at: string | null;
}

export interface IngestionStatusWire {
  state: "running" | "degraded" | "stopped";
  source_available: boolean;
  last_error: string | null;
  last_success_at: string | null;
  poll_count: number;
  batch_count: number;
  record_count: number;
  sessions_observed: number;
  session_id: string | null;
  session_closed: boolean;
  last_cursor: number;
  last_gap_records: number;
}

export interface StorageRuntimeReport {
  state: "ready" | "degraded" | "unavailable";
  initError: string | null;
  ingestionError: string | null;
  storage: StorageStatusWire | null;
  ingestion: IngestionStatusWire;
  lastError: string | null;
}

export type DetectionStatusWire =
  | "no_anomaly_detected"
  | "suspicious"
  | "anomaly"
  | "data_insufficient"
  | "indeterminate";

export type DetectionSeverityWire = "info" | "low" | "medium" | "high";
export type DetectionConfidenceWire = "low" | "medium" | "high";

export interface ThresholdValueWire {
  name: string;
  value: string;
}

export interface DetectionEvidenceWire {
  statement: string;
  event_ids: string[];
  value: string | null;
}

export interface DetectionResultWire {
  rule_id: string;
  rule_version: string;
  status: DetectionStatusWire;
  severity: DetectionSeverityWire;
  confidence: DetectionConfidenceWire;
  title: string;
  explanation: string;
  false_positive_notes: string[];
  thresholds: ThresholdValueWire[];
  evidence: DetectionEvidenceWire[];
  affected_event_count: number;
  window_start: string | null;
  window_end: string | null;
}

export interface InputQualityWire {
  total_events: number;
  start_events: number;
  stop_events: number;
  paired_events: number;
  unpaired_events: number;
  indeterminate_events: number;
  missing_time_events: number;
  missing_image_events: number;
  invalid_records: number;
  ingestion_gaps: number;
  truncated: boolean;
  session_closed: boolean;
  session_end_reason: string | null;
  etw_events_lost: number | null;
  realtime_buffers_lost: number | null;
  pipeline_dropped: number | null;
  pipeline_backlog: number | null;
  pipeline_shutdown_timed_out: boolean | null;
  sensor_health_known: boolean;
  sensor_healthy: boolean | null;
}

export interface SessionAnalysisWire {
  session_id: string;
  generated_at: string | null;
  rule_catalog_version: string;
  overall: DetectionStatusWire;
  summary: string;
  input: InputQualityWire;
  results: DetectionResultWire[];
}

export interface SessionRecordWire {
  session_id: string;
  first_seen_at: string | null;
  last_seen_at: string | null;
  last_cursor: number;
  closed: boolean;
  end_reason: string | null;
  ended_at: string | null;
  end_error: string | null;
  event_count: number;
  has_session_end: boolean;
}

export interface AffinityInspection {
  checkedAt: string;
  pid: number;
  identity: string;
  name: string;
  executablePath: string;
  startTime: string | null;
  inspectionState: "complete" | "partial" | "unavailable";
  signature: SignatureInfo;
  affinity: AffinityResult | null;
  cpuUsage: CpuUsageSample | null;
  issues: DataIssue[];
  affinityRisk: RiskAssessment;
  overallRisk: RiskAssessment;
  limitations: string[];
}
