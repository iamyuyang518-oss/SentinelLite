import { describe, expect, it } from "vitest";

import {
  associationCertaintyLabel,
  associationEvidenceTypeLabel,
  associationMatchMethodLabel,
  associationTargetKindLabel,
  pathStateLabel,
  postureReadStateLabel,
  postureTriageCategoryLabel,
  registryValueStateLabel,
  registryViewLabel,
  softwareAssociationStateLabel,
  softwareClassificationLabel,
  startupPathResolutionLabel,
  startupTargetKindLabel,
  startupValueTypeLabel,
} from "./uiLabels";

describe("inventory and path labels", () => {
  it("labels the HKCU shared view without pretending it is a WOW64 view", () => {
    expect(registryViewLabel("shared")).toBe("共享用户视图");
    expect(registryViewLabel("registry64")).toBe("64 位视图");
  });

  it("distinguishes access denied from missing and unknown", () => {
    expect(pathStateLabel("missing")).toBe("路径不存在");
    expect(pathStateLabel("accessDenied")).toBe("路径访问被拒绝（存在性未知）");
    expect(pathStateLabel("unknown")).toBe("路径状态未知");
    expect(pathStateLabel("accessDenied")).not.toBe(pathStateLabel("missing"));
  });

  it("exposes unquoted path ambiguity as its own UI state", () => {
    expect(startupPathResolutionLabel("ambiguousUnquoted")).toContain("歧义");
    expect(startupPathResolutionLabel("quotedExecutable")).toBe("带引号的可执行路径");
    expect(startupPathResolutionLabel("shortcutTargetResolved")).toContain("已解析");
  });

  it("labels startup value and target types without collapsing unknown types", () => {
    expect(startupValueTypeLabel("regSz")).toBe("REG_SZ");
    expect(startupValueTypeLabel("regExpandSz")).toBe("REG_EXPAND_SZ");
    expect(startupValueTypeLabel("other")).toBe("其他类型");
    expect(startupTargetKindLabel("shortcut")).toBe("快捷方式");
    expect(startupTargetKindLabel("startupFolderFile")).toBe("启动文件夹文件");
  });

  it("distinguishes explicit registry read failures from missing fields", () => {
    expect(registryValueStateLabel("missing")).toBe("未提供");
    expect(registryValueStateLabel("typeMismatch")).toBe("类型不匹配");
    expect(registryValueStateLabel("invalid")).toBe("值无效");
    expect(registryValueStateLabel("readError")).toBe("读取失败");
    expect(registryValueStateLabel("typeMismatch")).not.toBe(
      registryValueStateLabel("missing"),
    );
  });

  it("labels component classes instead of treating them as ordinary user software", () => {
    expect(softwareClassificationLabel("systemComponent")).toBe("系统组件");
    expect(softwareClassificationLabel("driverOrHardwareComponent")).toBe(
      "驱动或硬件组件",
    );
    expect(softwareClassificationLabel("unknown")).toBe("类别未知");
    expect(softwareClassificationLabel("userSoftware")).toBe("普通用户软件");
  });

  it("keeps exact, strong, weak and insufficient association states distinct", () => {
    expect(associationCertaintyLabel("exact")).toBe("精确路径");
    expect(associationCertaintyLabel("strong")).toBe("安装目录内路径");
    expect(associationCertaintyLabel("weak")).toBe("同名文件弱线索");
    expect(associationMatchMethodLabel("containedInInstallLocation")).toContain(
      "安装目录",
    );
    expect(associationTargetKindLabel("scheduledTask")).toBe("计划任务");
    expect(associationEvidenceTypeLabel("displayIconPath")).toBe("DisplayIcon 路径");
    expect(associationEvidenceTypeLabel("executableFileName")).toBe("可执行文件名");
    expect(softwareAssociationStateLabel("noMatch")).not.toBe(
      softwareAssociationStateLabel("insufficientEvidence"),
    );
    expect(softwareAssociationStateLabel("sourceUnavailable")).toContain("不可用");
  });

  it("labels posture readability and triage without a safety score", () => {
    expect(postureReadStateLabel("notRead")).toBe("尚未读取");
    expect(postureReadStateLabel("complete")).toBe("来源已读取");
    expect(postureReadStateLabel("partial")).toBe("部分读取");
    expect(postureReadStateLabel("unavailable")).toBe("来源不可用");
    expect(postureTriageCategoryLabel("observed")).toBe("已观察事实");
    expect(postureTriageCategoryLabel("attention")).toBe("值得核实");
    expect(postureTriageCategoryLabel("insufficient")).toBe("证据不足");
    expect(postureTriageCategoryLabel("unavailable")).toBe("来源不可用");
  });
});
