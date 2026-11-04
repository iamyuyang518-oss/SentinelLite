import { useEffect, useMemo, useRef, useState } from "react";

import { useSentinelApi } from "../components/ApiProvider";
import { DatePicker } from "../components/DatePicker";
import { ExpandableSection } from "../components/ExpandableSection";
import { getUserFacingApiError } from "../lib/api";
import { parseDateBoundary } from "../lib/dateRange";
import { formatCheckedAt } from "../lib/presentation";
import {
  completenessIssues,
  completenessLabel,
  eventTimeLabel,
  eventTimeSource,
  eventTypeLabel,
  identityCertaintyLabel,
  imagePathLabel,
  pairingPresentation,
  parentPidLabel,
  processDisplayName,
  processStartedAtLabel,
  type PairingPresentation,
} from "../lib/processEvents";
import { useLiveEvents } from "../lib/useLiveEvents";
import { useSensorStatus } from "../lib/useSensorStatus";
import type {
  DetectionResultWire,
  DetectionStatusWire,
  EventDetailWire,
  EventQueryResultWire,
  PairingRevisionWire,
  ProcessEventWire,
  SessionAnalysisWire,
  SessionEndWire,
  SessionRecordWire,
  StorageRuntimeReport,
  StoredEventFilter,
  StoredEventWire,
} from "../types";

interface FormState {
  from: string;
  to: string;
  image: string;
  pid: string;
  eventType: "all" | "started" | "stopped";
  sessionId: string;
}

interface HistoryQuery {
  filter: StoredEventFilter;
  offset: number;
  nonce: number;
}

interface Anomaly {
  tone: "critical" | "caution" | "notice";
  title: string;
  detail: string;
}

const EMPTY_FORM: FormState = {
  from: "",
  to: "",
  image: "",
  pid: "",
  eventType: "all",
  sessionId: "",
};

const HISTORY_LIMIT = 100;

function timeValue(value: string | null): string {
  return value === null ? "未提供" : formatCheckedAt(value);
}

function shortSession(sessionId: string | null): string {
  if (sessionId === null || sessionId.length === 0) {
    return "未知";
  }
  return sessionId.length > 18 ? `${sessionId.slice(0, 18)}…` : sessionId;
}

function PairingBadge({ pairing }: { pairing: PairingPresentation }) {
  return (
    <span
      className={`pairing-badge pairing-badge--${pairing.tone}`}
      title={pairing.detail}
    >
      {pairing.label}
    </span>
  );
}

function EventTypeMark({ event }: { event: ProcessEventWire }) {
  return (
    <span className={`event-type event-type--${event.event_type}`}>
      <span className="event-type__mark" aria-hidden="true" />
      {eventTypeLabel(event.event_type)}
    </span>
  );
}

function CompletenessDetail({ event }: { event: ProcessEventWire }) {
  const issues = completenessIssues(event);
  return (
    <div className="detail-block">
      <h4>字段完整性</h4>
      <p className="detail-emphasis">{completenessLabel(event)}</p>
      {issues.length === 0 ? (
        <p className="detail-muted">未记录缺失字段；这不表示事件内容一定正确。</p>
      ) : (
        <ul className="issue-list">
          {issues.map((issue) => (
            <li key={issue}>{issue}</li>
          ))}
        </ul>
      )}
    </div>
  );
}

function PairingDetail({ event }: { event: ProcessEventWire }) {
  const presentation = pairingPresentation(event.pairing);
  return (
    <div className="detail-block">
      <h4>配对状态</h4>
      <p className="detail-emphasis">
        <PairingBadge pairing={presentation} />
      </p>
      <p className="detail-muted">{presentation.detail}</p>
      {event.pairing.state === "paired" ? (
        <dl className="detail-grid">
          <div>
            <dt>启动事件</dt>
            <dd className="mono">{event.pairing.start_event_id}</dd>
          </div>
          <div>
            <dt>退出事件</dt>
            <dd className="mono">{event.pairing.stop_event_id}</dd>
          </div>
          <div>
            <dt>顺序校验</dt>
            <dd>
              {event.pairing.sequence_match === null
                ? "无法校验（缺少 ETW 序列号）"
                : event.pairing.sequence_match
                  ? "一致"
                  : "不一致"}
            </dd>
          </div>
        </dl>
      ) : null}
      {event.pairing.state === "unpaired" &&
      event.pairing.reason.reason === "identity_mismatch" ? (
        <p className="detail-muted">
          当前同 PID 的打开身份创建时间：
          {event.pairing.reason.open_create_time ?? "未提供"}
        </p>
      ) : null}
    </div>
  );
}

