import { describe, expect, it } from "vitest";

import type { RiskLevel } from "../types";
import {
  formatCheckedAt,
  formatCpuIndices,
  getRiskPresentation,
  protectionStateLabel,
  riskRank,
  riskScopeLabel,
  sensorStateLabel,
  startupSourceLabel,
} from "./presentation";

describe("risk presentation", () => {
  it.each<[RiskLevel, string]>([
    ["confirmedThreat", "已确认威胁"],
    ["highRiskSuspicious", "高风险可疑行为"],
    ["potentialUnwanted", "潜在不受欢迎软件"],
    ["userUnwanted", "用户不需要的软件"],
    ["configurationAnomaly", "配置异常"],
    ["noClearIssue", "检查范围内未发现异常"],
    ["insufficientEvidence", "证据不足"],
  ])("labels %s without upgrading it to a virus verdict", (level, label) => {
    expect(getRiskPresentation(level).label).toBe(label);
  });

  it("never gives a scoped no-clear result a green safety tone", () => {
    const presentation = getRiskPresentation("noClearIssue");

    expect(presentation.tone).toBe("neutral");
    expect(presentation.description).toContain("不代表对象安全");
  });

  it("keeps confirmed, suspicious, insufficient, and scoped-clear language distinct", () => {
    const labels = [
      getRiskPresentation("confirmedThreat").label,
      getRiskPresentation("highRiskSuspicious").label,
      getRiskPresentation("insufficientEvidence").label,
      getRiskPresentation("noClearIssue").label,
    ];

    expect(new Set(labels).size).toBe(4);
  });

  it("sorts confirmed evidence ahead of unknown or scoped-clear states", () => {
    expect(riskRank("confirmedThreat")).toBeLessThan(riskRank("insufficientEvidence"));
    expect(riskRank("potentialUnwanted")).toBeLessThan(riskRank("noClearIssue"));
  });

  it("shows the evidence scope instead of implying an overall object verdict", () => {
    expect(riskScopeLabel("affinityConfiguration")).toBe("亲和性配置");
    expect(riskScopeLabel("processOverall")).toBe("进程整体");
    expect(riskScopeLabel("softwareMetadata")).toBe("软件元数据");
    expect(riskScopeLabel("startupMetadata")).toBe("启动项元数据");
  });
});

describe("status labels", () => {
  it("never renders an unknown protection state as protected", () => {
    expect(protectionStateLabel("unknown")).toBe("状态未知");
    expect(protectionStateLabel("degraded")).toBe("保护降级");
  });

  it("distinguishes unimplemented sensors from sensor failures", () => {
    expect(sensorStateLabel("notImplemented")).toBe("尚未实现");
    expect(sensorStateLabel("degraded")).toBe("保护降级");
    expect(sensorStateLabel("notChecked")).toBe("尚未检查");
  });

  it("labels startup entry sources in user-facing terms", () => {
    expect(startupSourceLabel("registryRun")).toBe("注册表 Run");
    expect(startupSourceLabel("registryRunOnce")).toBe("注册表 RunOnce");
    expect(startupSourceLabel("startupFolder")).toBe("启动文件夹");
  });
});

describe("formatting", () => {
  it("formats a timestamp in the requested locale and timezone-independent shape", () => {
    expect(formatCheckedAt("2026-10-29T02:30:00Z", "zh-CN")).toContain("2026");
    expect(formatCheckedAt("2026-10-29T02:30:00Z", "zh-CN")).toContain("10");
  });

  it("shows an explicit unknown label for an invalid timestamp", () => {
    expect(formatCheckedAt("not-a-date", "zh-CN")).toBe("时间未知");
  });

  it("formats logical CPU sets, including holes", () => {
    expect(formatCpuIndices([0, 1, 2, 4, 7, 8, 9])).toBe("0–2, 4, 7–9");
    expect(formatCpuIndices([])).toBe("无");
    expect(formatCpuIndices([3])).toBe("3");
  });
});
