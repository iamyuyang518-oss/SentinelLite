import type {
  SensorLinkState,
  SensorPermissionState,
  SensorStatusDetails,
  SensorStatusReport,
} from "../types";
import { formatCheckedAt } from "./presentation";

export type SensorDisplayState = "active" | "unavailable" | "failed" | "unknown";
export type SensorHealthState = "healthy" | "unhealthy" | "unknown";

export interface SensorMetricValue {
  value: string;
  unknown: boolean;
}

export interface SensorMetrics {
  received: SensorMetricValue;
  processed: SensorMetricValue;
  lost: SensorMetricValue;
  uptime: SensorMetricValue;
}

export interface SensorView {
  state: SensorDisplayState;
  stateLabel: string;
  health: SensorHealthState;
  healthLabel: string;
  linkLabel: string;
  headline: string;
  explanation: string;
  permissionLabel: string;
  lastEventLabel: string;
  droppedLabel: string;
  decodeErrorsLabel: string;
  metrics: SensorMetrics;
  hasMetrics: boolean;
  diagnostic: string | null;
}

const STATE_LABELS: Record<SensorDisplayState, string> = {
  active: "运行中",
  unavailable: "不可用",
  failed: "已失败",
  unknown: "状态未知",
};

const HEALTH_LABELS: Record<SensorHealthState, string> = {
  healthy: "健康",
  unhealthy: "健康异常",
  unknown: "健康未知",
};

const PERMISSION_LABELS: Record<SensorPermissionState, string> = {
  unknown: "未知",
  granted: "已授予",
  limited: "受限",
  denied: "已拒绝",
};

const LINK_LABELS: Record<SensorLinkState, string> = {
  running: "已连接",
  degraded: "已连接",
  stopped: "未连接",
  unavailable: "不可用",
  communicationFailed: "通信失败",
};

const KNOWN_DIAGNOSTICS: Record<string, string> = {
  "sensor health not proven":
    "传感器尚未证明健康：需要先观察到已处理事件，且无丢失、解码错误或覆盖缺口。",
  "ETW live loss count unavailable":
    "无法读取 ETW 实时丢失计数，当前健康状态无法被证明。",
  "sensor status endpoint does not exist; the sensor may be stopped or its IPC channel may be unavailable":
    "未找到传感器状态通道；传感器可能未运行，或状态通道不可用。",
  "sensor IPC timed out": "传感器状态查询超时。",
  "pipe server is not elevated": "状态通道的服务器进程未以管理员权限运行。",
};

let countFormatter: Intl.NumberFormat | undefined;

export function formatEventCount(value: number): string {
  if (!Number.isFinite(value) || value < 0) {
    return "未知";
  }
  countFormatter ??= new Intl.NumberFormat("zh-CN");
  return countFormatter.format(Math.trunc(value));
}

