//! Conservative behavior rules over normalized process events.
//!
//! Data support is a first-class input: every analysis records whether the
//! session was closed, whether ETW loss is known, whether the ingestion
//! pipeline reported gaps, and whether timing fields are present. A session
//! with incomplete data cannot be reported as "normal".

use chrono::{DateTime, Utc};
use sentinel_events::{
    PairingStatus, ProcessEvent, ProcessEventType, UnpairedReason,
    time::{now_unix_ms, unix_ms_to_rfc3339},
};
use serde::{Deserialize, Serialize};

pub const RULES_VERSION: &str = "1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionStatus {
    NoAnomalyDetected,
    Suspicious,
    Anomaly,
    DataInsufficient,
    Indeterminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThresholdValue {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectionEvidence {
    pub statement: String,
    pub event_ids: Vec<String>,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectionResult {
    pub rule_id: String,
    pub rule_version: String,
    pub status: DetectionStatus,
    pub severity: Severity,
    pub confidence: Confidence,
    pub title: String,
    pub explanation: String,
    pub false_positive_notes: Vec<String>,
    pub thresholds: Vec<ThresholdValue>,
    pub evidence: Vec<DetectionEvidence>,
    pub affected_event_count: u64,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisConfig {
    pub min_events: usize,
    pub burst_window_secs: i64,
    pub burst_start_threshold: usize,
    pub exit_window_secs: i64,
    pub exit_threshold: usize,
    pub orphan_min_events: usize,
    pub orphan_ratio_percent: u8,
    pub max_evidence_ids: usize,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            min_events: 10,
            burst_window_secs: 10,
            burst_start_threshold: 30,
            exit_window_secs: 10,
            exit_threshold: 20,
            orphan_min_events: 20,
            orphan_ratio_percent: 30,
            max_evidence_ids: 32,
        }
    }
}

impl AnalysisConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.min_events == 0 {
            return Err("min_events must be greater than zero".to_owned());
        }
        if self.burst_window_secs <= 0 || self.exit_window_secs <= 0 {
            return Err("rule windows must be greater than zero".to_owned());
        }
        if self.burst_start_threshold < 2 || self.exit_threshold < 2 {
            return Err("rule thresholds must be at least two events".to_owned());
        }
        if self.orphan_min_events == 0 {
            return Err("orphan_min_events must be greater than zero".to_owned());
        }
        if self.orphan_ratio_percent == 0 || self.orphan_ratio_percent > 100 {
            return Err("orphan_ratio_percent must be between 1 and 100".to_owned());
        }
        if self.max_evidence_ids == 0 || self.max_evidence_ids > 64 {
            return Err("max_evidence_ids must be between 1 and 64".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SensorHealthInput {
    pub healthy: bool,
    pub etw_events_lost: Option<u64>,
    pub realtime_buffers_lost: Option<u64>,
    pub pipeline_dropped: u64,
}

/// Final statistics persisted with a closed session's `SessionEnd`.
///
/// These are authoritative for a historical session: the live sensor may no
/// longer exist. Unknown ETW values stay `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDataInput {
    pub etw_events_lost: Option<u64>,
    pub realtime_buffers_lost: Option<u64>,
    pub pipeline_dropped: u64,
    pub pipeline_invalid: u64,
    pub pipeline_backlog: u64,
    pub pipeline_shutdown_timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectionContext {
    pub session_id: String,
    pub session_closed: bool,
    pub session_end_reason: Option<String>,
    pub invalid_records: u64,
    pub ingestion_gaps: u64,
    pub truncated: bool,
    pub sensor: Option<SensorHealthInput>,
    pub session_data: Option<SessionDataInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputQuality {
    pub total_events: u64,
    pub start_events: u64,
    pub stop_events: u64,
    pub paired_events: u64,
    pub unpaired_events: u64,
    pub indeterminate_events: u64,
    pub missing_time_events: u64,
    pub missing_image_events: u64,
    pub invalid_records: u64,
    pub ingestion_gaps: u64,
    pub truncated: bool,
    pub session_closed: bool,
    pub session_end_reason: Option<String>,
    pub etw_events_lost: Option<u64>,
    pub realtime_buffers_lost: Option<u64>,
    pub pipeline_dropped: Option<u64>,
    pub pipeline_backlog: Option<u64>,
    pub pipeline_shutdown_timed_out: Option<bool>,
    pub sensor_health_known: bool,
    pub sensor_healthy: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAnalysis {
    pub session_id: String,
    pub generated_at: Option<String>,
    pub rule_catalog_version: String,
    pub overall: DetectionStatus,
    pub summary: String,
    pub input: InputQuality,
    pub results: Vec<DetectionResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleDefinition {
    pub id: String,
    pub version: String,
    pub title: String,
    pub description: String,
    pub data_required: Vec<String>,
    pub false_positive_notes: Vec<String>,
    pub thresholds: Vec<ThresholdValue>,
}

struct TimedEvent<'a> {
    milliseconds: i64,
    event: &'a ProcessEvent,
}

struct Window {
    count: usize,
    start_ms: i64,
    end_ms: i64,
    ids: Vec<String>,
}

pub fn analyze(
    events: &[ProcessEvent],
    context: &DetectionContext,
    config: &AnalysisConfig,
) -> SessionAnalysis {
    let input = input_quality(events, context);
    let mut results = Vec::new();

    data_quality_results(&input, context, config, &mut results);

    if events.len() >= config.min_events {
        if let Some(result) = burst_result(
            events,
            ProcessEventType::Started,
            config.burst_window_secs,
            config.burst_start_threshold,
            config.max_evidence_ids,
        ) {
            results.push(result);
        }
        if let Some(result) = burst_result(
            events,
            ProcessEventType::Stopped,
            config.exit_window_secs,
            config.exit_threshold,
            config.max_evidence_ids,
        ) {
            results.push(result);
        }
        if let Some(result) = orphan_result(events, config) {
            results.push(result);
        }
        if let Some(result) = integrity_result(events, config) {
            results.push(result);
        }
    }

    results.sort_by(|left, right| {
        status_rank(left.status)
            .cmp(&status_rank(right.status))
            .then_with(|| severity_rank(right.severity).cmp(&severity_rank(left.severity)))
            .then_with(|| left.rule_id.cmp(&right.rule_id))
    });
    let overall = overall_status(&results);
    let summary = summary_for(overall).to_owned();
    SessionAnalysis {
        session_id: context.session_id.clone(),
        generated_at: now_unix_ms().and_then(unix_ms_to_rfc3339),
        rule_catalog_version: RULES_VERSION.to_owned(),
        overall,
        summary,
        input,
        results,
    }
}

fn input_quality(events: &[ProcessEvent], context: &DetectionContext) -> InputQuality {
    // Final session data is authoritative for a closed session. If it says
    // ETW loss is unknown, a still-readable live sensor must not turn that
    // into a known value. Live health is only a fallback when no final block
    // exists (typically the currently running session).
    let (
        etw_events_lost,
        realtime_buffers_lost,
        pipeline_dropped,
        pipeline_backlog,
        pipeline_shutdown_timed_out,
    ) = if let Some(stats) = context.session_data.as_ref() {
        (
            stats.etw_events_lost,
            stats.realtime_buffers_lost,
            Some(stats.pipeline_dropped),
            Some(stats.pipeline_backlog),
            Some(stats.pipeline_shutdown_timed_out),
        )
    } else if let Some(sensor) = context.sensor.as_ref() {
        (
            sensor.etw_events_lost,
            sensor.realtime_buffers_lost,
            Some(sensor.pipeline_dropped),
            None,
            None,
        )
    } else {
        (None, None, None, None, None)
    };
    let final_invalid = context
        .session_data
        .as_ref()
        .map_or(0, |stats| stats.pipeline_invalid);
    let mut input = InputQuality {
        total_events: events.len() as u64,
        start_events: 0,
        stop_events: 0,
        paired_events: 0,
        unpaired_events: 0,
        indeterminate_events: 0,
        missing_time_events: 0,
        missing_image_events: 0,
        invalid_records: context.invalid_records.max(final_invalid),
        ingestion_gaps: context.ingestion_gaps,
        truncated: context.truncated,
        session_closed: context.session_closed,
        session_end_reason: context.session_end_reason.clone(),
        etw_events_lost,
        realtime_buffers_lost,
        pipeline_dropped,
        pipeline_backlog,
        pipeline_shutdown_timed_out,
        sensor_health_known: context.sensor.is_some(),
        sensor_healthy: context.sensor.as_ref().map(|sensor| sensor.healthy),
    };
    for event in events {
        match event.event_type {
            ProcessEventType::Started => input.start_events += 1,
            ProcessEventType::Stopped => input.stop_events += 1,
        }
        match event.pairing {
            PairingStatus::Paired { .. } => input.paired_events += 1,
            PairingStatus::Unpaired { .. } => input.unpaired_events += 1,
            PairingStatus::Indeterminate { .. } => input.indeterminate_events += 1,
            PairingStatus::Waiting => {}
        }
        if event_time_ms(event).is_none() {
            input.missing_time_events += 1;
        }
        if event.image_name.is_none() {
            input.missing_image_events += 1;
        }
    }
    input
}

fn data_quality_results(
    input: &InputQuality,
    context: &DetectionContext,
    config: &AnalysisConfig,
    results: &mut Vec<DetectionResult>,
) {
    if input.total_events < config.min_events as u64 {
        results.push(data_result(
            "DATA-SMALL-001",
            DetectionStatus::DataInsufficient,
            Severity::Info,
            Confidence::High,
            "事件样本不足",
            format!(
                "本会话仅有 {} 条标准化进程事件，少于规则所需的 {} 条；不能据此判断行为正常或异常。",
                input.total_events, config.min_events
            ),
            vec![ThresholdValue {
                name: "min_events".to_owned(),
                value: config.min_events.to_string(),
            }],
        ));
    }

    if !context.session_closed {
        results.push(data_result(
            "DATA-SESSION-001",
            DetectionStatus::DataInsufficient,
            Severity::Info,
            Confidence::High,
            "会话尚未结束",
            "尚未观察到 SessionEnd；进程生命周期可能仍不完整，不能宣称本次会话正常。".to_owned(),
            Vec::new(),
        ));
    }

    let known_loss = input.etw_events_lost.unwrap_or(0) > 0
        || input.realtime_buffers_lost.unwrap_or(0) > 0
        || input.pipeline_dropped.is_some_and(|value| value > 0)
        || input.pipeline_backlog.is_some_and(|value| value > 0)
        || input.pipeline_shutdown_timed_out == Some(true)
        || input.invalid_records > 0
        || input.ingestion_gaps > 0
        || input.truncated;
    if known_loss {
        results.push(data_result(
            "DATA-LOSS-001",
            DetectionStatus::DataInsufficient,
            Severity::Info,
            Confidence::High,
            "事件数据存在已知缺口",
            format!(
                "ETW 丢失={}，实时缓冲丢失={}，管道丢弃={}，最终积压={}，管道关闭超时={}，无法解码={}，投递缺口={}，事件加载截断={}。以上任一缺口都会使行为结论不完整。",
                option_value(input.etw_events_lost),
                option_value(input.realtime_buffers_lost),
                option_value(input.pipeline_dropped),
                option_value(input.pipeline_backlog),
                input
                    .pipeline_shutdown_timed_out
                    .map_or_else(|| "未知".to_owned(), |value| value.to_string()),
                input.invalid_records,
                input.ingestion_gaps,
                input.truncated
            ),
            Vec::new(),
        ));
    } else if input.etw_events_lost.is_none() {
        results.push(data_result(
            "DATA-ETW-UNKNOWN-001",
            DetectionStatus::Indeterminate,
            Severity::Info,
            Confidence::Medium,
            "ETW 丢失统计未知",
            "无法取得该会话对应的 ETW 丢失统计；不能把未知丢失当作零丢失，因此不能宣称行为正常。"
                .to_owned(),
            Vec::new(),
        ));
    } else if input.sensor_health_known && !input.sensor_healthy.unwrap_or(false) {
        results.push(data_result(
            "DATA-HEALTH-001",
            DetectionStatus::Indeterminate,
            Severity::Info,
            Confidence::Medium,
            "传感器健康状态未通过",
            "传感器在采集期间报告了健康异常；分析结果仍被保留，但不能作为正常结论。".to_owned(),
            Vec::new(),
        ));
    }

    if input.missing_time_events > 0 {
        let status = if input.missing_time_events * 2 > input.total_events.max(1) {
            DetectionStatus::DataInsufficient
        } else {
            DetectionStatus::Indeterminate
        };
        results.push(data_result(
            "DATA-TIME-001",
            status,
            Severity::Info,
            Confidence::High,
            "部分事件缺少时间",
            format!(
                "{} 条事件没有任何可用时间，基于时间窗口的规则会漏掉它们。",
                input.missing_time_events
            ),
            Vec::new(),
        ));
    }

    if input.missing_image_events > 0 {
        results.push(data_result(
            "DATA-FIELDS-001",
            DetectionStatus::Indeterminate,
            Severity::Info,
            Confidence::High,
            "部分事件缺少映像名称",
            format!(
                "{} 条事件缺少映像名称；基于名称或路径的分析不可用，当前规则只使用时间、事件类型和配对状态。",
                input.missing_image_events
            ),
            Vec::new(),
        ));
    }
}

fn burst_result(
    events: &[ProcessEvent],
    event_type: ProcessEventType,
    window_secs: i64,
    threshold: usize,
    max_ids: usize,
) -> Option<DetectionResult> {
    let mut timed: Vec<TimedEvent<'_>> = events
        .iter()
        .filter(|event| event.event_type == event_type)
        .filter_map(|event| {
            event_time_ms(event).map(|milliseconds| TimedEvent {
                milliseconds,
                event,
            })
        })
        .collect();
    timed.sort_by_key(|event| event.milliseconds);
    let window = best_window(&timed, window_secs, max_ids)?;
    if window.count < threshold {
        return None;
    }
    let (title, rule_id, kind) = match event_type {
        ProcessEventType::Started => ("短时间内大量进程启动", "PROC-BURST-001", "进程启动"),
        ProcessEventType::Stopped => ("短时间内大量进程退出", "PROC-EXIT-002", "进程退出"),
    };
    Some(DetectionResult {
        rule_id: rule_id.to_owned(),
        rule_version: RULES_VERSION.to_owned(),
        status: DetectionStatus::Suspicious,
        severity: if event_type == ProcessEventType::Started {
            Severity::Medium
        } else {
            Severity::Low
        },
        confidence: Confidence::Medium,
        title: title.to_owned(),
        explanation: format!(
            "在约 {window_secs} 秒窗口内观察到 {} 次{kind}（阈值 {threshold}）。这可能来自安装器、构建脚本、更新、登录脚本或进程树集中退出，也可能反映异常行为；本结果不判断恶意性。",
            window.count
        ),
        false_positive_notes: vec![
            "软件安装、更新、构建或批量脚本可能产生同样的模式。".to_owned(),
            "进程树集中退出可能只是应用关闭或用户注销。".to_owned(),
        ],
        thresholds: vec![
            ThresholdValue {
                name: "window_seconds".to_owned(),
                value: window_secs.to_string(),
            },
            ThresholdValue {
                name: "event_count".to_owned(),
                value: threshold.to_string(),
            },
        ],
        evidence: vec![DetectionEvidence {
            statement: format!("窗口内共有 {} 次{kind}。", window.count),
            event_ids: window.ids,
            value: Some(window.count.to_string()),
        }],
        affected_event_count: window.count as u64,
        window_start: unix_ms_to_rfc3339(window.start_ms.max(0) as u64),
        window_end: unix_ms_to_rfc3339(window.end_ms.max(0) as u64),
    })
}

fn best_window(timed: &[TimedEvent<'_>], window_secs: i64, max_ids: usize) -> Option<Window> {
    if timed.is_empty() {
        return None;
    }
    let window_ms = window_secs.saturating_mul(1_000);
    let mut best: Option<(usize, usize, usize)> = None;
    let mut right = 0usize;
    for left in 0..timed.len() {
        if right < left {
            right = left;
        }
        while right < timed.len()
            && timed[right]
                .milliseconds
                .saturating_sub(timed[left].milliseconds)
                <= window_ms
        {
            right += 1;
        }
        let count = right - left;
        if best.is_none_or(|(_, _, best_count)| count > best_count) {
            best = Some((left, right, count));
        }
    }
    let (left, right, count) = best?;
    let end_index = right.saturating_sub(1).max(left);
    let ids: Vec<String> = timed[left..right]
        .iter()
        .take(max_ids)
        .map(|timed| timed.event.event_id.clone())
        .collect();
    Some(Window {
        count,
        start_ms: timed[left].milliseconds,
        end_ms: timed[end_index].milliseconds,
        ids,
    })
}

fn orphan_result(events: &[ProcessEvent], config: &AnalysisConfig) -> Option<DetectionResult> {
    let total = events.len();
    if total < config.orphan_min_events {
        return None;
    }
    let mut unpaired_ids = Vec::new();
    let mut unpaired_count = 0usize;
    for event in events {
        let is_orphan = match &event.pairing {
            PairingStatus::Unpaired { reason } => matches!(
                reason,
                UnpairedReason::StopWithoutObservedStart
                    | UnpairedReason::OrphanStopNotRetained
                    | UnpairedReason::StartNotRetained
            ),
            PairingStatus::Indeterminate { .. } => true,
            _ => false,
        };
        if is_orphan {
            unpaired_count += 1;
            if unpaired_ids.len() < config.max_evidence_ids {
                unpaired_ids.push(event.event_id.clone());
            }
        }
    }
    let ratio = unpaired_count.saturating_mul(100) / total;
    if ratio < usize::from(config.orphan_ratio_percent) {
        return None;
    }
    Some(DetectionResult {
        rule_id: "PROC-ORPHAN-003".to_owned(),
        rule_version: RULES_VERSION.to_owned(),
        status: DetectionStatus::Anomaly,
        severity: Severity::Low,
        confidence: Confidence::High,
        title: "进程生命周期配对覆盖不足".to_owned(),
        explanation: format!(
            "{unpaired_count}/{total} 条生命周期事件无法可靠配对（{ratio}%，阈值 {}%）。这首先说明数据完整性存在问题，而不是进程恶意；应结合 ETW 丢失、会话边界和投递缺口核实。",
            config.orphan_ratio_percent
        ),
        false_positive_notes: vec![
            "会话开始时观察到的进程退出天然没有对应的启动事件。".to_owned(),
            "ETW 丢失或传感器重启也会造成配对缺口。".to_owned(),
        ],
        thresholds: vec![
            ThresholdValue {
                name: "min_events".to_owned(),
                value: config.orphan_min_events.to_string(),
            },
            ThresholdValue {
                name: "unpaired_ratio_percent".to_owned(),
                value: config.orphan_ratio_percent.to_string(),
            },
        ],
        evidence: vec![DetectionEvidence {
            statement: format!("无法配对或身份不确定的事件 {unpaired_count} 条。"),
            event_ids: unpaired_ids,
            value: Some(format!("{unpaired_count}/{total}")),
        }],
        affected_event_count: unpaired_count as u64,
        window_start: None,
        window_end: None,
    })
}

fn integrity_result(events: &[ProcessEvent], config: &AnalysisConfig) -> Option<DetectionResult> {
    let mut ids = Vec::new();
    let mut count = 0usize;
    for event in events {
        let inconsistent = match &event.pairing {
            PairingStatus::Unpaired { reason } => matches!(
                reason,
                UnpairedReason::DuplicateStart { .. }
                    | UnpairedReason::DuplicateStop { .. }
                    | UnpairedReason::IdentityMismatch { .. }
            ),
            _ => false,
        };
        if inconsistent {
            count += 1;
            if ids.len() < config.max_evidence_ids {
                ids.push(event.event_id.clone());
            }
        }
    }
    if count == 0 {
        return None;
    }
    Some(DetectionResult {
        rule_id: "PROC-INTEGRITY-004".to_owned(),
        rule_version: RULES_VERSION.to_owned(),
        status: DetectionStatus::Anomaly,
        severity: Severity::Medium,
        confidence: Confidence::High,
        title: "进程生命周期一致性异常".to_owned(),
        explanation: format!(
            "观察到 {count} 条重复启动、重复退出或同 PID 身份不匹配事件。这可能是 PID 复用、ETW 重复交付、传感器边界或数据缺口，不是恶意判定；需要人工核实相关事件。"
        ),
        false_positive_notes: vec![
            "PID 在短时间内被复用是正常的 Windows 行为。".to_owned(),
            "ETW 重传或传感器重启可能产生重复事件。".to_owned(),
        ],
        thresholds: Vec::new(),
        evidence: vec![DetectionEvidence {
            statement: format!("发现 {count} 条生命周期一致性异常事件。"),
            event_ids: ids,
            value: Some(count.to_string()),
        }],
        affected_event_count: count as u64,
        window_start: None,
        window_end: None,
    })
}

fn data_result(
    rule_id: &str,
    status: DetectionStatus,
    severity: Severity,
    confidence: Confidence,
    title: &str,
    explanation: String,
    thresholds: Vec<ThresholdValue>,
) -> DetectionResult {
    DetectionResult {
        rule_id: rule_id.to_owned(),
        rule_version: RULES_VERSION.to_owned(),
        status,
        severity,
        confidence,
        title: title.to_owned(),
        explanation,
        false_positive_notes: Vec::new(),
        thresholds,
        evidence: Vec::new(),
        affected_event_count: 0,
        window_start: None,
        window_end: None,
    }
}

fn overall_status(results: &[DetectionResult]) -> DetectionStatus {
    let has = |status: DetectionStatus| results.iter().any(|result| result.status == status);
    for status in [
        DetectionStatus::Anomaly,
        DetectionStatus::DataInsufficient,
        DetectionStatus::Suspicious,
        DetectionStatus::Indeterminate,
    ] {
        if has(status) {
            return status;
        }
    }
    DetectionStatus::NoAnomalyDetected
}

fn summary_for(status: DetectionStatus) -> &'static str {
    match status {
        DetectionStatus::NoAnomalyDetected => {
            "在本次会话的事件范围内未观察到规则覆盖的异常模式。这不证明进程安全，也不是恶意判定。"
        }
        DetectionStatus::Suspicious => {
            "观察到需要人工核实的模式。可疑不等于恶意；请查看证据、误报说明和数据质量。"
        }
        DetectionStatus::Anomaly => {
            "观察到生命周期或数据一致性异常。该结果首先描述可验证的异常，不是恶意判定。"
        }
        DetectionStatus::DataInsufficient => "输入数据不足或存在已知缺口，无法形成正常或异常结论。",
        DetectionStatus::Indeterminate => "关键数据缺失，当前无法判断。不会把无法判断显示为正常。",
    }
}

fn status_rank(status: DetectionStatus) -> u8 {
    match status {
        DetectionStatus::Anomaly => 0,
        DetectionStatus::DataInsufficient => 1,
        DetectionStatus::Suspicious => 2,
        DetectionStatus::Indeterminate => 3,
        DetectionStatus::NoAnomalyDetected => 4,
    }
}

fn severity_rank(severity: Severity) -> u8 {
    match severity {
        Severity::High => 4,
        Severity::Medium => 3,
        Severity::Low => 2,
        Severity::Info => 1,
    }
}

fn option_value(value: Option<u64>) -> String {
    value.map_or_else(|| "未知".to_owned(), |value| value.to_string())
}

fn event_time_ms(event: &ProcessEvent) -> Option<i64> {
    event
        .etw
        .occurred_at
        .as_deref()
        .or(event.received_at.as_deref())
        .or(event.normalized_at.as_deref())
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc).timestamp_millis())
}

pub fn rule_catalog() -> Vec<RuleDefinition> {
    let config = AnalysisConfig::default();
    vec![
        RuleDefinition {
            id: "PROC-BURST-001".to_owned(),
            version: RULES_VERSION.to_owned(),
            title: "短时间内大量进程启动".to_owned(),
            description: "在滑动时间窗口内统计进程启动事件数量。".to_owned(),
            data_required: vec![
                "事件类型".to_owned(),
                "可用事件时间".to_owned(),
                "进程事件 ID".to_owned(),
            ],
            false_positive_notes: vec![
                "安装器、更新、构建脚本和登录脚本可能批量启动进程。".to_owned(),
            ],
            thresholds: vec![
                ThresholdValue {
                    name: "window_seconds".to_owned(),
                    value: config.burst_window_secs.to_string(),
                },
                ThresholdValue {
                    name: "start_count".to_owned(),
                    value: config.burst_start_threshold.to_string(),
                },
            ],
        },
        RuleDefinition {
            id: "PROC-EXIT-002".to_owned(),
            version: RULES_VERSION.to_owned(),
            title: "短时间内大量进程退出".to_owned(),
            description: "在滑动时间窗口内统计进程退出事件数量。".to_owned(),
            data_required: vec!["事件类型".to_owned(), "可用事件时间".to_owned()],
            false_positive_notes: vec![
                "应用关闭、用户注销和进程树清理可能集中产生退出事件。".to_owned(),
            ],
            thresholds: vec![
                ThresholdValue {
                    name: "window_seconds".to_owned(),
                    value: config.exit_window_secs.to_string(),
                },
                ThresholdValue {
                    name: "stop_count".to_owned(),
                    value: config.exit_threshold.to_string(),
                },
            ],
        },
        RuleDefinition {
            id: "PROC-ORPHAN-003".to_owned(),
            version: RULES_VERSION.to_owned(),
            title: "进程生命周期配对覆盖不足".to_owned(),
            description: "统计无法可靠配对的启动/退出事件比例，描述数据完整性。".to_owned(),
            data_required: vec!["配对状态".to_owned(), "事件类型".to_owned()],
            false_positive_notes: vec![
                "会话边界的退出事件天然缺少启动事件。".to_owned(),
                "ETW 丢失或传感器重启会造成配对缺口。".to_owned(),
            ],
            thresholds: vec![
                ThresholdValue {
                    name: "min_events".to_owned(),
                    value: config.orphan_min_events.to_string(),
                },
                ThresholdValue {
                    name: "unpaired_ratio_percent".to_owned(),
                    value: config.orphan_ratio_percent.to_string(),
                },
            ],
        },
        RuleDefinition {
            id: "PROC-INTEGRITY-004".to_owned(),
            version: RULES_VERSION.to_owned(),
            title: "进程生命周期一致性异常".to_owned(),
            description: "报告重复启动、重复退出和同 PID 身份不匹配事件。".to_owned(),
            data_required: vec!["配对状态与原因".to_owned(), "进程身份".to_owned()],
            false_positive_notes: vec![
                "PID 复用是正常 Windows 行为。".to_owned(),
                "ETW 重传或传感器重启可能产生重复事件。".to_owned(),
            ],
            thresholds: Vec::new(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_events::{
        DataCompleteness, EtwEventInfo, EventSource, FieldState, IdentityCertainty, PairingOrder,
        ProcessIdentity,
    };

    fn base_event(sequence: u64, event_type: ProcessEventType) -> ProcessEvent {
        let offset = sequence as i64;
        let time = 1_790_000_000_000 + offset * 100;
        ProcessEvent {
            event_id: format!("session:{sequence:016x}"),
            sequence,
            session_id: "session".to_owned(),
            event_type,
            source: EventSource::MicrosoftWindowsKernelProcess,
            identity: ProcessIdentity {
                pid: 100 + sequence as u32,
                create_time: 1,
            },
            identity_certainty: IdentityCertainty::Exact,
            parent_pid: Some(4),
            image_name: Some("test.exe".to_owned()),
            etw: EtwEventInfo {
                event_id: if event_type == ProcessEventType::Started {
                    1
                } else {
                    2
                },
                version: 4,
                provider_sequence: Some(sequence),
                timestamp_100ns: None,
                occurred_at: unix_ms_to_rfc3339(time as u64),
            },
            process_started_at: None,
            received_at: None,
            normalized_at: None,
            completeness: DataCompleteness {
                identity_certain: true,
                event_time_known: true,
                received_time_known: false,
                normalized_time_known: false,
                parent_pid_available: true,
                image_name: FieldState::Present,
                etw_sequence_available: true,
                issues: Vec::new(),
            },
            pairing: PairingStatus::Waiting,
        }
    }

    fn paired_event(sequence: u64, event_type: ProcessEventType) -> ProcessEvent {
        let mut event = base_event(sequence, event_type);
        event.pairing = PairingStatus::Paired {
            start_event_id: "session:0000000000000001".to_owned(),
            stop_event_id: "session:0000000000000002".to_owned(),
            order: PairingOrder::InOrder,
            sequence_match: Some(true),
        };
        event
    }

    fn context() -> DetectionContext {
        DetectionContext {
            session_id: "session".to_owned(),
            session_closed: true,
            session_end_reason: Some("completed".to_owned()),
            invalid_records: 0,
            ingestion_gaps: 0,
            truncated: false,
            sensor: Some(SensorHealthInput {
                healthy: true,
                etw_events_lost: Some(0),
                realtime_buffers_lost: Some(0),
                pipeline_dropped: 0,
            }),
            session_data: None,
        }
    }

    fn normal_events() -> Vec<ProcessEvent> {
        let mut events = Vec::new();
        for index in 0..6 {
            events.push(paired_event(index * 2 + 1, ProcessEventType::Started));
            events.push(paired_event(index * 2 + 2, ProcessEventType::Stopped));
        }
        events
    }

    #[test]
    fn final_session_data_makes_etw_loss_known_without_live_sensor() {
        let mut context = context();
        context.sensor = None;
        context.session_data = Some(SessionDataInput {
            etw_events_lost: Some(0),
            realtime_buffers_lost: Some(0),
            pipeline_dropped: 0,
            pipeline_invalid: 0,
            pipeline_backlog: 0,
            pipeline_shutdown_timed_out: false,
        });
        let analysis = analyze(&normal_events(), &context, &AnalysisConfig::default());
        assert_eq!(analysis.input.etw_events_lost, Some(0));
        assert!(
            !analysis
                .results
                .iter()
                .any(|result| result.rule_id == "DATA-ETW-UNKNOWN-001"),
            "persisted final stats must replace the live-health requirement"
        );
    }

    #[test]
    fn final_unknown_etw_does_not_fall_back_to_live_sensor() {
        let mut context = context();
        context.session_data = Some(SessionDataInput {
            etw_events_lost: None,
            realtime_buffers_lost: None,
            pipeline_dropped: 0,
            pipeline_invalid: 0,
            pipeline_backlog: 0,
            pipeline_shutdown_timed_out: false,
        });
        let analysis = analyze(&normal_events(), &context, &AnalysisConfig::default());
        assert_eq!(analysis.input.etw_events_lost, None);
        assert!(
            analysis
                .results
                .iter()
                .any(|result| result.rule_id == "DATA-ETW-UNKNOWN-001")
        );
    }

    #[test]
    fn final_session_data_with_known_loss_is_data_insufficient() {
        let mut context = context();
        context.sensor = None;
        context.session_data = Some(SessionDataInput {
            etw_events_lost: Some(4),
            realtime_buffers_lost: Some(0),
            pipeline_dropped: 0,
            pipeline_invalid: 0,
            pipeline_backlog: 0,
            pipeline_shutdown_timed_out: false,
        });
        let analysis = analyze(&normal_events(), &context, &AnalysisConfig::default());
        assert_eq!(analysis.overall, DetectionStatus::DataInsufficient);
        assert!(
            analysis
                .results
                .iter()
                .any(|result| result.rule_id == "DATA-LOSS-001")
        );
    }

    #[test]
    fn final_session_invalid_events_are_a_known_loss() {
        let mut context = context();
        context.sensor = None;
        context.session_data = Some(SessionDataInput {
            etw_events_lost: Some(0),
            realtime_buffers_lost: Some(0),
            pipeline_dropped: 0,
            pipeline_invalid: 1,
            pipeline_backlog: 0,
            pipeline_shutdown_timed_out: false,
        });
        let analysis = analyze(&normal_events(), &context, &AnalysisConfig::default());
        assert_eq!(analysis.input.invalid_records, 1);
        assert_eq!(analysis.overall, DetectionStatus::DataInsufficient);
        assert!(
            analysis
                .results
                .iter()
                .any(|result| result.rule_id == "DATA-LOSS-001")
        );
    }

    #[test]
    fn final_session_data_with_pipeline_gap_is_data_insufficient() {
        let mut context = context();
        context.sensor = None;
        context.session_data = Some(SessionDataInput {
            etw_events_lost: Some(0),
            realtime_buffers_lost: Some(0),
            pipeline_dropped: 2,
            pipeline_invalid: 0,
            pipeline_backlog: 1,
            pipeline_shutdown_timed_out: true,
        });
        let analysis = analyze(&normal_events(), &context, &AnalysisConfig::default());
        assert_eq!(analysis.overall, DetectionStatus::DataInsufficient);
        assert!(
            analysis
                .results
                .iter()
                .any(|result| result.rule_id == "DATA-LOSS-001")
        );
        assert_eq!(analysis.input.pipeline_dropped, Some(2));
        assert_eq!(analysis.input.pipeline_backlog, Some(1));
        assert_eq!(analysis.input.pipeline_shutdown_timed_out, Some(true));
    }

    #[test]
    fn final_session_data_with_unknown_etw_loss_stays_indeterminate() {
        let mut context = context();
        context.sensor = None;
        context.session_data = Some(SessionDataInput {
            etw_events_lost: None,
            realtime_buffers_lost: None,
            pipeline_dropped: 0,
            pipeline_invalid: 0,
            pipeline_backlog: 0,
            pipeline_shutdown_timed_out: false,
        });
        let analysis = analyze(&normal_events(), &context, &AnalysisConfig::default());
        assert_eq!(analysis.overall, DetectionStatus::Indeterminate);
        assert!(
            analysis
                .results
                .iter()
                .any(|result| result.rule_id == "DATA-ETW-UNKNOWN-001")
        );
    }

    #[test]
    fn a_small_normal_session_is_not_reported_as_an_anomaly() {
        let analysis = analyze(&normal_events(), &context(), &AnalysisConfig::default());
        assert_eq!(analysis.overall, DetectionStatus::NoAnomalyDetected);
        assert!(analysis.results.is_empty());
    }

    #[test]
    fn a_start_burst_is_suspicious_but_not_malicious() {
        let events: Vec<ProcessEvent> = (1..=30)
            .map(|index| base_event(index, ProcessEventType::Started))
            .collect();
        let analysis = analyze(&events, &context(), &AnalysisConfig::default());
        assert_eq!(analysis.overall, DetectionStatus::Suspicious);
        let burst = analysis
            .results
            .iter()
            .find(|result| result.rule_id == "PROC-BURST-001")
            .expect("burst result");
        assert_eq!(burst.status, DetectionStatus::Suspicious);
        assert_eq!(burst.affected_event_count, 30);
        assert!(!burst.explanation.contains("恶意软件"));
    }

    #[test]
    fn a_normal_session_with_unknown_etw_loss_is_indeterminate() {
        let mut context = context();
        context.sensor = None;
        let analysis = analyze(&normal_events(), &context, &AnalysisConfig::default());
        assert_eq!(analysis.overall, DetectionStatus::Indeterminate);
        assert!(
            analysis
                .results
                .iter()
                .any(|result| result.rule_id == "DATA-ETW-UNKNOWN-001")
        );
    }

    #[test]
    fn known_loss_is_data_insufficient_even_without_behavior_anomalies() {
        let mut context = context();
        context.sensor = Some(SensorHealthInput {
            healthy: false,
            etw_events_lost: Some(4),
            realtime_buffers_lost: Some(0),
            pipeline_dropped: 1,
        });
        let analysis = analyze(&normal_events(), &context, &AnalysisConfig::default());
        assert_eq!(analysis.overall, DetectionStatus::DataInsufficient);
        assert!(
            analysis
                .results
                .iter()
                .any(|result| result.rule_id == "DATA-LOSS-001")
        );
    }

    #[test]
    fn an_open_session_is_data_insufficient() {
        let mut context = context();
        context.session_closed = false;
        let analysis = analyze(&normal_events(), &context, &AnalysisConfig::default());
        assert_eq!(analysis.overall, DetectionStatus::DataInsufficient);
    }

    #[test]
    fn too_few_events_are_data_insufficient_not_normal() {
        let analysis = analyze(
            &normal_events()[..6],
            &context(),
            &AnalysisConfig::default(),
        );
        assert_eq!(analysis.overall, DetectionStatus::DataInsufficient);
        assert!(
            analysis
                .results
                .iter()
                .any(|result| result.rule_id == "DATA-SMALL-001")
        );
    }

    #[test]
    fn orphan_lifecycle_is_reported_as_a_data_anomaly() {
        let mut events = Vec::new();
        for index in 1..=20 {
            let mut event = base_event(index, ProcessEventType::Stopped);
            event.pairing = PairingStatus::Unpaired {
                reason: UnpairedReason::StopWithoutObservedStart,
            };
            events.push(event);
        }
        let analysis = analyze(&events, &context(), &AnalysisConfig::default());
        assert_eq!(analysis.overall, DetectionStatus::Anomaly);
        let orphan = analysis
            .results
            .iter()
            .find(|result| result.rule_id == "PROC-ORPHAN-003")
            .expect("orphan result");
        assert_eq!(orphan.affected_event_count, 20);
        assert!(orphan.explanation.contains("数据完整性"));
    }

    #[test]
    fn duplicate_lifecycle_events_are_an_integrity_anomaly() {
        let mut events = normal_events();
        let mut duplicate = base_event(100, ProcessEventType::Started);
        duplicate.pairing = PairingStatus::Unpaired {
            reason: UnpairedReason::DuplicateStart {
                first_event_id: "session:0000000000000001".to_owned(),
            },
        };
        events.push(duplicate);
        let analysis = analyze(&events, &context(), &AnalysisConfig::default());
        assert_eq!(analysis.overall, DetectionStatus::Anomaly);
        assert!(
            analysis
                .results
                .iter()
                .any(|result| result.rule_id == "PROC-INTEGRITY-004")
        );
    }

    #[test]
    fn evidence_ids_are_bounded() {
        let config = AnalysisConfig {
            max_evidence_ids: 5,
            ..AnalysisConfig::default()
        };
        let events: Vec<ProcessEvent> = (1..=30)
            .map(|index| base_event(index, ProcessEventType::Started))
            .collect();
        let analysis = analyze(&events, &context(), &config);
        let burst = analysis
            .results
            .iter()
            .find(|result| result.rule_id == "PROC-BURST-001")
            .expect("burst");
        assert_eq!(burst.evidence[0].event_ids.len(), 5);
        assert_eq!(burst.affected_event_count, 30);
    }

    #[test]
    fn missing_timestamps_prevent_a_normal_claim() {
        let mut events = normal_events();
        for event in &mut events {
            event.etw.occurred_at = None;
        }
        let analysis = analyze(&events, &context(), &AnalysisConfig::default());
        assert_ne!(analysis.overall, DetectionStatus::NoAnomalyDetected);
        assert!(
            analysis
                .results
                .iter()
                .any(|result| result.rule_id == "DATA-TIME-001")
        );
    }

    #[test]
    fn catalog_rules_have_versions_and_false_positive_notes() {
        let catalog = rule_catalog();
        assert!(catalog.len() >= 4);
        for rule in catalog {
            assert_eq!(rule.version, RULES_VERSION);
            assert!(!rule.false_positive_notes.is_empty());
        }
    }

    #[test]
    fn analysis_config_validation_rejects_unsafe_thresholds() {
        let mut config = AnalysisConfig::default();
        assert!(config.validate().is_ok());
        config.orphan_ratio_percent = 0;
        assert!(config.validate().is_err());
    }
}