function EventCoreDetail({ event }: { event: ProcessEventWire }) {
  return (
    <>
      <div className="detail-block">
        <h4>事件时间</h4>
        <dl className="detail-grid">
          <div>
            <dt>ETW 事件时间</dt>
            <dd>{timeValue(event.etw.occurred_at)}</dd>
          </div>
          <div>
            <dt>传感器接收时间</dt>
            <dd>{timeValue(event.received_at)}</dd>
          </div>
          <div>
            <dt>标准化时间</dt>
            <dd>{timeValue(event.normalized_at)}</dd>
          </div>
          <div>
            <dt>进程启动时间</dt>
            <dd>{processStartedAtLabel(event)}</dd>
          </div>
        </dl>
        <p className="detail-muted">列表主时间取自：{eventTimeSource(event)}。</p>
      </div>

      <div className="detail-block">
        <h4>进程身份</h4>
        <dl className="detail-grid">
          <div>
            <dt>进程名称</dt>
            <dd>{processDisplayName(event.image_name)}</dd>
          </div>
          <div>
            <dt>PID</dt>
            <dd className="mono">{event.identity.pid}</dd>
          </div>
          <div>
            <dt>父 PID</dt>
            <dd className="mono">{parentPidLabel(event)}</dd>
          </div>
          <div>
            <dt>创建时间令牌</dt>
            <dd className="mono">{event.identity.create_time}</dd>
          </div>
          <div className="detail-grid__wide">
            <dt>映像路径</dt>
            <dd className="mono">{imagePathLabel(event.image_name)}</dd>
          </div>
        </dl>
        <p className="detail-muted">{identityCertaintyLabel(event)}</p>
        <p className="detail-muted">
          {event.identity_certainty === "exact"
            ? "身份由 PID 与进程创建时间共同确定。"
            : "创建时间缺失；不会仅凭 PID 将该事件与其他事件配对。"}
        </p>
      </div>

      <div className="detail-block">
        <h4>ETW 来源</h4>
        <dl className="detail-grid">
          <div>
            <dt>事件 ID / 版本</dt>
            <dd className="mono">
              {event.etw.event_id} / v{event.etw.version}
            </dd>
          </div>
          <div>
            <dt>提供程序序列号</dt>
            <dd className="mono">{event.etw.provider_sequence ?? "未提供"}</dd>
          </div>
          <div className="detail-grid__wide">
            <dt>会话</dt>
            <dd className="mono">{event.session_id}</dd>
          </div>
          <div className="detail-grid__wide">
            <dt>事件 ID</dt>
            <dd className="mono">{event.event_id}</dd>
          </div>
        </dl>
      </div>

      <PairingDetail event={event} />
      <CompletenessDetail event={event} />
    </>
  );
}

const DETECTION_STATUS_LABELS: Record<DetectionStatusWire, string> = {
  no_anomaly_detected: "未发现规则覆盖的异常",
  suspicious: "可疑模式",
  anomaly: "可验证异常",
  data_insufficient: "数据不足",
  indeterminate: "无法判断",
};

const DETECTION_SEVERITY_LABELS = {
  info: "信息",
  low: "低",
  medium: "中",
  high: "高",
} as const;

const DETECTION_CONFIDENCE_LABELS = {
  low: "低",
  medium: "中",
  high: "高",
} as const;

function detectionStatusTone(status: DetectionStatusWire): string {
  switch (status) {
    case "anomaly":
      return "critical";
    case "suspicious":
      return "caution";
    case "data_insufficient":
    case "indeterminate":
      return "unknown";
    case "no_anomaly_detected":
      return "good";
  }
}

