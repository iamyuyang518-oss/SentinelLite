use sentinel_core::error::CoreError;
use sentinel_core::model::{
    AffinityInspection, AssetAssociationResult, DataIssue, NativeProtectionStatus,
    ProcessListResult, ProtectionOverview, RiskAssessment, ScheduledTaskInventoryResult,
    ServiceInventoryResult, SoftwareScanResult, StartupScanResult,
};
use sentinel_core::redaction::{
    redact_account_name, redact_user_profile_path, redact_user_profile_paths_in_text,
    redact_username_in_text,
};
use sentinel_detection::{
    AnalysisConfig, DetectionContext, SensorHealthInput, SessionAnalysis, SessionDataInput,
};
use sentinel_ipc::client::{ClientError, ClientOptions};
use sentinel_ipc::events::EventBatch;
use sentinel_ipc::protocol::{ProtocolErrorCode, SensorState, SensorStatus};
use sentinel_storage::{EventDetail, EventFilter, EventQueryResult, SessionRecord, StorageError};
use serde::Serialize;
use tauri::State;

use crate::storage_state::{StorageRuntimeReport, StorageState};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    code: String,
    message: String,
    retryable: bool,
}

impl CommandError {
    fn new(code: &str, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            retryable,
        }
    }

    fn invalid_pid() -> Self {
        Self::new("invalidPid", "进程 ID 无效。请刷新进程列表后重试。", false)
    }
}

impl From<CoreError> for CommandError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::InvalidPid => {
                Self::new("invalidPid", "进程 ID 无效。请刷新进程列表后重试。", false)
            }
            CoreError::InvalidProcessIdentity => Self::new(
                "invalidIdentity",
                "进程身份令牌无效。请刷新进程列表后重试。",
                false,
            ),
            CoreError::ProcessIdentityChanged { .. } => Self::new(
                "processIdentityChanged",
                "目标进程身份已变化，可能发生了 PID 复用或进程退出；请刷新后重试。",
                true,
            ),
            CoreError::ProcessUnavailable { reason, .. } => Self::new(
                "processUnavailable",
                format!("无法完成进程检查：{reason}。请刷新进程列表后重试。"),
                true,
            ),
            CoreError::ProcessSnapshot(detail) => {
                Self::new("scanFailed", format!("无法创建进程快照：{detail}"), true)
            }
        }
    }
}

async fn run_blocking<T, F>(task: F) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|_| {
            CommandError::new(
                "internalError",
                "后台检查任务异常结束，未返回结果。请重新尝试。",
                true,
            )
        })
}

fn redact_optional_path(value: &mut Option<String>) {
    if let Some(path) = value {
        *path = redact_user_profile_path(path);
    }
}

fn redact_text(value: &mut String) {
    *value = redact_user_profile_paths_in_text(value);
}

fn redact_issue(issue: &mut DataIssue) {
    redact_text(&mut issue.message);
    redact_text(&mut issue.source);
}

fn redact_risk(risk: &mut RiskAssessment) {
    redact_text(&mut risk.summary);
    redact_text(&mut risk.recommendation);
    for evidence in &mut risk.evidence {
        redact_text(&mut evidence.statement);
        redact_text(&mut evidence.source);
    }
    for blind_spot in &mut risk.blind_spots {
        redact_text(blind_spot);
    }
}

fn redact_software_inventory(result: &mut SoftwareScanResult) {
    for item in &mut result.items {
        redact_optional_path(&mut item.install_location);
        redact_optional_path(&mut item.display_icon_path);
        redact_risk(&mut item.risk);
        for issue in &mut item.issues {
            redact_issue(issue);
        }
    }
    for issue in &mut result.issues {
        redact_issue(issue);
    }
}

fn redact_startup_inventory(result: &mut StartupScanResult) {
    for item in &mut result.items {
        redact_optional_path(&mut item.target_path);
        redact_optional_path(&mut item.target_candidate);
        redact_risk(&mut item.risk);
        for issue in &mut item.issues {
            redact_issue(issue);
        }
    }
    for issue in &mut result.issues {
        redact_issue(issue);
    }
}