export function formatUptime(uptimeMs: number): string {
  if (!Number.isFinite(uptimeMs) || uptimeMs < 0) {
    return "未知";
  }

  const totalSeconds = Math.floor(uptimeMs / 1000);
  const days = Math.floor(totalSeconds / 86_400);
  const hours = Math.floor((totalSeconds % 86_400) / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  const seconds = totalSeconds % 60;

  if (days > 0) {
    return `${days} 天 ${hours} 小时`;
  }
  if (hours > 0) {
    return `${hours} 小时 ${minutes} 分`;
  }
  if (minutes > 0) {
    return `${minutes} 分 ${seconds} 秒`;
  }
  return `${seconds} 秒`;
}

export function permissionStateLabel(state: SensorPermissionState | null): string {
  return state === null ? "未知" : PERMISSION_LABELS[state];
}

export function linkStateLabel(state: SensorLinkState | null): string {
  return state === null ? "未知" : LINK_LABELS[state];
}

function unknownMetric(value = "—"): SensorMetricValue {
  return { value, unknown: true };
}

function countMetric(value: number): SensorMetricValue {
  return { value: formatEventCount(value), unknown: false };
}

function sensorMetrics(sensor: SensorStatusDetails | null): SensorMetrics {
  if (sensor === null) {
    return {
      received: unknownMetric(),
      processed: unknownMetric(),
      lost: unknownMetric(),
      uptime: unknownMetric(),
    };
  }

  const uptime = formatUptime(sensor.uptime_ms);
  return {
    received: countMetric(sensor.events_received),
    processed: countMetric(sensor.events_processed),
    lost:
      sensor.events_lost === null
        ? unknownMetric("未知")
        : countMetric(sensor.events_lost),
    uptime: { value: uptime, unknown: uptime === "未知" },
  };
}

function resolveState(
  report: SensorStatusReport | null,
  queryError: string | null,
): SensorDisplayState {
  if (queryError !== null) {
    return "unavailable";
  }
  if (report === null) {
    return "unknown";
  }
  if (
    report.linkState === "stopped" ||
    report.linkState === "unavailable" ||
    report.linkState === "communicationFailed"
  ) {
    return "unavailable";
  }
  if (report.sensor === null) {
    return "unknown";
  }

  switch (report.sensor.sensor_state) {
    case "failed":
      return "failed";
    case "stopped":
    case "unavailable":
      return "unavailable";
    case "active":
    case "degraded":
      return "active";
    case "not_started":
    case "starting":
      return "unknown";
  }
}

function resolveHealth(
  report: SensorStatusReport | null,
  sensor: SensorStatusDetails | null,
  state: SensorDisplayState,
): SensorHealthState {
  if (sensor === null) {
    return "unknown";
  }
  if (!sensor.health_ok) {
    return "unhealthy";
  }
  if (
    state === "active" &&
    report?.linkState === "running" &&
    sensor.sensor_state === "active"
  ) {
    return "healthy";
  }
  return "unknown";
}

function presentDiagnostic(value: string | null | undefined): string | null {
  if (value === null || value === undefined) {
    return null;
  }
  const text = value.trim().replace(/\s+/g, " ");
  if (text.length === 0) {
    return null;
  }
  const translated = KNOWN_DIAGNOSTICS[text] ?? text;
  const characters = Array.from(translated);
  return characters.length > 240
    ? `${characters.slice(0, 240).join("")}…`
    : translated;
}

function activeCopy(
  state: SensorDisplayState,
  health: SensorHealthState,
  sensor: SensorStatusDetails | null,
): { headline: string; explanation: string } {
  if (state === "unavailable") {
    return {
      headline: "传感器当前不可用",
      explanation:
        "传感器拒绝或无法完成状态查询；页面不会保留上一次的健康结果。整体保护状态仍为未知。",
    };
  }
  if (state === "failed") {
    return {
      headline: "传感器已报告失败",
      explanation:
        "传感器明确报告了失败状态；下方计数是当前响应中的最后值，不代表采集仍在运行。整体保护状态仍为未知。",
    };
  }
  if (state === "unknown") {
    if (sensor?.sensor_state === "starting" || sensor?.sensor_state === "not_started") {
      return {
        headline: "传感器正在启动",
        explanation:
          "状态通道已连接，但传感器尚未报告可采集状态；在取得有效结果前不会视为健康。整体保护状态仍为未知。",
      };
    }
    return {
      headline: "正在确认传感器状态",
      explanation:
        "在取得有效结果前，不会假定传感器健康或正在运行。整体保护状态仍为未知。",
    };
  }
  if (health === "healthy") {
    return {
      headline: "进程事件采集正在运行",
      explanation:
        "传感器已连接，并报告事件处理、丢失与覆盖条件处于健康范围。整体保护状态仍为未知：当前版本不提供告警、拦截或自动处置。",
    };
  }
  if (health === "unhealthy") {
    if (sensor?.sensor_state === "degraded") {
      return {
        headline: "传感器运行中，但已降级",
        explanation:
          "传感器仍在采集，但报告了降级状态。运行中不等于健康；请查看采集统计与传感器报告。整体保护状态仍为未知。",
      };
    }
    return {
      headline: "传感器运行中，但健康状态未通过",
      explanation:
        "采集组件仍在运行；健康异常不等于传感器已停止。请查看采集统计与传感器报告。整体保护状态仍为未知。",
    };
  }
  return {
    headline: "传感器运行中，健康状态尚无法判定",
    explanation:
      "状态通道已连接，但当前结果不足以证明健康；不会把运行中当作健康。整体保护状态仍为未知。",
  };
}

export function deriveSensorView(
  report: SensorStatusReport | null,
  queryError: string | null = null,
): SensorView {
  const normalizedError = presentDiagnostic(queryError);
  const trustSensor =
    report !== null &&
    report.sensor !== null &&
    report.linkState !== "communicationFailed";
  const sensor = trustSensor ? report.sensor : null;
  const state = resolveState(report, normalizedError);
  const health = resolveHealth(report, sensor, state);

  let copy = activeCopy(state, health, sensor);
  if (normalizedError !== null) {
    copy = {
      headline: "无法读取传感器状态",
      explanation:
        "状态查询失败，当前不能确认传感器是否运行；页面不会保留上一次的健康状态或计数。整体保护状态仍为未知。",
    };
  } else if (report?.linkState === "communicationFailed") {
    copy = {
      headline: "与传感器的状态查询通信失败",
      explanation:
        "状态查询通信失败，当前无法确认传感器是否运行，也不会显示健康或正常状态。整体保护状态仍为未知。",
    };
  } else if (report?.linkState === "stopped") {
    copy = {
      headline: "未检测到正在运行的传感器",
      explanation:
        "状态通道不存在，传感器可能未启动。页面不会保留上一次的健康状态或计数。整体保护状态仍为未知。",
    };
  } else if (sensor?.sensor_state === "stopped" || sensor?.sensor_state === "unavailable") {
    copy = {
      headline: "传感器已停止采集",
      explanation:
        "传感器报告停止或不可用；下方计数是当前响应中的最后值，不代表正在采集。整体保护状态仍为未知。",
    };
  }

  const diagnostic = normalizedError ?? presentDiagnostic(sensor?.last_error ?? report?.error);

  return {
    state,
    stateLabel: STATE_LABELS[state],
    health,
    healthLabel: HEALTH_LABELS[health],
    linkLabel:
      normalizedError !== null ? "查询失败" : linkStateLabel(report?.linkState ?? null),
    headline: copy.headline,
    explanation: copy.explanation,
    permissionLabel: permissionStateLabel(sensor?.permission_state ?? null),
    lastEventLabel:
      sensor === null
        ? "—"
        : sensor.last_event_at === null
          ? "尚未收到事件"
          : formatCheckedAt(sensor.last_event_at),
    droppedLabel: sensor === null ? "—" : formatEventCount(sensor.events_dropped),
    decodeErrorsLabel: sensor === null ? "—" : formatEventCount(sensor.decode_errors),
    metrics: sensorMetrics(sensor),
    hasMetrics: sensor !== null,
    diagnostic,
  };
}