function DetectionResultCard({
  result,
  onSelectEvent,
}: {
  result: DetectionResultWire;
  onSelectEvent: (eventId: string) => void;
}) {
  return (
    <article className={`detection-result detection-result--${detectionStatusTone(result.status)}`}>
      <header className="detection-result__header">
        <span className={`detection-status detection-status--${detectionStatusTone(result.status)}`}>
          {DETECTION_STATUS_LABELS[result.status]}
        </span>
        <span className="detection-meta">
          严重程度 {DETECTION_SEVERITY_LABELS[result.severity]} · 置信度{" "}
          {DETECTION_CONFIDENCE_LABELS[result.confidence]}
        </span>
        <span className="detection-rule mono">
          {result.rule_id} · v{result.rule_version}
        </span>
      </header>
      <h3>{result.title}</h3>
      <p>{result.explanation}</p>
      {result.window_start === null || result.window_end === null ? null : (
        <p className="detail-muted">
          窗口：{formatCheckedAt(result.window_start)} — {formatCheckedAt(result.window_end)} ·
          影响 {result.affected_event_count} 条事件
        </p>
      )}
      {result.evidence.length === 0 ? null : (
        <div className="detection-evidence">
          <h4>证据</h4>
          {result.evidence.map((evidence, index) => (
            <div key={`${evidence.statement}-${index}`} className="detection-evidence__item">
              <p>
                {evidence.statement}
                {evidence.value === null ? null : `（${evidence.value}）`}
              </p>
              {evidence.event_ids.length === 0 ? null : (
                <div className="detection-evidence__ids">
                  {evidence.event_ids.map((eventId) => (
                    <button
                      key={eventId}
                      type="button"
                      className="event-id-link mono"
                      onClick={() => onSelectEvent(eventId)}
                    >
                      {eventId}
                    </button>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
      {result.thresholds.length === 0 ? null : (
        <p className="detail-muted">
          阈值：
          {result.thresholds.map((threshold) => `${threshold.name}=${threshold.value}`).join(" · ")}
        </p>
      )}
      {result.false_positive_notes.length === 0 ? null : (
        <div className="detection-false-positive">
          <h4>可能的误报来源</h4>
          <ul className="issue-list">
            {result.false_positive_notes.map((note) => (
              <li key={note}>{note}</li>
            ))}
          </ul>
        </div>
      )}
    </article>
  );
}

const SESSION_END_REASON_LABELS: Record<SessionEndWire["reason"], string> = {
  completed: "正常完成",
  interrupted: "被中断",
  error: "错误结束",
  shutdown: "服务关闭",
};

function optionalCount(value: number | null): string {
  return value === null ? "未知" : String(value);
}

function SessionEndDetail({ sessionEnd }: { sessionEnd: SessionEndWire | null }) {
  if (sessionEnd === null) {
    return (
      <div className="detail-block">
        <h4>会话结束</h4>
        <p className="detail-muted">
          未观察到 SessionEnd。会话可能仍在运行，或传感器异常中断；不能视为正常结束。
        </p>
      </div>
    );
  }
  const stats = sessionEnd.final_stats;
  return (
    <div className="detail-block">
      <h4>会话结束</h4>
      <dl className="detail-grid">
        <div>
          <dt>结束原因</dt>
          <dd>{SESSION_END_REASON_LABELS[sessionEnd.reason]}</dd>
        </div>
        <div>
          <dt>结束时间</dt>
          <dd>{timeValue(sessionEnd.ended_at)}</dd>
        </div>
        <div>
          <dt>未完成启动</dt>
          <dd>{sessionEnd.open_start_events.length} 条样本</dd>
        </div>
        <div>
          <dt>已配对</dt>
          <dd>{sessionEnd.pairing.paired}</dd>
        </div>
        <div>
          <dt>未配对启动</dt>
          <dd>{sessionEnd.pairing.unpaired_starts}</dd>
        </div>
        <div>
          <dt>待回补孤儿退出</dt>
          <dd>{sessionEnd.pairing.pending_orphan_stops}</dd>
        </div>
      </dl>
      {stats === null ? (
        <p className="detail-muted">
          该会话未包含最终统计（旧记录或结束流程未取得）。未知值不会当作 0。
        </p>
      ) : (
        <>
          <h5>最终统计</h5>
          <dl className="detail-grid">
            <div>
              <dt>ETW 事件丢失</dt>
              <dd>{optionalCount(stats.events_lost)}</dd>
            </div>
            <div>
              <dt>实时缓冲丢失</dt>
              <dd>{optionalCount(stats.realtime_buffers_lost)}</dd>
            </div>
            <div>
              <dt>管道接收 / 消费 / 标准化</dt>
              <dd>
                {stats.pipeline_received} / {stats.pipeline_consumed} /{" "}
                {stats.pipeline_normalized}
              </dd>
            </div>
            <div>
              <dt>队列满 / 消费者消失丢弃</dt>
              <dd>
                {stats.pipeline_dropped_queue_full} /{" "}
                {stats.pipeline_dropped_consumer_gone}
              </dd>
            </div>
            <div>
              <dt>无法解码</dt>
              <dd>{stats.pipeline_invalid}</dd>
            </div>
            <div>
              <dt>过滤忽略</dt>
              <dd>{stats.pipeline_filtered_ignored}</dd>
            </div>
            <div>
              <dt>最终积压 / 峰值</dt>
              <dd>
                {stats.pipeline_backlog} / {stats.pipeline_peak_backlog}
              </dd>
            </div>
            <div>
              <dt>关闭超时</dt>
              <dd>
                {stats.pipeline_shutdown_timed_out ? "是" : "否"}
                {stats.pipeline_backlog_at_shutdown_timeout === null
                  ? ""
                  : `（超时积压 ${stats.pipeline_backlog_at_shutdown_timeout}）`}
              </dd>
            </div>
          </dl>
          <p className="detail-muted">
            最终积压与关闭超时反映传感器退出时的有界队列状态；ETW 丢失与管道丢弃是不同计数。
          </p>
        </>
      )}
      {sessionEnd.error === null ? null : (
        <p className="detail-alert">传感器报告的结束错误：{sessionEnd.error}</p>
      )}
      {sessionEnd.open_start_events.length === 0 ? null : (
        <ul className="issue-list">
          {sessionEnd.open_start_events.map((eventId) => (
            <li key={eventId} className="mono">
              {eventId}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function StoredEventSummary({
  event,
  onSelect,
}: {
  event: StoredEventWire;
  onSelect: (eventId: string) => void;
}) {
  const record = event.record;
  return (
    <button
      type="button"
      className="paired-event"
      onClick={() => onSelect(event.event_id)}
    >
      <span className="paired-event__type">
        <EventTypeMark event={record} />
      </span>
      <span>
        <strong>{processDisplayName(record.image_name)}</strong>
        <span className="paired-event__meta">
          {eventTimeLabel(record)} · PID {record.identity.pid}
        </span>
      </span>
    </button>
  );
}

function StoredDetail({
  detail,
  onSelectEvent,
}: {
  detail: EventDetailWire;
  onSelectEvent: (eventId: string) => void;
}) {
  const event = detail.event.record;
  return (
    <div className="event-detail">
      <header className="event-detail__header">
        <EventTypeMark event={event} />
        <h3>{processDisplayName(event.image_name)}</h3>
        <PairingBadge pairing={pairingPresentation(event.pairing)} />
      </header>
      <p className="detail-muted">
        数据库记录 · {timeValue(detail.event.event_time)}
      </p>
      <EventCoreDetail event={event} />

      <div className="detail-block">
        <h4>配对事件</h4>
        {detail.paired_event === null ? (
          <p className="detail-muted">
            {event.pairing.state === "paired"
              ? "配对事件不在本地数据库中，可能已被保留策略删除。"
              : "当前没有可用的配对事件。"}
          </p>
        ) : (
          <StoredEventSummary event={detail.paired_event} onSelect={onSelectEvent} />
        )}
      </div>

      <div className="detail-block">
        <h4>配对修正历史</h4>
        {detail.revisions.length === 0 ? (
          <p className="detail-muted">未记录配对修正。</p>
        ) : (
          <ul className="revision-list">
            {detail.revisions.map((revision) => (
              <li key={`${revision.stop_event_id}-${revision.start_event_id}`}>
                <strong>{revision.reason}</strong>
                <span className="mono">{revision.stop_event_id}</span>
                <span className="mono">← {revision.start_event_id}</span>
                <span className="detail-muted">
                  {revision.normalized_at === null
                    ? "时间未提供"
                    : formatCheckedAt(revision.normalized_at)}
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>

      <SessionEndDetail sessionEnd={detail.session_end} />
    </div>
  );
}

function LiveDetail({
  event,
  events,
  revisions,
  onSelectEvent,
}: {
  event: ProcessEventWire;
  events: ProcessEventWire[];
  revisions: PairingRevisionWire[];
  onSelectEvent: (eventId: string) => void;
}) {
  const pairedId =
    event.pairing.state === "paired"
      ? event.pairing.start_event_id === event.event_id
        ? event.pairing.stop_event_id
        : event.pairing.start_event_id
      : null;
  const paired = pairedId === null
    ? null
    : events.find((candidate) => candidate.event_id === pairedId) ?? null;

  return (
    <div className="event-detail">
      <header className="event-detail__header">
        <EventTypeMark event={event} />
        <h3>{processDisplayName(event.image_name)}</h3>
        <PairingBadge pairing={pairingPresentation(event.pairing)} />
      </header>
      <p className="detail-muted">实时事件 · {eventTimeLabel(event)}</p>
      <EventCoreDetail event={event} />

      <div className="detail-block">
        <h4>配对事件</h4>
        {paired === null ? (
          <p className="detail-muted">
            {event.pairing.state === "waiting"
              ? "退出事件尚未到达。"
              : event.pairing.state === "paired"
                ? "配对事件不在当前实时列表的保留范围内。"
                : "当前没有可用的配对事件。"}
          </p>
        ) : (
          <button
            type="button"
            className="paired-event"
            onClick={() => onSelectEvent(paired.event_id)}
          >
            <span className="paired-event__type">
              <EventTypeMark event={paired} />
            </span>
            <span>
              <strong>{processDisplayName(paired.image_name)}</strong>
              <span className="paired-event__meta">
                {eventTimeLabel(paired)} · PID {paired.identity.pid}
              </span>
            </span>
          </button>
        )}
      </div>

      <div className="detail-block">
        <h4>配对修正</h4>
        {revisions.length === 0 ? (
          <p className="detail-muted">当前会话未收到配对修正事件。</p>
        ) : (
          <p className="detail-muted">
            已收到并应用 {revisions.length} 条配对修正；列表中的退出事件已更新。
          </p>
        )}
      </div>
    </div>
  );
}

function AnomalyStack({ anomalies }: { anomalies: Anomaly[] }) {
  if (anomalies.length === 0) {
    return null;
  }
  return (
    <div className="anomaly-stack" aria-label="事件数据异常与不确定项">
      {anomalies.map((anomaly) => (
        <section
          key={`${anomaly.tone}-${anomaly.title}`}
          className={`anomaly anomaly--${anomaly.tone}`}
        >
          <h2>{anomaly.title}</h2>
          <p>{anomaly.detail}</p>
        </section>
      ))}
    </div>
  );
}

function LiveEventRow({
  event,
  selected,
  onSelect,
}: {
  event: ProcessEventWire;
  selected: boolean;
  onSelect: (eventId: string) => void;
}) {
  return (
    <button
      type="button"
      className={`event-simple-row${selected ? " is-selected" : ""}`}
      onClick={() => onSelect(event.event_id)}
      aria-pressed={selected}
      aria-label={`${eventTypeLabel(event.event_type)} ${processDisplayName(event.image_name)} PID ${event.identity.pid}`}
    >
      <span className="event-simple-row__name">
        <strong>{processDisplayName(event.image_name)}</strong>
      </span>
      <span className="event-simple-row__type">
        <EventTypeMark event={event} />
      </span>
      <span className="event-simple-row__time">{eventTimeLabel(event)}</span>
      <span className="event-simple-row__pid">PID {event.identity.pid}</span>
    </button>
  );
}

function HistoryTable({
  result,
  onSelect,
}: {
  result: EventQueryResultWire;
  onSelect: (eventId: string) => void;
}) {
  if (result.events.length === 0) {
    return <p className="empty-block">没有符合条件的事件。</p>;
  }
  return (
    <div className="history-table-wrap">
      <table className="history-table history-table--simple">
        <thead>
          <tr>
            <th scope="col">时间</th>
            <th scope="col">事件</th>
            <th scope="col">进程</th>
            <th scope="col">PID</th>
          </tr>
        </thead>
        <tbody>
          {result.events.map((event) => (
            <tr key={event.event_id}>
              <td>
                <button
                  type="button"
                  className="row-link"
                  onClick={() => onSelect(event.event_id)}
                  aria-label={`查看事件 ${event.event_id}`}
                >
                  {timeValue(event.event_time)}
                </button>
              </td>
              <td>
                <EventTypeMark event={event.record} />
              </td>
              <td>
                <strong>{processDisplayName(event.record.image_name)}</strong>
              </td>
              <td className="table-pid">{event.record.identity.pid}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function buildFilter(form: FormState, offset: number): StoredEventFilter | string {
  const filter: StoredEventFilter = { limit: HISTORY_LIMIT, offset };
  if (form.from.trim().length > 0) {
    const from = parseDateBoundary(form.from, false);
    if (from === null) {
      return "开始日期无效。";
    }
    filter.from = from.toISOString();
  }
  if (form.to.trim().length > 0) {
    const to = parseDateBoundary(form.to, true);
    if (to === null) {
      return "结束日期无效。";
    }
    filter.to = to.toISOString();
  }
  if (form.image.trim().length > 0) {
    filter.image = form.image.trim();
  }
  if (form.pid.trim().length > 0) {
    const pid = Number(form.pid);
    if (!Number.isInteger(pid) || pid <= 0) {
      return "PID 必须是正整数。";
    }
    filter.pid = pid;
  }
  if (form.eventType !== "all") {
    filter.eventType = form.eventType;
  }
  if (form.sessionId.length > 0) {
    filter.sessionId = form.sessionId;
  }
  return filter;
}

export function EventsPage() {
  const api = useSentinelApi();
  const live = useLiveEvents(api);
  const sensorQuery = useSensorStatus(api);
  const sensor = sensorQuery.state.report?.sensor ?? null;

  const [storage, setStorage] = useState<StorageRuntimeReport | null>(null);
  const [storageError, setStorageError] = useState<string | null>(null);
  const [sessions, setSessions] = useState<SessionRecordWire[]>([]);
  const [form, setForm] = useState<FormState>(EMPTY_FORM);
  const [historyQuery, setHistoryQuery] = useState<HistoryQuery>({
    filter: { limit: HISTORY_LIMIT, offset: 0 },
    offset: 0,
    nonce: 0,
  });
  const [history, setHistory] = useState<EventQueryResultWire | null>(null);
  const [historyLoading, setHistoryLoading] = useState(true);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [selectedEventId, setSelectedEventId] = useState<string | null>(null);
  const [storedDetail, setStoredDetail] = useState<EventDetailWire | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const detailRequest = useRef(0);
  const [analysisSessionId, setAnalysisSessionId] = useState("");
  const [analysis, setAnalysis] = useState<SessionAnalysisWire | null>(null);
  const [analysisLoading, setAnalysisLoading] = useState(false);
  const [analysisError, setAnalysisError] = useState<string | null>(null);

  const selectedAnalysisSession =
    analysisSessionId ||
    (live.state.raw.sessionClosed ? null : live.state.raw.sessionId) ||
    sessions[0]?.session_id ||
    "";

  const selectedLiveEvent = useMemo(
    () =>
      selectedEventId === null
        ? null
        : live.state.raw.events.find((event) => event.event_id === selectedEventId) ?? null,
    [live.state.raw.events, selectedEventId],
  );

  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;

    async function load(): Promise<void> {
      try {
        const report = await api.getStorageStatus();
        const sessionList = await api.listSensorSessions(50);
        if (!cancelled) {
          setStorage(report);
          setStorageError(null);
          setSessions(sessionList);
        }
      } catch (error) {
        if (!cancelled) {
          setStorageError(getUserFacingApiError(error));
        }
      } finally {
        if (!cancelled) {
          timer = window.setTimeout(() => {
            void load();
          }, 5_000);
        }
      }
    }

    void load();
    return () => {
      cancelled = true;
      if (timer !== undefined) {
        window.clearTimeout(timer);
      }
    };
  }, [api]);

  useEffect(() => {
    let cancelled = false;
    api.queryStoredEvents(historyQuery.filter).then(
      (result) => {
        if (!cancelled) {
          setHistory(result);
          setHistoryLoading(false);
        }
      },
      (error: unknown) => {
        if (!cancelled) {
          setHistory(null);
          setHistoryError(getUserFacingApiError(error));
          setHistoryLoading(false);
        }
      },
    );
    return () => {
      cancelled = true;
    };
  }, [api, historyQuery]);

  function runSearch(): void {
    const filter = buildFilter(form, 0);
    if (typeof filter === "string") {
      setFormError(filter);
      return;
    }
    setFormError(null);
    setHistoryLoading(true);
    setHistoryError(null);
    setHistoryQuery((current) => ({
      filter,
      offset: 0,
      nonce: current.nonce + 1,
    }));
  }

  function resetSearch(): void {
    setForm(EMPTY_FORM);
    setFormError(null);
    setHistoryLoading(true);
    setHistoryError(null);
    setHistoryQuery((current) => ({
      filter: { limit: HISTORY_LIMIT, offset: 0 },
      offset: 0,
      nonce: current.nonce + 1,
    }));
  }

  function changePage(direction: -1 | 1): void {
    const nextOffset = Math.max(
      0,
      historyQuery.offset + direction * HISTORY_LIMIT,
    );
    const filter = { ...historyQuery.filter, offset: nextOffset };
    setHistoryLoading(true);
    setHistoryError(null);
    setHistoryQuery((current) => ({
      filter,
      offset: nextOffset,
      nonce: current.nonce + 1,
    }));
  }

  async function selectStoredEvent(eventId: string): Promise<void> {
    const request = detailRequest.current + 1;
    detailRequest.current = request;
    setSelectedEventId(eventId);
    setStoredDetail(null);
    setDetailError(null);
    setDetailLoading(true);
    try {
      const detail = await api.getStoredEvent(eventId);
      if (detailRequest.current !== request) {
        return;
      }
      if (detail === null) {
        setDetailError("该事件不在本地数据库中，可能已被保留策略删除。");
      } else {
        setStoredDetail(detail);
      }
    } catch (error) {
      if (detailRequest.current === request) {
        setDetailError(getUserFacingApiError(error));
      }
    } finally {
      if (detailRequest.current === request) {
        setDetailLoading(false);
      }
    }
  }

  function selectLiveEvent(eventId: string): void {
    detailRequest.current += 1;
    setSelectedEventId(eventId);
    setStoredDetail(null);
    setDetailError(null);
    setDetailLoading(false);
  }

  async function runAnalysis(): Promise<void> {
    if (selectedAnalysisSession.length === 0) {
      setAnalysisError("尚未选择传感器会话。");
      return;
    }
    setAnalysisLoading(true);
    setAnalysisError(null);
    try {
      const result = await api.analyzeSession(selectedAnalysisSession);
      setAnalysis(result);
    } catch (error) {
      setAnalysis(null);
      setAnalysisError(getUserFacingApiError(error));
    } finally {
      setAnalysisLoading(false);
    }
  }

  const unpairedCount = live.state.raw.events.filter(
    (event) => event.pairing.state === "unpaired",
  ).length;
  const indeterminateCount = live.state.raw.events.filter(
    (event) => event.identity_certainty === "indeterminate",
  ).length;

  const anomalies: Anomaly[] = [];
  if (live.state.phase === "error") {
    anomalies.push({
      tone: "critical",
      title: "无法读取实时事件",
      detail: live.state.error ?? "事件通道读取失败；实时列表可能停留在旧数据。",
    });
  }
  if (
    live.state.linkState === "stopped" ||
    live.state.linkState === "unavailable" ||
    live.state.linkState === "communicationFailed"
  ) {
    anomalies.push({
      tone: "caution",
      title: "传感器未连接",
      detail:
        "实时列表不会继续更新。此前显示的事件仍然保留，但不能当作当前完整的进程活动。",
    });
  }
  if (live.state.raw.cursorExpiredCount > 0) {
    anomalies.push({
      tone: "caution",
      title: "事件投递窗口已覆盖",
      detail: `读取游标落后于传感器保留窗口，至少缺少 ${live.state.raw.missedRecords} 条记录。列表不是连续的。`,
    });
  }
  if (live.state.raw.invalidRecords > 0) {
    anomalies.push({
      tone: "caution",
      title: "存在无法解码的事件",
      detail: `${live.state.raw.invalidRecords} 条原始 ETW 事件未能标准化；它们已被保留为异常记录，不会显示为正常进程事件。`,
    });
  }
  if (live.state.raw.trimmed > 0) {
    anomalies.push({
      tone: "notice",
      title: "实时列表已截断",
      detail: `为限制内存，仅保留最近 ${live.state.raw.events.length} 条事件，已移除 ${live.state.raw.trimmed} 条旧显示记录。完整历史请查询本地数据库。`,
    });
  }
  if (live.state.raw.restarted) {
    anomalies.push({
      tone: "notice",
      title: "检测到新的传感器会话",
      detail: "传感器重启后事件 ID 和游标会重新开始；实时列表已切换到新会话。",
    });
  }
  if (sensor?.events_lost === null) {
    anomalies.push({
      tone: "caution",
      title: "ETW 丢失统计未知",
      detail: "无法读取 ETW 丢失计数；不能把未知当作零丢失。",
    });
  } else if ((sensor?.events_lost ?? 0) > 0) {
    anomalies.push({
      tone: "critical",
      title: "ETW 报告事件丢失",
      detail: `ETW events_lost = ${sensor?.events_lost}。事件历史可能不完整，不能视为完整采集。`,
    });
  }
  if ((sensor?.realtime_buffers_lost ?? 0) > 0) {
    anomalies.push({
      tone: "caution",
      title: "ETW 实时缓冲区丢失",
      detail: `realtime_buffers_lost = ${sensor?.realtime_buffers_lost}。`,
    });
  }
  if ((sensor?.events_dropped ?? 0) > 0) {
    anomalies.push({
      tone: "caution",
      title: "传感器管道丢弃事件",
      detail: `events_dropped = ${sensor?.events_dropped}。该项不能与 ETW 丢失混为一谈。`,
    });
  }
  if ((sensor?.decode_errors ?? 0) > 0) {
    anomalies.push({
      tone: "caution",
      title: "传感器解码错误",
      detail: `decode_errors = ${sensor?.decode_errors}。`,
    });
  }
  if (unpairedCount > 0 || indeterminateCount > 0) {
    anomalies.push({
      tone: "notice",
      title: "当前列表存在未配对或身份不确定事件",
      detail: `未配对 ${unpairedCount} 条，身份不确定 ${indeterminateCount} 条。这些事件仍被保留，不能据此判断进程行为。`,
    });
  }
  if (storageError !== null) {
    anomalies.push({
      tone: "critical",
      title: "本地事件数据库不可用",
      detail: storageError,
    });
  } else if (storage !== null && storage.state !== "ready") {
    anomalies.push({
      tone: storage.state === "unavailable" ? "critical" : "caution",
      title:
        storage.state === "unavailable"
          ? "本地事件数据库不可用"
          : "本地事件持久化降级",
      detail:
        storage.lastError ??
        storage.initError ??
        storage.ingestionError ??
        "持久化状态异常；实时事件仍来自传感器，但历史查询可能不完整。",
    });
  }
  if (live.state.raw.sessionClosed) {
    anomalies.push({
      tone: "notice",
      title: "本次传感器会话已结束",
      detail:
        live.state.raw.sessionEnd === null
          ? "实时通道已关闭，但未收到 SessionEnd 记录。"
          : `SessionEnd：${live.state.raw.sessionEnd.reason}。实时列表不再更新。`,
    });
  }

  const storageStateLabel =
    storage === null
      ? "未知"
      : storage.state === "ready"
        ? "正常"
        : storage.state === "degraded"
          ? "降级"
          : "不可用";
  const sessionEnded = live.state.raw.sessionClosed;
  const connectionLabel =
    live.state.phase === "loading" && live.state.linkState === null
      ? "正在连接"
      : sessionEnded
        ? "会话已结束"
        : live.state.phase === "error" || live.state.linkState === null
          ? "未连接"
          : live.state.linkState === "running"
            ? "已连接"
            : live.state.linkState === "degraded"
              ? "连接异常"
              : "未连接";

  const connectionTone =
    connectionLabel === "已连接"
      ? "success"
      : connectionLabel === "正在连接"
        ? "unknown"
        : "warning";

  const dataAnomalies = anomalies.filter(
    (anomaly) => anomaly.title !== "传感器未连接",
  );
  const liveSummary =
    live.state.phase === "loading"
      ? "连接中"
      : live.state.raw.events.length > 0
        ? `${live.state.raw.events.length} 条`
        : "暂无";
  const historySummary =
    history === null
      ? historyError
        ? "读取失败"
        : "查询中"
      : `${history.total} 条`;

  return (
    <div className="page events-page">
      <header className="events-simple-header">
        <h1>进程事件</h1>
        <div className={`events-connection events-connection--${connectionTone}`}>
          <span className="events-connection__dot" aria-hidden="true" />
          <span aria-live="polite">{connectionLabel}</span>
        </div>
      </header>

      {dataAnomalies.length > 0 ? (
        <p className="events-attention-line" role="status">
          {dataAnomalies.length} 条数据提醒。展开“实时事件”查看。
        </p>
      ) : null}

      <div className="events-sections">
        <ExpandableSection title="历史事件" summary={historySummary}>
          <div className="events-history">
            <form
              className="history-filters history-filters--simple"
              onSubmit={(event) => {
                event.preventDefault();
                runSearch();
              }}
            >
              <div className="filter-field">
                <span>开始日期</span>
                <DatePicker
                  label="开始日期"
                  value={form.from}
                  onChange={(value) =>
                    setForm((current) => ({ ...current, from: value }))
                  }
                />
              </div>
              <div className="filter-field">
                <span>结束日期</span>
                <DatePicker
                  label="结束日期"
                  value={form.to}
                  onChange={(value) =>
                    setForm((current) => ({ ...current, to: value }))
                  }
                />
              </div>
              <label className="filter-field">
                <span>进程</span>
                <input
                  type="text"
                  value={form.image}
                  maxLength={160}
                  placeholder="名称或路径"
                  onChange={(event) =>
                    setForm((current) => ({ ...current, image: event.target.value }))
                  }
                />
              </label>
              <label className="filter-field">
                <span>PID</span>
                <input
                  type="number"
                  min={1}
                  value={form.pid}
                  onChange={(event) =>
                    setForm((current) => ({ ...current, pid: event.target.value }))
                  }
                />
              </label>
              <label className="filter-field">
                <span>事件</span>
                <select
                  value={form.eventType}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      eventType: event.target.value as FormState["eventType"],
                    }))
                  }
                >
                  <option value="all">全部</option>
                  <option value="started">启动</option>
                  <option value="stopped">退出</option>
                </select>
              </label>
              <div className="history-filters__actions">
                <button type="submit" className="button button--primary">
                  查询
                </button>
                <button
                  type="button"
                  className="button button--quiet"
                  onClick={resetSearch}
                >
                  重置
                </button>
              </div>
            </form>
            {formError === null ? null : <p className="detail-alert">{formError}</p>}

            {historyError !== null ? (
              <p className="detail-alert">{historyError}</p>
            ) : historyLoading && history === null ? (
              <p className="home-note" role="status">
                正在查询…
              </p>
            ) : history === null ? (
              <p className="home-note">暂无历史事件。</p>
            ) : (
              <>
                <HistoryTable
                  result={history}
                  onSelect={(id) => void selectStoredEvent(id)}
                />
                <div className="history-pagination">
                  <button
                    type="button"
                    className="button button--quiet"
                    onClick={() => changePage(-1)}
                    disabled={historyQuery.offset === 0}
                  >
                    上一页
                  </button>
                  <span>
                    第 {Math.floor(historyQuery.offset / HISTORY_LIMIT) + 1} 页 · 每页{" "}
                    {HISTORY_LIMIT} 条
                  </span>
                  <button
                    type="button"
                    className="button button--quiet"
                    onClick={() => changePage(1)}
                    disabled={historyQuery.offset + HISTORY_LIMIT >= history.total}
                  >
                    下一页
                  </button>
                </div>
              </>
            )}
          </div>
        </ExpandableSection>

        <ExpandableSection title="实时事件" summary={liveSummary}>
          <div className="events-live">
            <div className="events-live__toolbar">
              <button
                type="button"
                className="button button--quiet"
                onClick={live.resume}
                disabled={live.state.phase === "loading"}
              >
                重新连接
              </button>
            </div>

            <AnomalyStack anomalies={anomalies} />

            {live.state.phase === "loading" && live.state.raw.events.length === 0 ? (
              <p className="home-note" role="status">
                正在连接…
              </p>
            ) : null}

            {live.state.phase !== "loading" && live.state.raw.events.length === 0 ? (
              <p className="home-note">暂无事件。</p>
            ) : null}

            {live.state.raw.events.length > 0 ? (
              <div className="event-simple-list" aria-label="实时进程事件列表">
                <div className="event-simple-list__header" aria-hidden="true">
                  <span>进程</span>
                  <span>事件</span>
                  <span>时间</span>
                  <span>PID</span>
                </div>
                {[...live.state.raw.events].reverse().map((event) => (
                  <LiveEventRow
                    key={event.event_id}
                    event={event}
                    selected={selectedEventId === event.event_id}
                    onSelect={selectLiveEvent}
                  />
                ))}
              </div>
            ) : null}

            {selectedLiveEvent !== null ? (
              <LiveDetail
                event={selectedLiveEvent}
                events={live.state.raw.events}
                revisions={live.state.raw.revisions}
                onSelectEvent={selectLiveEvent}
              />
            ) : null}
          </div>
        </ExpandableSection>

        <ExpandableSection title="更多">
          <div className="events-more">
            <section className="events-more-block">
              <h3>行为分析</h3>
              <div className="analysis-controls">
                <label className="filter-field">
                  <span>会话</span>
                  <select
                    value={selectedAnalysisSession}
                    onChange={(event) => setAnalysisSessionId(event.target.value)}
                  >
                    <option value="">选择会话</option>
                    {sessions.map((session) => (
                      <option key={session.session_id} value={session.session_id}>
                        {shortSession(session.session_id)} ·{" "}
                        {session.closed ? "已结束" : "进行中"} · {session.event_count} 条
                      </option>
                    ))}
                  </select>
                </label>
                <button
                  type="button"
                  className="button button--primary"
                  onClick={() => void runAnalysis()}
                  disabled={analysisLoading || selectedAnalysisSession.length === 0}
                >
                  {analysisLoading ? "分析中…" : "分析"}
                </button>
              </div>
              {analysisError === null ? null : (
                <p className="detail-alert">{analysisError}</p>
              )}
              {analysisLoading ? (
                <p className="home-note" role="status">
                  正在分析…
                </p>
              ) : analysis === null ? (
                <p className="home-note">选择会话后分析。</p>
              ) : (
                <>
                  <div
                    className={`analysis-overall analysis-overall--${detectionStatusTone(analysis.overall)}`}
                  >
                    <span
                      className={`detection-status detection-status--${detectionStatusTone(analysis.overall)}`}
                    >
                      {DETECTION_STATUS_LABELS[analysis.overall]}
                    </span>
                    <p>{analysis.summary}</p>
                  </div>
                  {analysis.results.length === 0 ? (
                    <p className="home-note">当前规则范围内未发现异常。</p>
                  ) : (
                    <div className="detection-results">
                      {analysis.results.map((result) => (
                        <DetectionResultCard
                          key={result.rule_id}
                          result={result}
                          onSelectEvent={(id) => void selectStoredEvent(id)}
                        />
                      ))}
                    </div>
                  )}
                </>
              )}
            </section>

            <section className="events-more-block">
              <h3>本地数据</h3>
              {storageError !== null ? (
                <p className="home-note home-note--warning">{storageError}</p>
              ) : storage === null ? (
                <p className="home-note">正在读取…</p>
              ) : (
                <>
                  <p className="home-note">状态：{storageStateLabel}</p>
                  {storage.ingestion.source_available ? null : (
                    <p className="home-note home-note--warning">
                      事件通道不可用，历史查询仍可继续。
                    </p>
                  )}
                  {storage.lastError === null ? null : (
                    <p className="home-note home-note--warning">{storage.lastError}</p>
                  )}
                </>
              )}
            </section>
          </div>
        </ExpandableSection>

        {detailLoading || detailError !== null || storedDetail !== null ? (
          <ExpandableSection
            key={selectedEventId ?? "stored-detail"}
            title="事件详情"
            defaultOpen
          >
            {detailLoading ? (
              <p className="home-note" role="status">
                正在读取事件详情…
              </p>
            ) : null}
            {detailError === null ? null : (
              <p className="detail-alert">{detailError}</p>
            )}
            {!detailLoading && detailError === null && storedDetail !== null ? (
              <StoredDetail
                detail={storedDetail}
                onSelectEvent={(id) => void selectStoredEvent(id)}
              />
            ) : null}
          </ExpandableSection>
        ) : null}
      </div>
    </div>
  );
}