fn redact_service_inventory(result: &mut ServiceInventoryResult) {
    for item in &mut result.items {
        redact_optional_path(&mut item.executable_path);
        if let Some(account) = &mut item.account {
            *account = redact_account_name(account);
        }
        for issue in &mut item.issues {
            redact_issue(issue);
        }
    }
    for issue in &mut result.issues {
        redact_issue(issue);
    }
}

fn redact_association_result(result: &mut AssetAssociationResult) {
    let username = std::env::var("USERNAME")
        .ok()
        .filter(|value| value.len() >= 3);
    for relation in &mut result.relations {
        if let Some(user) = username.as_deref() {
            relation.id = redact_username_in_text(&relation.id, user);
            relation.target.id = redact_username_in_text(&relation.target.id, user);
            relation.target.name = redact_username_in_text(&relation.target.name, user);
            relation.target.detail = redact_username_in_text(&relation.target.detail, user);
        }
        redact_optional_path(&mut relation.target.path);
        redact_optional_path(&mut relation.software_evidence_path);
        redact_text(&mut relation.evidence_value);
        for issue in &mut relation.issues {
            redact_issue(issue);
        }
    }
    for issue in &mut result.issues {
        redact_issue(issue);
    }
    for software in &mut result.software {
        for issue in &mut software.issues {
            redact_issue(issue);
        }
    }
}

fn redact_task_inventory(result: &mut ScheduledTaskInventoryResult) {
    let username = std::env::var("USERNAME")
        .ok()
        .filter(|value| value.len() >= 3);
    for item in &mut result.items {
        if let Some(user) = username.as_deref() {
            item.id = redact_username_in_text(&item.id, user);
            item.name = redact_username_in_text(&item.name, user);
            item.path = redact_username_in_text(&item.path, user);
        }
        if let Some(run_as) = &mut item.run_as {
            *run_as = redact_account_name(run_as);
        }
        if let Some(author) = &mut item.author {
            redact_text(author);
            if !author.contains(":\\") && author.contains('\\') {
                *author = redact_account_name(author);
            } else if username
                .as_deref()
                .is_some_and(|user| author.eq_ignore_ascii_case(user))
            {
                *author = "***".to_string();
            }
        }
        for action in &mut item.actions {
            redact_optional_path(&mut action.executable_path);
        }
        for issue in &mut item.issues {
            redact_issue(issue);
        }
    }
    for issue in &mut result.issues {
        redact_issue(issue);
    }
}

#[tauri::command]
pub fn get_protection_overview() -> ProtectionOverview {
    sentinel_core::overview::protection_overview()
}

#[tauri::command]
pub async fn get_native_protection_status() -> Result<NativeProtectionStatus, CommandError> {
    run_blocking(sentinel_core::native_protection::query_native_protection).await
}

#[tauri::command]
pub async fn scan_installed_software() -> Result<SoftwareScanResult, CommandError> {
    let mut result = run_blocking(sentinel_core::software::scan_installed_software).await?;
    redact_software_inventory(&mut result);
    Ok(result)
}

#[tauri::command]
pub async fn scan_startup_items() -> Result<StartupScanResult, CommandError> {
    let mut result = run_blocking(sentinel_core::startup::scan_startup_items).await?;
    redact_startup_inventory(&mut result);
    Ok(result)
}

#[tauri::command]
pub async fn scan_asset_associations() -> Result<AssetAssociationResult, CommandError> {
    let mut result = run_blocking(sentinel_core::association::scan_asset_associations).await?;
    redact_association_result(&mut result);
    Ok(result)
}

#[tauri::command]
pub async fn list_services() -> Result<ServiceInventoryResult, CommandError> {
    let mut result = run_blocking(sentinel_core::services::list_services).await?;
    redact_service_inventory(&mut result);
    Ok(result)
}

#[tauri::command]
pub async fn list_scheduled_tasks() -> Result<ScheduledTaskInventoryResult, CommandError> {
    let mut result = run_blocking(sentinel_core::scheduled_tasks::list_scheduled_tasks).await?;
    redact_task_inventory(&mut result);
    Ok(result)
}

#[tauri::command]
pub async fn list_processes() -> Result<ProcessListResult, CommandError> {
    let result = run_blocking(sentinel_core::process::list_processes).await?;
    result.map_err(CommandError::from)
}

