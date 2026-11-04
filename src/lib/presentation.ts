import type {
  ProtectionState,
  RiskLevel,
  RiskScope,
  SensorState,
  StartupSource,
} from "../types";

export type RiskTone = "critical" | "alert" | "caution" | "notice" | "neutral" | "unknown";

export interface RiskPresentation {
  label: string;
  tone: RiskTone;
  description: string;
}

const RISK_PRESENTATION: Record<RiskLevel, RiskPresentation> = {
  confirmedThreat: {
    label: "已确认威胁",
    tone: "critical",
    description: "现有证据达到确认级别。该等级不会仅凭名称、签名状态或品牌推断。",
  },
  highRiskSuspicious: {
    label: "高风险可疑行为",
    tone: "alert",
    description: "观察到高风险异常，但尚不足以称为已确认恶意软件。",
  },
  potentialUnwanted: {
    label: "潜在不受欢迎软件",
    tone: "caution",
    description: "行为或来源符合推广、捆绑或持续驻留等特征，需要人工核实。",
  },
  userUnwanted: {
    label: "用户不需要的软件",
    tone: "notice",
    description: "用户已明确标记为不需要；这不等于它是恶意软件。",
  },
  configurationAnomaly: {
    label: "配置异常",
    tone: "caution",
    description: "观察到偏离常见配置的证据，但原因和意图可能未知。",
  },
  noClearIssue: {
    label: "检查范围内未发现异常",
    tone: "neutral",
    description: "仅表示当前已检查的证据范围内没有发现问题，不代表对象安全或行为可信。",
  },
  insufficientEvidence: {
    label: "证据不足",
    tone: "unknown",
    description: "数据缺失、权限不足或尚未检查，当前不能形成安全结论。",
  },
};

const RISK_RANK: Record<RiskLevel, number> = {
  confirmedThreat: 0,
  highRiskSuspicious: 1,
  potentialUnwanted: 2,
  userUnwanted: 3,
  configurationAnomaly: 4,
  insufficientEvidence: 5,
  noClearIssue: 6,
};

const PROTECTION_STATE_LABELS: Record<ProtectionState, string> = {
  riskDetected: "发现风险",
  degraded: "保护降级",
  unknown: "状态未知",
};

const RISK_SCOPE_LABELS: Record<RiskScope, string> = {
  processOverall: "进程整体",
  affinityConfiguration: "亲和性配置",
  softwareMetadata: "软件元数据",
  startupMetadata: "启动项元数据",
};

const SENSOR_STATE_LABELS: Record<SensorState, string> = {
  active: "运行中",
  degraded: "保护降级",
  notImplemented: "尚未实现",
  notChecked: "尚未检查",
  unknown: "未知",
};

const STARTUP_SOURCE_LABELS: Record<StartupSource, string> = {
  registryRun: "注册表 Run",
  registryRunOnce: "注册表 RunOnce",
  startupFolder: "启动文件夹",
};

export function getRiskPresentation(level: RiskLevel): RiskPresentation {
  return RISK_PRESENTATION[level];
}

export function riskRank(level: RiskLevel): number {
  return RISK_RANK[level];
}

export function riskScopeLabel(scope: RiskScope): string {
  return RISK_SCOPE_LABELS[scope];
}

export function protectionStateLabel(state: ProtectionState): string {
  return PROTECTION_STATE_LABELS[state];
}

export function sensorStateLabel(state: SensorState): string {
  return SENSOR_STATE_LABELS[state];
}

export function startupSourceLabel(source: StartupSource): string {
  return STARTUP_SOURCE_LABELS[source];
}

export function formatCheckedAt(isoTimestamp: string, locale = "zh-CN"): string {
  const date = new Date(isoTimestamp);
  if (Number.isNaN(date.getTime())) {
    return "时间未知";
  }

  return new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(date);
}

export function formatCpuIndices(indices: readonly number[]): string {
  const sorted = [...new Set(indices)]
    .filter((value) => Number.isInteger(value) && value >= 0)
    .sort((left, right) => left - right);

  if (sorted.length === 0) {
    return "无";
  }

  const ranges: string[] = [];
  let start = sorted[0] as number;
  let previous = start;

  for (let index = 1; index <= sorted.length; index += 1) {
    const current = sorted[index];
    if (current !== undefined && current === previous + 1) {
      previous = current;
      continue;
    }

    ranges.push(start === previous ? String(start) : `${start}–${previous}`);
    if (current !== undefined) {
      start = current;
      previous = current;
    }
  }

  return ranges.join(", ");
}
