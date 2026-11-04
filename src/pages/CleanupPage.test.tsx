import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ApiProvider } from "../components/ApiProvider";
import type { SentinelApi } from "../lib/api";
import { NativeProtectionScanProvider } from "../lib/nativeProtectionScan";
import { useScanAction } from "../lib/useScanAction";
import {
  makeAffinityInspection,
  makeAssetAssociationResult,
  makeInstalledSoftware,
  makeNormalAffinityInspection,
  makeProcessList,
  makeRisk,
  makeSoftwareScan,
  makeStartupItem,
  makeStartupScan,
} from "../test/fixtures";
import { CleanupPage } from "./CleanupPage";

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
    scanInstalledSoftware: vi.fn().mockResolvedValue(makeSoftwareScan()),
    scanStartupItems: vi.fn().mockResolvedValue(makeStartupScan()),
    scanAssetAssociations: vi.fn(),
    listServices: vi.fn(),
    listScheduledTasks: vi.fn(),
    listProcesses: vi.fn().mockResolvedValue(makeProcessList()),
    inspectProcessAffinity: vi.fn().mockResolvedValue(makeAffinityInspection()),
    ...overrides,
  };
}

function CleanupHarness({ api }: { api: SentinelApi }) {
  const controller = useScanAction(api.getNativeProtectionStatus);
  return (
    <NativeProtectionScanProvider controller={controller}>
      <CleanupPage />
    </NativeProtectionScanProvider>
  );
}

function renderCleanup(api: SentinelApi): void {
  render(
    <ApiProvider api={api}>
      <CleanupHarness api={api} />
    </ApiProvider>,
  );
}

function openSection(name: string): void {
  const trigger = screen.getByRole("button", { name });
  if (trigger.getAttribute("aria-expanded") === "false") {
    fireEvent.click(trigger);
  }
}

async function scanSoftware(): Promise<void> {
  openSection("软件");
  fireEvent.click(screen.getByRole("button", { name: "开始只读盘点" }));
  await screen.findByRole("table", { name: "已安装软件列表" });
}

