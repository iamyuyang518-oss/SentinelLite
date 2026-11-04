import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ApiProvider } from "./ApiProvider";
import { SecurityPosture } from "./SecurityPosture";
import { SentinelApiError, type SentinelApi } from "../lib/api";
import { useScanAction } from "../lib/useScanAction";
import {
  makeAssetAssociationResult,
  makeAssetRelation,
  makeAssociationSource,
  makeNativeProtectionStatus,
  makeSoftwareAssociationSummary,
} from "../test/fixtures";

function createApi(overrides: Partial<SentinelApi> = {}): SentinelApi {
  return {
    getProtectionOverview: vi.fn(),
    getNativeProtectionStatus: vi.fn(),
    getSensorStatus: vi.fn(),
    getSensorEvents: vi.fn(),
    getStorageStatus: vi.fn(),
    queryStoredEvents: vi.fn(),
    getStoredEvent: vi.fn(),
    listSensorSessions: vi.fn(),
    analyzeSession: vi.fn(),
    scanInstalledSoftware: vi.fn(),
    scanStartupItems: vi.fn(),
    scanAssetAssociations: vi.fn(),
    listServices: vi.fn(),
    listScheduledTasks: vi.fn(),
    listProcesses: vi.fn(),
    inspectProcessAffinity: vi.fn(),
    ...overrides,
  };
}

function PostureHarness({ api }: { api: SentinelApi }) {
  const association = useScanAction(api.scanAssetAssociations);
  const protection = useScanAction(api.getNativeProtectionStatus);
  return <SecurityPosture association={association} protection={protection} />;
}

function renderPosture(api: SentinelApi): void {
  render(
    <ApiProvider api={api}>
      <PostureHarness api={api} />
    </ApiProvider>,
  );
}

async function readPosture(api: SentinelApi): Promise<void> {
  renderPosture(api);
  fireEvent.click(screen.getByRole("button", { name: "读取安全态势" }));
  await waitFor(() => {
    expect(api.getNativeProtectionStatus).toHaveBeenCalledTimes(1);
    expect(api.scanAssetAssociations).toHaveBeenCalledTimes(1);
  });
}

afterEach(cleanup);

