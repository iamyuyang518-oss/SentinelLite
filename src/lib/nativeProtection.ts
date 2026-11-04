import type {
  FirewallProfile,
  NativeProtectionObservation,
  NativeProtectionProviderState,
  NativeProtectionStatus,
} from "../types";

export const NATIVE_PROTECTION_OBSERVATION_LABELS: Record<
  NativeProtectionObservation,
  string
> = {
  complete: "已读取全部字段",
  partial: "部分字段未知",
  unavailable: "无法读取",
};

export const NATIVE_PROTECTION_PROVIDER_STATE_LABELS: Record<
  NativeProtectionProviderState,
  string
> = {
  observed: "已读取",
  partial: "部分读取",
  unavailable: "不可用",
  accessDenied: "无权限",
  error: "查询失败",
};

export const FIREWALL_PROFILE_LABELS: Record<FirewallProfile, string> = {
  domain: "域网络",
  private: "专用网络",
  public: "公用网络",
};

export type ProtectionSwitchSource = "defender" | "firewall";

export interface ProtectionSwitch {
  id: string;
  label: string;
  source: ProtectionSwitchSource;
  enabled: boolean | null;
}

export function boolLabel(value: boolean | null): string {
  if (value === null) {
    return "未知";
  }
  return value ? "已启用" : "未启用";
}

/**
 * Flattens the provider payload into the individual switches a user can reason about.
 * A missing value stays null; it is never converted to enabled or disabled.
 */
export function protectionSwitches(
  status: NativeProtectionStatus | null,
): ProtectionSwitch[] {
  if (status === null) {
    return [];
  }

  return [
    {
      id: "defender:antivirus",
      label: "Defender 防病毒",
      source: "defender",
      enabled: status.defender.antivirusEnabled,
    },
    {
      id: "defender:realtime",
      label: "Defender 实时保护",
      source: "defender",
      enabled: status.defender.realTimeProtectionEnabled,
    },
    {
      id: "defender:am-service",
      label: "Defender 反恶意软件服务",
      source: "defender",
      enabled: status.defender.amServiceEnabled,
    },
    ...status.firewall.profiles.map((profile) => ({
      id: `firewall:${profile.profile}`,
      label: `防火墙 · ${FIREWALL_PROFILE_LABELS[profile.profile]}`,
      source: "firewall" as const,
      enabled: profile.enabled,
    })),
  ];
}

export function enabledProtectionSwitches(
  status: NativeProtectionStatus | null,
): ProtectionSwitch[] {
  return protectionSwitches(status).filter((entry) => entry.enabled === true);
}

export function disabledProtectionSwitches(
  status: NativeProtectionStatus | null,
): ProtectionSwitch[] {
  return protectionSwitches(status).filter((entry) => entry.enabled === false);
}

export function unknownProtectionSwitches(
  status: NativeProtectionStatus | null,
): ProtectionSwitch[] {
  return protectionSwitches(status).filter((entry) => entry.enabled === null);
}

export function unknownProtectionFieldCount(
  status: NativeProtectionStatus | null,
): number {
  if (status === null) {
    return 0;
  }
  return (
    unknownProtectionSwitches(status).length +
    (status.defender.signatureVersion === null ? 1 : 0) +
    (status.defender.signatureAgeDays === null ? 1 : 0)
  );
}

export function unavailableProtectionProviderCount(
  status: NativeProtectionStatus | null,
): number {
  if (status === null) {
    return 0;
  }
  return [status.defender.state, status.firewall.state].filter(
    (state) => state === "unavailable" || state === "accessDenied" || state === "error",
  ).length;
}

export function protectionIssueCount(status: NativeProtectionStatus | null): number {
  if (status === null) {
    return 0;
  }
  return status.defender.issues.length + status.firewall.issues.length;
}

export type ProtectionReadState = "notRead" | "complete" | "partial" | "unavailable";

/**
 * Summarizes whether the native protection snapshot was readable. This is not a protection
 * verdict and must never be rendered as "protected" or "safe".
 */
export function protectionReadState(
  status: NativeProtectionStatus | null,
  failed: boolean,
): ProtectionReadState {
  if (failed) {
    return "unavailable";
  }
  if (status === null) {
    return "notRead";
  }
  if (status.observation === "unavailable") {
    return "unavailable";
  }
  if (status.observation === "partial" || unknownProtectionFieldCount(status) > 0) {
    return "partial";
  }
  return "complete";
}