describe("CleanupPage read-only software inventory", () => {
  it("shows scoped evidence and offers no uninstall or delete action", async () => {
    const api = createApi();
    renderCleanup(api);

    expect(screen.getByRole("heading", { level: 1, name: "软件体检" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "软件" })).toBeInTheDocument();
    expect(screen.getByText("尚未读取")).toBeInTheDocument();
    expect(api.scanInstalledSoftware).not.toHaveBeenCalled();
    expect(api.getNativeProtectionStatus).not.toHaveBeenCalled();
    expect(api.scanAssetAssociations).not.toHaveBeenCalled();

    await scanSoftware();

    expect(await screen.findByText("虚构样本应用")).toBeInTheDocument();
    expect(api.scanInstalledSoftware).toHaveBeenCalledTimes(1);
    expect(screen.getAllByText("已读取").length).toBeGreaterThan(0);
    expect(screen.getByText("数据来源")).toBeInTheDocument();
    expect(screen.getByText(/不包含 Microsoft Store 应用/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "查看 虚构样本应用 的详情" }));

    expect(screen.getByText(/TEST-INV-000/)).toBeInTheDocument();
    expect(screen.getByText("存在（内容未读取）")).toBeInTheDocument();
    expect(screen.queryByText(/uninstall\.exe/i)).not.toBeInTheDocument();
    expect(screen.getByText(/本阶段不会卸载软件或删除残留/)).toBeInTheDocument();

    expect(screen.queryByRole("button", { name: /卸载/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /删除/ })).not.toBeInTheDocument();
  });

  it("shares one association snapshot between the posture overview and software detail", async () => {
    const api = createApi({
      scanAssetAssociations: vi.fn().mockResolvedValue(makeAssetAssociationResult()),
    });
    renderCleanup(api);

    await scanSoftware();
    fireEvent.click(screen.getByRole("button", { name: "查看 虚构样本应用 的详情" }));
    fireEvent.click(screen.getByRole("button", { name: "读取资产关联" }));

    expect((await screen.findAllByText("1 条关系")).length).toBeGreaterThan(0);
    expect(screen.getByText(/1 条关联线索/)).toBeInTheDocument();
    expect(api.scanAssetAssociations).toHaveBeenCalledTimes(1);
    expect(api.getNativeProtectionStatus).not.toHaveBeenCalled();
    expect(screen.getAllByText("精确路径").length).toBeGreaterThan(1);
  });

  it("offers separate read-only services and scheduled task disclosures", () => {
    const api = createApi();
    renderCleanup(api);

    openSection("服务");
    expect(screen.getByRole("button", { name: "服务" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    expect(screen.getByRole("button", { name: "开始只读盘点" })).toBeInTheDocument();
    expect(api.listServices).not.toHaveBeenCalled();

    openSection("计划任务");
    expect(
      screen.getAllByRole("button", { name: "开始只读盘点" }).length,
    ).toBeGreaterThan(1);
    expect(api.listScheduledTasks).not.toHaveBeenCalled();
  });

  it("marks component entries as non-user software instead of showing a configuration anomaly", async () => {
    const componentRisk = makeRisk({
      scope: "softwareMetadata",
      level: "noClearIssue",
      confidence: "low",
      ruleIds: ["INV-COMPONENT-000"],
      summary: "已识别为系统组件或不可移除组件；未按普通用户软件处理。",
      evidence: [
        {
          code: "INV-CLASSIFICATION",
          statement: "SystemComponent/NoRemove 注册表语义表明它不是普通用户软件。",
          source: "fictional test fixture",
        },
      ],
    });
    const systemComponent = makeInstalledSoftware({
      id: "test:system-component",
      displayName: "虚构系统组件",
      classification: "systemComponent",
      systemComponent: 1,
      systemComponentState: "present",
      noRemove: null,
      installLocationState: "unknown",
      risk: componentRisk,
    });
    const api = createApi({
      scanInstalledSoftware: vi.fn().mockResolvedValue(
        makeSoftwareScan({ items: [systemComponent] }),
      ),
    });
    renderCleanup(api);
    await scanSoftware();

    expect(screen.getAllByText(/系统组件/).length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole("button", { name: "查看 虚构系统组件 的详情" }));

    expect(
      screen.getByText("这是系统组件或组件类条目，不是普通用户软件。"),
    ).toBeInTheDocument();
    const detail = screen.getByLabelText("软件详情");
    expect(within(detail).queryByText("配置异常")).not.toBeInTheDocument();
  });

  it("shows an explicit incomplete-result warning and never claims a complete scan", async () => {
    const api = createApi({
      scanInstalledSoftware: vi.fn().mockResolvedValue(
        makeSoftwareScan({
          truncated: true,
          issues: [
            {
              code: "inventoryTruncated",
              message: "已达到每视图扫描上限。",
              source: "fictional registry view",
            },
          ],
        }),
      ),
    });
    renderCleanup(api);
    await scanSoftware();

    expect(screen.getByText("软件盘点结果不完整")).toBeInTheDocument();
    expect(screen.getByText(/已达到条目、时间或缓冲上限/)).toBeInTheDocument();
    expect(screen.queryByText(/所有条目在本次元数据范围内/)).not.toBeInTheDocument();
  });

  it("paginates large inventories instead of rendering every row at once", async () => {
    const items = Array.from({ length: 60 }, (_, index) =>
      makeInstalledSoftware({
        id: `test:software:${index + 1}`,
        displayName: `虚构软件 ${String(index + 1).padStart(2, "0")}`,
      }),
    );
    const api = createApi({
      scanInstalledSoftware: vi.fn().mockResolvedValue(makeSoftwareScan({ items })),
    });
    renderCleanup(api);
    await scanSoftware();

    const table = screen.getByRole("table", { name: "已安装软件列表" });
    expect(within(table).getAllByRole("button")).toHaveLength(50);
    expect(screen.getByText("第 1 / 2 页")).toBeInTheDocument();
    expect(screen.queryByText("虚构软件 60")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "下一页" }));

    expect(await screen.findByText("虚构软件 60")).toBeInTheDocument();
    expect(screen.getByText("第 2 / 2 页")).toBeInTheDocument();
  });
});