describe("SecurityPosture", () => {
  it("does not read any source automatically and does not show a safety score", () => {
    const api = createApi();
    renderPosture(api);

    expect(screen.getByText("尚未读取")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "读取安全态势" })).toBeInTheDocument();
    expect(screen.getAllByText("尚未读取").length).toBeGreaterThan(0);
    expect(api.getNativeProtectionStatus).not.toHaveBeenCalled();
    expect(api.scanAssetAssociations).not.toHaveBeenCalled();
    expect(screen.queryByText("保护正常")).not.toBeInTheDocument();
    expect(screen.queryByText("已确认威胁")).not.toBeInTheDocument();
  });

  it("shows loading status instead of pretending snapshots are unread", () => {
    const api = createApi({
      getNativeProtectionStatus: vi.fn().mockReturnValue(new Promise<never>(() => {})),
      scanAssetAssociations: vi.fn().mockReturnValue(new Promise<never>(() => {})),
    });
    renderPosture(api);

    fireEvent.click(screen.getByRole("button", { name: "读取安全态势" }));

    expect(screen.getAllByText("正在读取…").length).toBeGreaterThan(0);
  });

  it("integrates protection, five asset sources and evidence without destructive actions", async () => {
    const api = createApi({
      getNativeProtectionStatus: vi.fn().mockResolvedValue(makeNativeProtectionStatus()),
      scanAssetAssociations: vi.fn().mockResolvedValue(
        makeAssetAssociationResult({
          relations: [
            makeAssetRelation({
              evidenceValue: "C:\\Users\\Example\\AppData\\Local\\Fictional\\app.exe",
              softwareEvidencePath: "C:\\Users\\Example\\AppData\\Local\\Fictional\\app.exe",
              target: {
                kind: "process",
                id: "process:4242:pidct1:00000000000000a1",
                name: "app.exe",
                detail: "PID 4242",
                path: "C:\\Users\\Example\\AppData\\Local\\Fictional\\app.exe",
                observation: "observed",
              },
            }),
          ],
        }),
      ),
    });

    await readPosture(api);
    fireEvent.click(await screen.findByRole("button", { name: "系统防护" }));
    fireEvent.click(await screen.findByRole("button", { name: "关联信息" }));

    expect(await screen.findByText("精确路径")).toBeInTheDocument();
    expect(api.getNativeProtectionStatus).toHaveBeenCalledTimes(1);
    expect(api.scanAssetAssociations).toHaveBeenCalledTimes(1);
    expect(screen.getAllByText("来源已读取").length).toBeGreaterThan(0);
    expect(screen.getByText("软件盘点")).toBeInTheDocument();
    expect(screen.getByText("进程快照")).toBeInTheDocument();
    expect(screen.getByText("服务盘点")).toBeInTheDocument();
    expect(screen.getByText("计划任务盘点")).toBeInTheDocument();
    expect(screen.getByText("启动项盘点")).toBeInTheDocument();
    expect(screen.getAllByText("精确路径").length).toBeGreaterThan(0);
    expect(screen.getByText("规范化路径完全一致")).toBeInTheDocument();
    expect(screen.getAllByText(/证据类型：/).length).toBeGreaterThan(0);
    expect(
      screen.getAllByText(/C:\\Users\\\*\*\*\\AppData\\Local\\Fictional\\app.exe/).length,
    ).toBeGreaterThan(0);
    expect(screen.queryByText(/C:\\Users\\Example/)).not.toBeInTheDocument();
    expect(screen.getByText("Windows Defender")).toBeInTheDocument();
    expect(screen.getAllByText("已启用").length).toBeGreaterThan(0);
    expect(screen.queryByText("保护正常")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /卸载|删除|清理|运行|停止|禁用/ })).not.toBeInTheDocument();
  });

  it("keeps partial and unknown protection fields separate from enabled and disabled facts", async () => {
    const status = makeNativeProtectionStatus({
      observation: "partial",
      defender: {
        ...makeNativeProtectionStatus().defender,
        state: "partial",
        amServiceEnabled: false,
        signatureVersion: null,
        issues: [
          {
            code: "DEFENDER-FIELD-MISSING",
            message: "Defender 状态缺少 AntivirusSignatureVersion 字段；该项保持未知。",
          },
        ],
      },
      firewall: {
        ...makeNativeProtectionStatus().firewall,
        state: "partial",
        profiles: [
          { profile: "domain", enabled: true },
          { profile: "private", enabled: null },
          { profile: "public", enabled: true },
        ],
        issues: [
          {
            code: "FIREWALL-ACCESS-DENIED",
            message: "没有权限读取 Windows 防火墙专用网络状态。当前状态未知。",
          },
        ],
      },
    });
    const api = createApi({
      getNativeProtectionStatus: vi.fn().mockResolvedValue(status),
      scanAssetAssociations: vi.fn().mockResolvedValue(
        makeAssetAssociationResult({
          observation: "partial",
          sources: [
            makeAssociationSource({
              source: "processSnapshot",
              observation: "error",
              itemCount: 0,
              addressableCount: 0,
            }),
          ],
          software: [
            makeSoftwareAssociationSummary({
              state: "insufficientEvidence",
              relationCount: 0,
              strongRelationCount: 0,
            }),
          ],
          relations: [],
        }),
      ),
    });

    await readPosture(api);
    fireEvent.click(await screen.findByRole("button", { name: "系统防护" }));
    fireEvent.click(await screen.findByRole("button", { name: "关联信息" }));

    expect((await screen.findAllByText("部分读取")).length).toBeGreaterThan(0);
    expect(screen.getByText(/未启用：/)).toHaveTextContent(
      "Defender 反恶意软件服务",
    );
    expect(screen.getAllByText("未知").length).toBeGreaterThan(0);
    expect(screen.getAllByText("查询失败").length).toBeGreaterThan(0);
    expect(screen.getAllByText("证据不足").length).toBeGreaterThan(0);
    expect(screen.getByText(/证据不足不等于安全/)).toBeInTheDocument();
    expect(screen.queryByText("保护正常")).not.toBeInTheDocument();
  });

  it("does not render an unavailable persistent source as zero items", async () => {
    const base = makeAssetAssociationResult();
    const api = createApi({
      getNativeProtectionStatus: vi.fn().mockResolvedValue(makeNativeProtectionStatus()),
      scanAssetAssociations: vi.fn().mockResolvedValue(
        makeAssetAssociationResult({
          sources: base.sources.map((source) =>
            source.source === "scheduledTaskInventory"
              ? {
                  ...source,
                  observation: "unavailable",
                  itemCount: 0,
                  addressableCount: 0,
                }
              : source,
          ),
        }),
      ),
    });

    await readPosture(api);
    fireEvent.click(await screen.findByRole("button", { name: "关联信息" }));

    expect(await screen.findByText(/计划任务 来源不可用/)).toBeInTheDocument();
    expect(screen.queryByText(/计划任务 0 项/)).not.toBeInTheDocument();
    expect(screen.getByText(/1 个来源不可用/)).toBeInTheDocument();
  });

  it("keeps protection triage visible when the association source fails", async () => {
    const api = createApi({
      getNativeProtectionStatus: vi.fn().mockResolvedValue(makeNativeProtectionStatus()),
      scanAssetAssociations: vi
        .fn()
        .mockRejectedValue(
          new SentinelApiError("associationUnavailable", "关联读取失败。", true),
        ),
    });
    renderPosture(api);

    fireEvent.click(screen.getByRole("button", { name: "读取安全态势" }));

    expect(await screen.findByText("关联读取失败。")).toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: "关联信息" }));
    expect(screen.getByRole("heading", { level: 3, name: "线索分类" })).toBeInTheDocument();
    expect(
      screen.getByText(/关联来源不可用，相关分类计数显示为 —。/),
    ).toBeInTheDocument();
    expect(screen.getByText(/6 个已启用开关/)).toBeInTheDocument();
  });

  it("clears both old snapshots when a combined refresh fails", async () => {
    const getNativeProtectionStatus = vi
      .fn()
      .mockResolvedValueOnce(makeNativeProtectionStatus())
      .mockRejectedValueOnce(
        new SentinelApiError("nativeProtectionUnavailable", "防护状态读取失败。", true),
      );
    const scanAssetAssociations = vi
      .fn()
      .mockResolvedValueOnce(makeAssetAssociationResult())
      .mockRejectedValueOnce(
        new SentinelApiError("associationUnavailable", "关联读取失败。", true),
      );
    const api = createApi({ getNativeProtectionStatus, scanAssetAssociations });

    await readPosture(api);
    expect(await screen.findByText("精确路径")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "重新读取安全态势" }));

    expect(await screen.findByText("防护状态读取失败。")).toBeInTheDocument();
    expect(await screen.findByText("关联读取失败。")).toBeInTheDocument();
    expect(screen.queryByText("精确路径")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "系统防护" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "关联信息" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("保护正常")).not.toBeInTheDocument();
  });

  it("labels filename-only and multi-candidate clues as unverified, not malicious", async () => {
    const api = createApi({
      getNativeProtectionStatus: vi.fn().mockResolvedValue(makeNativeProtectionStatus()),
      scanAssetAssociations: vi.fn().mockResolvedValue(
        makeAssetAssociationResult({
          relations: [
            makeAssetRelation({
              certainty: "weak",
              matchMethod: "sameFileNameDifferentDirectory",
              evidenceType: "executableFileName",
              evidenceValue: "app.exe",
              ambiguous: true,
              candidateSoftwareCount: 2,
            }),
          ],
          software: [
            makeSoftwareAssociationSummary({
              state: "weakOnly",
              strongRelationCount: 0,
              weakRelationCount: 1,
            }),
          ],
        }),
      ),
    });

    await readPosture(api);

    expect((await screen.findAllByText("同名文件弱线索")).length).toBeGreaterThan(0);
    expect(
      screen.getAllByText(/同一目标同时匹配 2 个软件候选/).length,
    ).toBeGreaterThan(0);
    expect(screen.getAllByText("值得核实").length).toBeGreaterThan(0);
    expect(screen.getByText(/弱线索、多候选和未启用开关值得人工核实/)).toBeInTheDocument();
    expect(screen.queryByText("已确认威胁")).not.toBeInTheDocument();
  });

  it("keeps noMatch, insufficient evidence and source unavailable distinct", async () => {
    const api = createApi({
      getNativeProtectionStatus: vi.fn().mockResolvedValue(makeNativeProtectionStatus()),
      scanAssetAssociations: vi.fn().mockResolvedValue(
        makeAssetAssociationResult({
          software: [
            makeSoftwareAssociationSummary({
              softwareId: "no-match",
              softwareName: "NoMatch",
              state: "noMatch",
              relationCount: 0,
              strongRelationCount: 0,
            }),
            makeSoftwareAssociationSummary({
              softwareId: "insufficient",
              softwareName: "Insufficient",
              state: "insufficientEvidence",
              relationCount: 0,
              strongRelationCount: 0,
            }),
            makeSoftwareAssociationSummary({
              softwareId: "unavailable",
              softwareName: "Unavailable",
              state: "sourceUnavailable",
              relationCount: 0,
              strongRelationCount: 0,
            }),
          ],
          relations: [],
        }),
      ),
    });

    await readPosture(api);

    expect(await screen.findByText("当前快照未匹配")).toBeInTheDocument();
    expect(screen.getAllByText("证据不足").length).toBeGreaterThan(0);
    expect(
      screen.getByText("6 个防护开关已启用 · 0 条关联线索"),
    ).toBeInTheDocument();
    expect(screen.getByText(/1 个软件来源不可用/)).toBeInTheDocument();
    expect(screen.getByText(/“当前快照未匹配”只适用于来源完整/)).toBeInTheDocument();
    expect(screen.queryByText(/uninstall\.exe/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/msiexec/i)).not.toBeInTheDocument();
  });
});
