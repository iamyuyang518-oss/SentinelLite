import type {
  DataIssueWire,
  EventBatchWire,
  PairingRevisionWire,
  PairingStatusWire,
  ProcessEventWire,
  SessionEndWire,
} from "../types";
import { formatCheckedAt } from "./presentation";

export const MAX_LIVE_EVENTS = 500;
export const MAX_LIVE_REVISIONS = 200;

export interface PairingPresentation {
  label: string;
  tone: "paired" | "waiting" | "unpaired" | "indeterminate";
  detail: string;
}

const UNPAIRED_LABELS: Record<string, string> = {
  stop_without_observed_start: "未观察到启动事件",
  orphan_stop_not_retained: "孤儿退出事件未保留",
  duplicate_start: "重复启动",
  duplicate_stop: "重复退出",
  identity_mismatch: "进程身份不匹配",
  start_not_retained: "启动事件未保留",
};

export function pairingPresentation(pairing: PairingStatusWire): PairingPresentation {
  switch (pairing.state) {
    case "paired":
      return {
        label: pairing.order === "in_order" ? "已配对" : "已配对（乱序修正）",
        tone: "paired",
        detail:
          pairing.order === "in_order"
            ? "启动与退出事件已按顺序关联。"
            : "退出事件先到达；后续启动事件到达后已修正配对。",
      };
    case "waiting":
      return {
        label: "等待退出",
        tone: "waiting",
        detail: "已观察到启动事件，尚未观察到对应退出事件。",
      };
    case "unpaired": {
      const reason = pairing.reason.reason;
      return {
        label: `未配对：${UNPAIRED_LABELS[reason] ?? "原因未知"}`,
        tone: "unpaired",
        detail:
          reason === "identity_mismatch" && pairing.reason.open_create_time !== undefined
            ? `同 PID 的打开身份创建时间不同（${pairing.reason.open_create_time}），未跨身份配对。`
            : "该事件被保留，但当前不能可靠关联到另一侧生命周期事件。",
      };
    }
    case "indeterminate":
      return {
        label:
          pairing.reason === "missing_process_create_time"
            ? "身份不确定：缺少创建时间"
            : "身份不确定：事件类型不支持",
        tone: "indeterminate",
        detail: "缺少可靠进程身份，未进行配对；不会仅凭 PID 猜测。",
      };
  }
}

export const DATA_ISSUE_LABELS: Record<DataIssueWire, string> = {
  missing_parent_pid: "缺少父进程 PID",
  missing_image_name: "缺少映像名称",
  partial_image_name: "映像名称不完整",
  image_name_truncated: "映像名称已截断",
  missing_event_timestamp: "缺少 ETW 事件时间",
  event_timestamp_unrepresentable: "ETW 事件时间无法表示",
  missing_receive_time: "缺少接收时间",
  missing_normalize_time: "缺少标准化时间",
  indeterminate_process_create_time: "进程创建时间缺失，身份不确定",
  process_start_time_unrepresentable: "进程启动时间无法表示",
};

export function processDisplayName(imageName: string | null): string {
  if (imageName === null || imageName.trim().length === 0) {
    return "名称未提供";
  }
  const segments = imageName.split(/[\\/]/);
  const name = segments[segments.length - 1]?.trim();
  return name && name.length > 0 ? name : imageName;
}

export function imagePathLabel(imageName: string | null): string {
  return imageName === null || imageName.trim().length === 0 ? "未提供" : imageName;
}

export function eventTypeLabel(eventType: ProcessEventWire["event_type"]): string {
  return eventType === "started" ? "进程启动" : "进程退出";
}

export function primaryEventTime(event: ProcessEventWire): string | null {
  return event.etw.occurred_at ?? event.received_at ?? event.normalized_at;
}

export function eventTimeLabel(event: ProcessEventWire): string {
  const time = primaryEventTime(event);
  return time === null ? "时间未知" : formatCheckedAt(time);
}

export function eventTimeSource(event: ProcessEventWire): string {
  if (event.etw.occurred_at !== null) {
    return "ETW 事件时间";
  }
  if (event.received_at !== null) {
    return "传感器接收时间";
  }
  if (event.normalized_at !== null) {
    return "标准化时间";
  }
  return "无可用时间";
}

export function identityLabel(event: ProcessEventWire): string {
  return `PID ${event.identity.pid} · 创建时间 ${event.identity.create_time}`;
}