describe("CleanupPage startup inspection", () => {
  it("records unquoted path ambiguity separately from a missing file", async () => {
    const ambiguousRisk = makeRisk({
      scope: "startupMetadata",
      level: "insufficientEvidence",
      confidence: "unknown",
      ruleIds: ["STARTUP-005"],
      summary:
        "启动命令包含未加引号的路径，Windows 可能以不同方式解释可执行文件；未进行存在性判断。",
      evidence: [
        {
          code: "STARTUP-UNQUOTED-PATH",
          statement: "命令未加完整引号；这不是文件不存在的证据。",
          source: "fictional test fixture",
        },
      ],
    });
    const item = makeStartupItem({
      id: "test:startup:ambiguous",
      name: "歧义启动项",
      targetPath: null,
      targetCandidate: "C:\\Program Files\\Example\\app.exe",
      targetState: "unknown",
      pathResolution: "ambiguousUnquoted",
      risk: ambiguousRisk,
    });
    const api = createApi({
      scanStartupItems: vi.fn().mockResolvedValue(
        makeStartupScan({
          items: [item],
          issues: [
            {
              code: "unquotedPath",
              message: "1 个启动命令包含未加引号的歧义路径。",
              source: "fictional registry view",
            },
          ],
        }),
      ),
    });
    renderCleanup(api);

    openSection("启动项");
    fireEvent.click(screen.getByRole("button", { name: "开始启动项检查" }));
    expect(await screen.findByText("歧义启动项")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "查看 歧义启动项 的详情" }));

    expect(screen.getAllByText(/未加引号的歧义路径/).length).toBeGreaterThan(0);
    expect(screen.getByText("unquotedPath")).toBeInTheDocument();
    expect(
      screen.getByText("C:\\Program Files\\Example\\app.exe"),
    ).toBeInTheDocument();
    expect(screen.queryByText(/目标不存在/)).not.toBeInTheDocument();
  });

  it("shows access-denied paths as unknown and does not expose a disable action", async () => {
    const deniedRisk = makeRisk({
      scope: "startupMetadata",
      level: "insufficientEvidence",
      confidence: "unknown",
      ruleIds: ["STARTUP-006"],
      summary: "启动项目标路径访问被拒绝，文件是否存在保持未知；当前为证据不足。",
    });
    const item = makeStartupItem({
      id: "test:startup:denied",
      name: "访问受限启动项",
      targetState: "accessDenied",
      pathResolution: "quotedExecutable",
      risk: deniedRisk,
    });
    const api = createApi({
      scanStartupItems: vi.fn().mockResolvedValue(makeStartupScan({ items: [item] })),
    });
    renderCleanup(api);

    openSection("启动项");
    fireEvent.click(screen.getByRole("button", { name: "开始启动项检查" }));
    expect(await screen.findByText("访问受限启动项")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "查看 访问受限启动项 的详情" }));

    expect(screen.getAllByText(/路径访问被拒绝/).length).toBeGreaterThan(0);
    expect(screen.queryByRole("button", { name: /禁用/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /删除/ })).not.toBeInTheDocument();
  });

  it("surfaces a registry type mismatch and never falls back to ordinary user software", async () => {
    const mismatchRisk = makeRisk({
      scope: "softwareMetadata",
      level: "insufficientEvidence",
      confidence: "unknown",
      ruleIds: ["INV-COMPONENT-UNKNOWN"],
      summary: "组件字段读取异常，无法确认软件类别。",
    });
    const item = makeInstalledSoftware({
      id: "test:component-mismatch",
      displayName: "虚构类型异常组件",
      classification: "unknown",
      systemComponent: null,
      systemComponentState: "typeMismatch",
      risk: mismatchRisk,
    });
    const api = createApi({
      scanInstalledSoftware: vi.fn().mockResolvedValue(
        makeSoftwareScan({
          items: [item],
          issues: [
            {
              code: "registryValueTypeMismatch",
              message: "字段 SystemComponent 期望 REG_DWORD；字段类型与预期不一致，未按缺失处理。",
              source: "fictional uninstall key",
            },
          ],
        }),
      ),
    });
    renderCleanup(api);
    await scanSoftware();

    fireEvent.click(
      screen.getByRole("button", { name: "查看 虚构类型异常组件 的详情" }),
    );

    expect(screen.getAllByText(/类型不匹配/).length).toBeGreaterThan(0);
    expect(screen.getByText(/不会回退为普通用户软件/)).toBeInTheDocument();
    expect(screen.getByText("registryValueTypeMismatch")).toBeInTheDocument();
    expect(screen.getAllByText("类别未知").length).toBeGreaterThan(0);
  });
});