#[tauri::command]
pub async fn inspect_process_affinity(
    pid: u32,
    identity: String,
) -> Result<AffinityInspection, CommandError> {
    if pid == 0 {
        return Err(CommandError::invalid_pid());
    }
    if identity.is_empty() || identity.len() > 128 {
        return Err(CommandError::new(
            "invalidIdentity",
            "进程身份令牌无效。请刷新进程列表后重试。",
            false,
        ));
    }

    let result =
        run_blocking(move || sentinel_core::process::inspect_process_affinity(pid, &identity))
            .await?;
    result.map_err(CommandError::from)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorStatusReport {
    pub link_state: &'static str,
    pub sensor: Option<SensorStatus>,
    pub error: Option<String>,
}

impl SensorStatusReport {
    fn from_client_result(result: Result<SensorStatus, ClientError>) -> Self {
        match result {
            Ok(status) => {
                let healthy = status.health_ok && status.sensor_state == SensorState::Active;
                Self {
                    link_state: if healthy { "running" } else { "degraded" },
                    sensor: Some(status),
                    error: None,
                }
            }
            Err(error @ ClientError::NotRunning) => Self {
                link_state: "stopped",
                sensor: None,
                error: Some(error.to_string()),
            },
            Err(error @ ClientError::Unavailable(_)) => Self {
                link_state: "unavailable",
                sensor: None,
                error: Some(error.to_string()),
            },
            Err(ClientError::Remote(remote))
                if matches!(
                    remote.code,
                    ProtocolErrorCode::Unauthorized | ProtocolErrorCode::Unavailable
                ) =>
            {
                Self {
                    link_state: "unavailable",
                    sensor: None,
                    error: Some(format!("sensor rejected status query: {}", remote.message)),
                }
            }
            Err(error) => Self {
                link_state: "communicationFailed",
                sensor: None,
                error: Some(error.to_string()),
            },
        }
    }
}

#[tauri::command]
pub async fn get_sensor_status() -> Result<SensorStatusReport, CommandError> {
    let result = run_blocking(|| {
        sentinel_ipc::client::query_status(
            sentinel_ipc::DEFAULT_PIPE_NAME,
            &ClientOptions::secure(),
        )
    })
    .await?;
    Ok(SensorStatusReport::from_client_result(result))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorEventsReport {
    pub link_state: &'static str,
    pub batch: Option<EventBatch>,
    pub error: Option<String>,
}

impl SensorEventsReport {
    fn from_client_result(result: Result<EventBatch, ClientError>) -> Self {
        match result {
            Ok(batch) => Self {
                link_state: "running",
                batch: Some(batch),
                error: None,
            },
            Err(error @ ClientError::NotRunning) => Self {
                link_state: "stopped",
                batch: None,
                error: Some(error.to_string()),
            },
            Err(error @ ClientError::Unavailable(_)) => Self {
                link_state: "unavailable",
                batch: None,
                error: Some(error.to_string()),
            },
            Err(ClientError::Remote(remote))
                if matches!(
                    remote.code,
                    ProtocolErrorCode::Unauthorized | ProtocolErrorCode::Unavailable
                ) =>
            {
                Self {
                    link_state: "unavailable",
                    batch: None,
                    error: Some(format!("sensor rejected event query: {}", remote.message)),
                }
            }
            Err(error) => Self {
                link_state: "communicationFailed",
                batch: None,
                error: Some(error.to_string()),
            },
        }
    }
}

#[tauri::command]
pub async fn get_sensor_events(after: u64, limit: u16) -> Result<SensorEventsReport, CommandError> {
    let result = run_blocking(move || {
        sentinel_ipc::client::query_events(
            sentinel_ipc::DEFAULT_EVENT_PIPE_NAME,
            &ClientOptions::secure(),
            after,
            limit,
        )
    })
    .await?;
    Ok(SensorEventsReport::from_client_result(result))
}

impl From<StorageError> for CommandError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::InvalidFilter(message) => {
                Self::new("invalidStorageFilter", message, false)
            }
            StorageError::Corrupt(message) => Self::new(
                "storageUnavailable",
                format!("本地事件数据库已损坏，无法安全读取：{message}"),
                false,
            ),
            StorageError::Open(message) => Self::new(
                "storageUnavailable",
                format!("本地事件数据库无法打开：{message}"),
                false,
            ),
            StorageError::UnsupportedSchema { .. } => Self::new(
                "storageUnavailable",
                "本地事件数据库版本高于当前应用支持的版本。",
                false,
            ),
            StorageError::Query(message) => {
                Self::new("storageError", format!("本地事件查询失败：{message}"), true)
            }
            StorageError::Retention(message) => Self::new(
                "storageError",
                format!("本地事件保留策略执行失败：{message}"),
                true,
            ),
        }
    }
}

