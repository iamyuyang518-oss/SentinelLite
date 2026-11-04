import type { SensorStatusReport } from "../types";
import { deriveSensorView, type SensorView } from "./sensorStatus";

export type HomeStatusPhase =
  | "loading"
  | "notEnabled"
  | "starting"
  | "active"
  | "attention"
  | "failed"
  | "disconnected"
  | "unknown";

export type HomeStatusTone = "idle" | "success" | "warning" | "danger" | "unknown";

export interface HomeAttention {
  title: string;
  message: string;
}

export interface HomeStatus {
  phase: HomeStatusPhase;
  statusLabel: string;
  title: string;
  description: string;
  tone: HomeStatusTone;
  attention: HomeAttention | null;
  canRetry: boolean;
  /**
   * Selects the static not-enabled page. 0.1.0 exposes no in-app sensor start
   * action; the page only explains that in-app enabling is unavailable.
   */
  showEnable: boolean;
}

function attentionFrom(
  view: SensorView,
  title = view.headline,
  fallback = "监测状态未达到健康条件。",
): HomeAttention {
  return {
    title,
    message: view.diagnostic ?? fallback,
  };
}

/**
 * Maps the live sensor report to the one status a normal user needs to see first.
 *
 * Not enabled, starting, active, partial, failed, disconnected, and unknown remain distinct.
 * A failed query never falls back to a normal or healthy state.
 */
export function deriveHomeStatus(
  report: SensorStatusReport | null,
  queryError: string | null,
  queryPhase: "loading" | "ready" | "error",
): HomeStatus {
  if (queryPhase === "loading" && report === null && queryError === null) {
    return {
      phase: "loading",
      statusLabel: "正在确认",
      title: "正在确认保护状态。",
      description: "正在读取本机监测状态。",
      tone: "unknown",
      attention: null,
      canRetry: false,
      showEnable: false,
    };
  }

  const view = deriveSensorView(report, queryError);

  if (queryPhase === "error" || queryError !== null) {
    return {
      phase: "disconnected",
      statusLabel: "未连接",
      title: "无法确认保护状态。",
      description: "状态查询失败或通信中断，不能确认监测是否运行。",
      tone: "warning",
      attention: {
        title:
          report?.linkState === "communicationFailed" ? "与传感器的通信失败" : "状态查询失败",
        message: queryError ?? view.diagnostic ?? view.explanation,
      },
      canRetry: true,
      showEnable: false,
    };
  }

  if (report === null) {
    return {
      phase: "unknown",
      statusLabel: "状态未知",
      title: "保护状态未知。",
      description: "当前没有足够信息判断监测是否运行。",
      tone: "unknown",
      attention: attentionFrom(view, view.headline, "当前没有足够信息判断监测是否运行。"),
      canRetry: true,
      showEnable: false,
    };
  }

  if (report.linkState === "communicationFailed") {
    return {
      phase: "disconnected",
      statusLabel: "未连接",
      title: "无法确认保护状态。",
      description: "与传感器的状态查询通信中断，不能确认监测是否运行。",
      tone: "warning",
      attention: attentionFrom(view, "与传感器的通信失败", "状态查询通信中断。"),
      canRetry: true,
      showEnable: false,
    };
  }

  if (report.linkState === "stopped" || report.linkState === "unavailable") {
    return {
      phase: "notEnabled",
      statusLabel: "尚未启用",
      title: "让电脑更安全。",
      description: "当前版本暂不支持在应用内启用实时保护。管理员可以在应用外手动启动传感器。",
      tone: "idle",
      attention: null,
      canRetry: true,
      showEnable: true,
    };
  }

  if (report.sensor === null) {
    return {
      phase: "unknown",
      statusLabel: "状态未知",
      title: "保护状态未知。",
      description: "状态通道已连接，但暂无传感器状态可确认。",
      tone: "unknown",
      attention: attentionFrom(view, view.headline, "暂无传感器状态可确认。"),
      canRetry: true,
      showEnable: false,
    };
  }

  switch (report.sensor.sensor_state) {
    case "failed":
      return {
        phase: "failed",
        statusLabel: "启动失败",
        title: "监测启动失败。",
        description: "传感器报告失败，当前无法确认正在采集。",
        tone: "danger",
        attention: attentionFrom(view, view.headline, "传感器报告失败，当前无法确认采集。"),
        canRetry: true,
        showEnable: false,
      };
    case "starting":
      return {
        phase: "starting",
        statusLabel: "正在启动",
        title: "监测正在启动。",
        description: "状态通道已连接，正在等待采集就绪。",
        tone: "unknown",
        attention: null,
        canRetry: true,
        showEnable: false,
      };
    case "not_started":
    case "stopped":
    case "unavailable":
      return {
        phase: "notEnabled",
        statusLabel: "尚未启用",
        title: "让电脑更安全。",
        description: "当前版本暂不支持在应用内启用实时保护。管理员可以在应用外手动启动传感器。",
        tone: "idle",
        attention: null,
        canRetry: true,
        showEnable: true,
      };
    case "active":
    case "degraded": {
      const partiallyAvailable =
        report.linkState === "degraded" ||
        report.sensor.sensor_state === "degraded" ||
        !report.sensor.health_ok;

      if (partiallyAvailable) {
        return {
          phase: "attention",
          statusLabel: "部分可用",
          title: "监测需要关注。",
          description: "实时监测仍在运行，但部分状态未达到健康条件。",
          tone: "warning",
          attention: attentionFrom(view, view.headline, "部分状态未达到健康条件。"),
          canRetry: true,
          showEnable: false,
        };
      }

      return {
        phase: "active",
        statusLabel: "监测正常",
        title: "实时监测正在运行。",
        description: "SentinelLite 正在记录这台电脑上的进程活动。",
        tone: "success",
        attention: null,
        canRetry: false,
        showEnable: false,
      };
    }
  }
}