describe("CleanupPage bounded process snapshot", () => {
  it("marks a capped process snapshot as incomplete instead of a full list", async () => {
    const api = createApi({
      listProcesses: vi.fn().mockResolvedValue(
        makeProcessList({
          truncated: true,
          issues: [
            {
              code: "processInventoryTruncated",
              message: "进程快照达到安全上限。",
              source: "fictional Toolhelp32",
            },
          ],
        }),
      ),
    });
    renderCleanup(api);

    openSection("进程");
    fireEvent.click(screen.getByRole("button", { name: "刷新进程快照" }));

    expect(await screen.findByText("进程快照结果不完整")).toBeInTheDocument();
    expect(screen.getByText(/已达到条目、时间或缓冲上限/)).toBeInTheDocument();
    expect(screen.getByText("个已收集进程（不完整）")).toBeInTheDocument();
    expect(screen.getByText("processInventoryTruncated")).toBeInTheDocument();
    expect(screen.queryByText("个可见进程")).not.toBeInTheDocument();
  });
});

describe("CleanupPage CPU affinity inspection", () => {
  it("keeps a normal affinity result scoped and shows an overall insufficient-evidence verdict", async () => {
    const api = createApi({
      inspectProcessAffinity: vi.fn().mockResolvedValue(makeNormalAffinityInspection()),
    });
    renderCleanup(api);

    openSection("进程");
    fireEvent.click(screen.getByRole("button", { name: "刷新进程快照" }));
    expect(await screen.findByText("fictional-process.exe")).toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", {
        name: "检查 PID 4242 的亲和性与签名",
      }),
    );

    expect(await screen.findByText("处理器范围与系统一致")).toBeInTheDocument();
    expect(api.inspectProcessAffinity).toHaveBeenCalledWith(
      4242,
      "pidct1:00000000000000a1",
    );
    expect(screen.getByText(/亲和性范围内未发现异常/)).toBeInTheDocument();

    const overall = screen.getByLabelText("整体进程结论");
    expect(within(overall).getByText("证据不足")).toBeInTheDocument();
    expect(within(overall).queryByText("检查范围内未发现异常")).not.toBeInTheDocument();
    expect(screen.getByText(/不代表进程整体安全/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /结束进程/ })).not.toBeInTheDocument();
  });

  it("separates a restricted affinity configuration from the overall process conclusion", async () => {
    const api = createApi();
    renderCleanup(api);

    openSection("进程");
    fireEvent.click(screen.getByRole("button", { name: "刷新进程快照" }));

    expect(await screen.findByText("fictional-process.exe")).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", {
        name: "检查 PID 4242 的亲和性与签名",
      }),
    );

    expect(await screen.findByText("处理器范围受限")).toBeInTheDocument();
    expect(screen.getAllByText("0–3").length).toBeGreaterThan(0);
    expect(screen.getAllByText("0–19").length).toBeGreaterThan(0);
    expect(screen.getAllByText(/不等同于恶意行为/).length).toBeGreaterThan(0);
    expect(screen.getByText(/不能提供设置亲和性的调用者和时间/)).toBeInTheDocument();
    expect(screen.getByText("签名有效")).toBeInTheDocument();
    expect(screen.getByText("3.2%")).toBeInTheDocument();
    expect(screen.getByText(/磁盘文件签名与哈希/)).toBeInTheDocument();
    expect(
      screen.getByText("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    ).toBeInTheDocument();
    expect(screen.getByText(/不会修改处理器亲和性/)).toBeInTheDocument();

    expect(screen.queryByRole("button", { name: /结束进程/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /设置亲和性/ })).not.toBeInTheDocument();
  });
});