#[tauri::command]
pub async fn get_storage_status(
    state: State<'_, StorageState>,
) -> Result<StorageRuntimeReport, CommandError> {
    Ok(state.report())
}

#[tauri::command]
pub async fn query_stored_events(
    state: State<'_, StorageState>,
    filter: EventFilter,
) -> Result<EventQueryResult, CommandError> {
    let storage = state.storage().ok_or_else(|| {
        CommandError::new("storageUnavailable", "本地事件数据库当前不可用。", false)
    })?;
    let result = run_blocking(move || storage.query_events(&filter)).await?;
    result.map_err(CommandError::from)
}

#[tauri::command]
pub async fn get_stored_event(
    state: State<'_, StorageState>,
    event_id: String,
) -> Result<Option<EventDetail>, CommandError> {
    let storage = state.storage().ok_or_else(|| {
        CommandError::new("storageUnavailable", "本地事件数据库当前不可用。", false)
    })?;
    let result = run_blocking(move || storage.get_event(&event_id)).await?;
    result.map_err(CommandError::from)
}

#[tauri::command]
pub async fn list_sensor_sessions(
    state: State<'_, StorageState>,
    limit: u32,
) -> Result<Vec<SessionRecord>, CommandError> {
    let storage = state.storage().ok_or_else(|| {
        CommandError::new("storageUnavailable", "本地事件数据库当前不可用。", false)
    })?;
    let result = run_blocking(move || storage.list_sessions(limit)).await?;
    result.map_err(CommandError::from)
}