export function identityCertaintyLabel(event: ProcessEventWire): string {
  return event.identity_certainty === "exact"
    ? "身份确定（PID + 创建时间）"
    : "身份不确定（创建时间缺失，未配对）";
}

export function completenessIssues(event: ProcessEventWire): string[] {
  return event.completeness.issues.map((issue) => DATA_ISSUE_LABELS[issue]);
}

export function completenessLabel(event: ProcessEventWire): string {
  return event.completeness.issues.length === 0 ? "字段完整" : "字段不完整";
}

export function parentPidLabel(event: ProcessEventWire): string {
  return event.parent_pid === null ? "未提供" : String(event.parent_pid);
}

export function processStartedAtLabel(event: ProcessEventWire): string {
  return event.process_started_at === null
    ? "未提供"
    : formatCheckedAt(event.process_started_at);
}

export function applyPairingRevision(
  events: ProcessEventWire[],
  revision: PairingRevisionWire,
): ProcessEventWire[] {
  let changed = false;
  const updated = events.map((event) => {
    if (event.event_id !== revision.stop_event_id) {
      return event;
    }
    changed = true;
    return {
      ...event,
      pairing: {
        state: "paired" as const,
        start_event_id: revision.start_event_id,
        stop_event_id: revision.stop_event_id,
        order: "out_of_order" as const,
        sequence_match: revision.sequence_match,
      },
    };
  });
  return changed ? updated : events;
}

export interface LiveEventState {
  sessionId: string | null;
  events: ProcessEventWire[];
  revisions: PairingRevisionWire[];
  sessionEnd: SessionEndWire | null;
  nextAfter: number;
  cursorExpiredCount: number;
  missedRecords: number;
  invalidRecords: number;
  sessionClosed: boolean;
  trimmed: number;
  restarted: boolean;
  batchCount: number;
}

export const EMPTY_LIVE_STATE: LiveEventState = {
  sessionId: null,
  events: [],
  revisions: [],
  sessionEnd: null,
  nextAfter: 0,
  cursorExpiredCount: 0,
  missedRecords: 0,
  invalidRecords: 0,
  sessionClosed: false,
  trimmed: 0,
  restarted: false,
  batchCount: 0,
};

export function mergeLiveBatch(state: LiveEventState, batch: EventBatchWire): LiveEventState {
  const sessionChanged = state.sessionId !== null && state.sessionId !== batch.session_id;
  const next: LiveEventState = sessionChanged
    ? {
        ...EMPTY_LIVE_STATE,
        sessionId: batch.session_id,
        restarted: true,
        cursorExpiredCount: state.cursorExpiredCount,
        missedRecords: state.missedRecords,
        invalidRecords: state.invalidRecords,
      }
    : { ...state, sessionId: batch.session_id };

  next.batchCount += 1;
  if (batch.cursor_expired) {
    next.cursorExpiredCount += 1;
    next.missedRecords += batch.missed_records;
  }

  if (batch.cursor_ahead) {
    // A cursor from a previous session (or an impossible ahead cursor) is not
    // reinterpreted: restart this read from the retained window.
    next.nextAfter = 0;
    return next;
  }

  let events = next.events;
  const revisions = [...next.revisions];
  for (const envelope of batch.records) {
    const record = envelope.record;
    switch (record.record) {
      case "process_event": {
        if (!events.some((event) => event.event_id === record.event_id)) {
          events = [...events, record];
        }
        break;
      }
      case "pairing_revision": {
        if (!revisions.some(
          (revision) =>
            revision.stop_event_id === record.stop_event_id &&
            revision.start_event_id === record.start_event_id,
        )) {
          revisions.push(record);
        }
        break;
      }
      case "session_end": {
        next.sessionEnd = record;
        break;
      }
      case "invalid_event": {
        next.invalidRecords += 1;
        break;
      }
    }
  }

  events = revisions.reduce(applyPairingRevision, events);
  if (events.length > MAX_LIVE_EVENTS) {
    const excess = events.length - MAX_LIVE_EVENTS;
    events = events.slice(excess);
    next.trimmed += excess;
  }
  next.events = events;
  next.revisions = revisions.slice(-MAX_LIVE_REVISIONS);
  next.nextAfter = batch.next_cursor;
  next.sessionClosed = next.sessionClosed || batch.session_closed;
  return next;
}

export function formatByteSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) {
    return "未知";
  }
  if (bytes < 1024) {
    return `${Math.trunc(bytes)} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KiB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}