#[tauri::command]
pub async fn analyze_session(
    state: State<'_, StorageState>,
    session_id: String,
) -> Result<SessionAnalysis, CommandError> {
    let storage = state.storage().ok_or_else(|| {
        CommandError::new("storageUnavailable", "本地事件数据库当前不可用。", false)
    })?;
    let current_session = state.ingestion_session_id();
    run_blocking(move || {
        let set = storage
            .load_session_for_analysis(&session_id, 20_000)
            .map_err(CommandError::from)?
            .ok_or_else(|| CommandError::new("sessionNotFound", "未找到该传感器会话。", false))?;
        let sensor = if current_session.as_deref() == Some(session_id.as_str()) {
            sentinel_ipc::client::query_status(
                sentinel_ipc::DEFAULT_PIPE_NAME,
                &ClientOptions::secure(),
            )
            .ok()
            .map(|status| SensorHealthInput {
                healthy: status.health_ok && status.sensor_state == SensorState::Active,
                etw_events_lost: status.events_lost,
                realtime_buffers_lost: status.realtime_buffers_lost,
                pipeline_dropped: status.events_dropped.saturating_sub(status.decode_errors),
            })
        } else {
            None
        };
        let session_data = set
            .session_end
            .as_ref()
            .and_then(|end| end.final_stats.as_ref())
            .map(|stats| SessionDataInput {
                etw_events_lost: stats.events_lost,
                realtime_buffers_lost: stats.realtime_buffers_lost,
                pipeline_dropped: stats.pipeline_dropped_total(),
                pipeline_invalid: stats.pipeline_invalid,
                pipeline_backlog: stats.pipeline_backlog,
                pipeline_shutdown_timed_out: stats.pipeline_shutdown_timed_out,
            });
        let context = DetectionContext {
            session_id: set.session_id.clone(),
            session_closed: set.closed,
            session_end_reason: set.end_reason.clone(),
            invalid_records: set.invalid_records,
            ingestion_gaps: set.ingestion_gaps,
            truncated: set.truncated,
            sensor,
            session_data,
        };
        Ok(sentinel_detection::analyze(
            &set.events,
            &context,
            &AnalysisConfig::default(),
        ))
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_core::model::{
        AssetAssociationResult, AssetRelation, AssociationCertainty, AssociationEvidenceType,
        AssociationMatchMethod, AssociationRelationType, AssociationTarget, AssociationTargetKind,
        Confidence, Evidence, InstallScope, InstalledSoftware, InventoryObservation, PathState,
        RegistryValueState, RegistryView, RiskLevel, RiskScope, ScheduledTaskAction,
        ScheduledTaskActionType, ScheduledTaskState, ScheduledTaskSummary, ServiceStartType,
        ServiceState, ServiceSummary, ServiceType, SoftwareClassification, SoftwareScanResult,
        StartupApprovalState, StartupItem, StartupPathResolution, StartupSource, StartupTargetKind,
        StartupValueType,
    };

    fn risk_with_path(path: &str) -> RiskAssessment {
        RiskAssessment {
            scope: RiskScope::StartupMetadata,
            level: RiskLevel::NoClearIssue,
            confidence: Confidence::Low,
            rule_ids: Vec::new(),
            summary: format!("目标 {path}"),
            evidence: vec![Evidence {
                code: "TEST-PATH".to_string(),
                statement: format!("证据路径“{path}”。"),
                source: "test".to_string(),
            }],
            blind_spots: vec![format!("未核验 {path}")],
            recommendation: format!("保留 {path} 作为证据"),
        }
    }

    fn sensor_status(state: SensorState, health_ok: bool) -> SensorStatus {
        SensorStatus {
            sensor_state: state,
            permission_state: sentinel_ipc::protocol::PermissionState::Granted,
            events_received: 5,
            events_processed: 5,
            events_dropped: 0,
            events_lost: Some(0),
            realtime_buffers_lost: Some(0),
            decode_errors: 0,
            last_event_at: None,
            uptime_ms: 1000,
            health_ok,
            last_error: None,
        }
    }

    fn empty_event_batch() -> EventBatch {
        EventBatch {
            session_id: "session".to_owned(),
            oldest_cursor: 0,
            latest_cursor: 0,
            next_cursor: 0,
            cursor_expired: false,
            missed_records: 0,
            cursor_ahead: false,
            evictions: 0,
            session_closed: false,
            has_more: false,
            records: Vec::new(),
        }
    }

    #[test]
    fn event_batch_maps_to_running() {
        let report = SensorEventsReport::from_client_result(Ok(empty_event_batch()));
        assert_eq!(report.link_state, "running");
        assert!(report.batch.is_some());
        assert!(report.error.is_none());
    }

    #[test]
    fn missing_event_endpoint_is_stopped_not_empty_data() {
        let report = SensorEventsReport::from_client_result(Err(ClientError::NotRunning));
        assert_eq!(report.link_state, "stopped");
        assert!(report.batch.is_none());
        assert!(report.error.is_some());
    }

    #[test]
    fn unauthorized_event_query_is_unavailable() {
        let remote = sentinel_ipc::protocol::ProtocolError::new(
            ProtocolErrorCode::Unauthorized,
            "client user mismatch",
        );
        let report = SensorEventsReport::from_client_result(Err(ClientError::Remote(remote)));
        assert_eq!(report.link_state, "unavailable");
        assert!(report.batch.is_none());
    }

    #[test]
    fn event_timeout_is_a_communication_failure() {
        let report = SensorEventsReport::from_client_result(Err(ClientError::Timeout));
        assert_eq!(report.link_state, "communicationFailed");
        assert!(report.batch.is_none());
    }

    #[test]
    fn healthy_sensor_status_maps_to_running() {
        let report =
            SensorStatusReport::from_client_result(Ok(sensor_status(SensorState::Active, true)));
        assert_eq!(report.link_state, "running");
        assert!(report.sensor.is_some());
        assert!(report.error.is_none());
    }

    #[test]
    fn degraded_sensor_status_is_not_reported_as_running() {
        let report =
            SensorStatusReport::from_client_result(Ok(sensor_status(SensorState::Degraded, false)));
        assert_eq!(report.link_state, "degraded");
        assert!(report.sensor.is_some());
    }

    #[test]
    fn missing_sensor_is_stopped_not_healthy() {
        let report = SensorStatusReport::from_client_result(Err(ClientError::NotRunning));
        assert_eq!(report.link_state, "stopped");
        assert!(report.sensor.is_none());
        assert!(report.error.is_some());
    }

    #[test]
    fn timeout_is_a_communication_failure() {
        let report = SensorStatusReport::from_client_result(Err(ClientError::Timeout));
        assert_eq!(report.link_state, "communicationFailed");
        assert!(report.sensor.is_none());
    }

    #[test]
    fn lost_connection_is_a_communication_failure() {
        let report = SensorStatusReport::from_client_result(Err(ClientError::ConnectionLost(
            "pipe disconnected".to_owned(),
        )));
        assert_eq!(report.link_state, "communicationFailed");
        assert!(report.sensor.is_none());
    }

    #[test]
    fn unauthorized_remote_error_is_unavailable() {
        let remote = sentinel_ipc::protocol::ProtocolError::new(
            ProtocolErrorCode::Unauthorized,
            "client user mismatch",
        );
        let report = SensorStatusReport::from_client_result(Err(ClientError::Remote(remote)));
        assert_eq!(report.link_state, "unavailable");
        assert!(report.sensor.is_none());
    }

    #[test]
    fn converts_invalid_pid_into_stable_frontend_error() {
        let error: CommandError = CoreError::InvalidPid.into();
        assert_eq!(error.code, "invalidPid");
        assert!(!error.retryable);
    }

    #[test]
    fn identity_mismatch_is_a_retryable_identity_error() {
        let error: CommandError = CoreError::ProcessIdentityChanged { pid: 42 }.into();
        assert_eq!(error.code, "processIdentityChanged");
        assert!(error.retryable);
    }

    #[test]
    fn unavailable_process_is_retryable() {
        let error: CommandError = CoreError::ProcessUnavailable {
            pid: 42,
            reason: "query handle denied".to_string(),
        }
        .into();
        assert_eq!(error.code, "processUnavailable");
        assert!(error.retryable);
    }

    #[test]
    fn process_snapshot_failure_is_a_scan_failure() {
        let error: CommandError =
            CoreError::ProcessSnapshot("snapshot unavailable".to_string()).into();
        assert_eq!(error.code, "scanFailed");
        assert!(error.retryable);
    }

    #[test]
    fn service_inventory_redacts_account_and_executable_path() {
        let mut result = ServiceInventoryResult {
            checked_at: "2026-11-01T00:00:00Z".to_string(),
            observation: InventoryObservation::Observed,
            items: vec![ServiceSummary {
                id: "service:test".to_string(),
                name: "test".to_string(),
                display_name: "Test".to_string(),
                state: ServiceState::Running,
                start_type: ServiceStartType::Auto,
                service_types: vec![ServiceType::OwnProcess],
                account: Some("CONTOSO\\alice".to_string()),
                executable_path: Some(r"C:\Users\Alice\AppData\Local\svc.exe".to_string()),
                observation: InventoryObservation::Observed,
                issues: Vec::new(),
            }],
            issues: Vec::new(),
            limitations: Vec::new(),
            truncated: false,
        };

        redact_service_inventory(&mut result);

        assert_eq!(result.items[0].account.as_deref(), Some("CONTOSO\\***"));
        assert_eq!(
            result.items[0].executable_path.as_deref(),
            Some(r"C:\Users\***\AppData\Local\svc.exe")
        );
    }

    #[test]
    fn task_inventory_redacts_run_as_and_action_paths() {
        let mut result = ScheduledTaskInventoryResult {
            checked_at: "2026-11-01T00:00:00Z".to_string(),
            observation: InventoryObservation::Observed,
            items: vec![ScheduledTaskSummary {
                id: "task:\\Test".to_string(),
                name: "Test".to_string(),
                path: "\\Test".to_string(),
                state: ScheduledTaskState::Ready,
                enabled: Some(true),
                author: Some("CONTOSO\\alice".to_string()),
                run_as: Some("CONTOSO\\alice".to_string()),
                actions: vec![ScheduledTaskAction {
                    action_type: ScheduledTaskActionType::Exec,
                    executable_path: Some(r"C:\Users\Alice\AppData\Local\task.exe".to_string()),
                }],
                actions_truncated: false,
                observation: InventoryObservation::Observed,
                issues: Vec::new(),
            }],
            issues: Vec::new(),
            limitations: Vec::new(),
            truncated: false,
        };

        redact_task_inventory(&mut result);

        assert_eq!(result.items[0].run_as.as_deref(), Some("CONTOSO\\***"));
        assert_eq!(result.items[0].author.as_deref(), Some("CONTOSO\\***"));
        assert_eq!(
            result.items[0].actions[0].executable_path.as_deref(),
            Some(r"C:\Users\***\AppData\Local\task.exe")
        );
    }

    #[test]
    fn software_inventory_redacts_install_and_icon_paths() {
        let path = r"C:\Users\Alice\AppData\Local\Programs\Test\test.exe";
        let mut result = SoftwareScanResult {
            checked_at: "2026-11-01T00:00:00Z".to_string(),
            observation: InventoryObservation::Observed,
            sources: Vec::new(),
            items: vec![InstalledSoftware {
                id: "software:test".to_string(),
                display_name: "Test".to_string(),
                version: None,
                version_state: RegistryValueState::Missing,
                publisher: None,
                publisher_state: RegistryValueState::Missing,
                install_location: Some(path.to_string()),
                install_location_state: PathState::Present,
                install_location_value_state: RegistryValueState::Present,
                install_date: None,
                install_date_state: RegistryValueState::Missing,
                display_icon_path: Some(path.to_string()),
                display_icon_state: RegistryValueState::Present,
                windows_installer: None,
                windows_installer_state: RegistryValueState::Missing,
                uninstall_command_present: false,
                quiet_uninstall_command_present: false,
                scope: InstallScope::Machine,
                registry_view: RegistryView::Registry64,
                registry_key: "HKEY_LOCAL_MACHINE\\test".to_string(),
                system_component: None,
                system_component_state: RegistryValueState::Missing,
                no_remove: None,
                no_remove_state: RegistryValueState::Missing,
                parent_key_name: None,
                parent_key_name_state: RegistryValueState::Missing,
                release_type: None,
                release_type_state: RegistryValueState::Missing,
                classification: SoftwareClassification::UserSoftware,
                observation: InventoryObservation::Observed,
                issues: Vec::new(),
                risk: risk_with_path(path),
            }],
            issues: Vec::new(),
            limitations: Vec::new(),
            truncated: false,
        };

        redact_software_inventory(&mut result);

        assert_eq!(
            result.items[0].install_location.as_deref(),
            Some(r"C:\Users\***\AppData\Local\Programs\Test\test.exe")
        );
        assert_eq!(
            result.items[0].display_icon_path.as_deref(),
            Some(r"C:\Users\***\AppData\Local\Programs\Test\test.exe")
        );
        assert!(!result.items[0].risk.evidence[0].statement.contains("Alice"));
    }

    #[test]
    fn association_redacts_username_from_task_targets() {
        let Ok(username) = std::env::var("USERNAME") else {
            return;
        };
        if username.len() < 3 {
            return;
        }
        let path = r"C:\Users\Alice\AppData\Local\WpsUpdate.exe".to_string();
        let mut result = AssetAssociationResult {
            checked_at: "2026-11-01T00:00:00Z".to_string(),
            observation: InventoryObservation::Observed,
            sources: Vec::new(),
            software: Vec::new(),
            relations: vec![AssetRelation {
                id: format!("association:test:task_{username}"),
                software_id: "software:test".to_string(),
                software_name: "Test".to_string(),
                target: AssociationTarget {
                    kind: AssociationTargetKind::ScheduledTask,
                    id: format!("task:\\WpsUpdateTask_{username}:action:0"),
                    name: format!("WpsUpdateTask_{username}"),
                    detail: "任务动作 1".to_string(),
                    path: Some(path),
                    observation: InventoryObservation::Observed,
                },
                relation_type: AssociationRelationType::SoftwareScheduledTask,
                evidence_type: AssociationEvidenceType::DisplayIconPath,
                match_method: AssociationMatchMethod::ExactPath,
                software_evidence_path: Some(r"C:\Program Files\Test\test.exe".to_string()),
                evidence_value: r"C:\Users\Alice\AppData\Local\WpsUpdate.exe".to_string(),
                certainty: AssociationCertainty::Exact,
                observation: InventoryObservation::Observed,
                ambiguous: false,
                candidate_software_count: 1,
                issues: Vec::new(),
            }],
            issues: Vec::new(),
            limitations: Vec::new(),
            truncated: false,
        };

        redact_association_result(&mut result);

        let relation = &result.relations[0];
        assert!(!relation.id.contains(&username));
        assert!(!relation.target.id.contains(&username));
        assert!(!relation.target.name.contains(&username));
        assert_eq!(
            relation.target.path.as_deref(),
            Some(r"C:\Users\***\AppData\Local\WpsUpdate.exe")
        );
    }

    #[test]
    fn task_inventory_redacts_current_username_from_name_and_author() {
        let Ok(username) = std::env::var("USERNAME") else {
            return;
        };
        if username.len() < 3 {
            return;
        }
        let mut result = ScheduledTaskInventoryResult {
            checked_at: "2026-11-01T00:00:00Z".to_string(),
            observation: InventoryObservation::Observed,
            items: vec![ScheduledTaskSummary {
                id: format!("task:\\Task_{username}"),
                name: format!("Task_{username}"),
                path: format!("\\Task_{username}"),
                state: ScheduledTaskState::Ready,
                enabled: Some(true),
                author: Some(username.clone()),
                run_as: Some(username.clone()),
                actions: Vec::new(),
                actions_truncated: false,
                observation: InventoryObservation::Observed,
                issues: Vec::new(),
            }],
            issues: Vec::new(),
            limitations: Vec::new(),
            truncated: false,
        };

        redact_task_inventory(&mut result);

        assert!(!result.items[0].id.contains(&username));
        assert!(!result.items[0].name.contains(&username));
        assert!(!result.items[0].path.contains(&username));
        assert_eq!(result.items[0].author.as_deref(), Some("***"));
        assert_eq!(result.items[0].run_as.as_deref(), Some("***"));
    }

    #[test]
    fn startup_inventory_redacts_target_paths_and_risk_evidence() {
        let path = r"C:\Users\Alice\AppData\Local\Temp\startup.exe";
        let mut result = StartupScanResult {
            checked_at: "2026-11-01T00:00:00Z".to_string(),
            observation: InventoryObservation::Observed,
            sources: Vec::new(),
            items: vec![StartupItem {
                id: "startup:test".to_string(),
                name: "Test".to_string(),
                value_name: Some("Test".to_string()),
                value_type: Some(StartupValueType::RegSz),
                raw_value_readable: true,
                source: StartupSource::RegistryRun,
                scope: InstallScope::User,
                registry_view: Some(RegistryView::Shared),
                registry_key: Some("HKEY_CURRENT_USER\\...\\Run".to_string()),
                target_kind: StartupTargetKind::RegistryCommand,
                target_path: Some(path.to_string()),
                target_candidate: Some(path.to_string()),
                target_state: PathState::Present,
                path_resolution: StartupPathResolution::QuotedExecutable,
                shortcut_arguments_present: None,
                shortcut_working_directory_present: None,
                approval_state: StartupApprovalState::NotChecked,
                observation: InventoryObservation::Observed,
                issues: Vec::new(),
                risk: risk_with_path(path),
            }],
            issues: Vec::new(),
            limitations: Vec::new(),
            truncated: false,
        };

        redact_startup_inventory(&mut result);

        assert_eq!(
            result.items[0].target_path.as_deref(),
            Some(r"C:\Users\***\AppData\Local\Temp\startup.exe")
        );
        assert_eq!(
            result.items[0].target_candidate.as_deref(),
            Some(r"C:\Users\***\AppData\Local\Temp\startup.exe")
        );
        assert!(!result.items[0].risk.evidence[0].statement.contains("Alice"));
    }

    #[test]
    fn risk_text_redaction_covers_embedded_profile_paths() {
        let mut risk = risk_with_path(r"C:\Users\Alice\AppData\Local\Temp\app.exe");
        redact_risk(&mut risk);

        assert!(!risk.summary.contains("Alice"));
        assert!(!risk.evidence[0].statement.contains("Alice"));
        assert!(!risk.blind_spots[0].contains("Alice"));
        assert!(!risk.recommendation.contains("Alice"));
        assert!(risk.evidence[0].statement.contains(r"C:\Users\***\"));
    }

    #[test]
    fn optional_path_redaction_is_idempotent() {
        let mut value = Some(r"C:\Users\***\AppData\Local\app.exe".to_string());
        redact_optional_path(&mut value);
        assert_eq!(
            value.as_deref(),
            Some(r"C:\Users\***\AppData\Local\app.exe")
        );
    }
}
