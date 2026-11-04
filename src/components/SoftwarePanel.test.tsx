import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ApiProvider } from "./ApiProvider";
import { SoftwarePanel } from "./SoftwarePanel";
import { SentinelApiError, type SentinelApi } from "../lib/api";
import { useScanAction } from "../lib/useScanAction";
import {
  makeAssetAssociationResult,
  makeAssetRelation,
  makeAssociationSource,
  makeInstalledSoftware,
  makeSoftwareAssociationSummary,
  makeSoftwareScan,
  makeSoftwareSource,
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
    scanInstalledSoftware: vi.fn().mockResolvedValue(makeSoftwareScan()),
    scanStartupItems: vi.fn(),
    scanAssetAssociations: vi.fn(),
    listServices: vi.fn(),
    listScheduledTasks: vi.fn(),
    listProcesses: vi.fn(),
    inspectProcessAffinity: vi.fn(),
    ...overrides,
  };
}

function PanelHarness({ api }: { api: SentinelApi }) {
  const association = useScanAction(api.scanAssetAssociations);
  return <SoftwarePanel association={association} />;
}

function renderPanel(api: SentinelApi): void {
  render(
    <ApiProvider api={api}>
      <PanelHarness api={api} />
    </ApiProvider>,
  );
}

async function scan(api: SentinelApi): Promise<void> {
  renderPanel(api);
  fireEvent.click(screen.getByRole("button", { name: "开始只读盘点" }));
  expect(await screen.findByRole("table", { name: "已安装软件列表" })).toBeInTheDocument();
}

afterEach(cleanup);

describe("SoftwarePanel", () => {
  it("does not query software before the user asks", () => {
    const api = createApi();
    renderPanel(api);

    expect(
      screen.getByRole("heading", { level: 2, name: "软件与组件只读盘点" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始只读盘点" })).toBeInTheDocument();
    expect(api.scanInstalledSoftware).not.toHaveBeenCalled();
  });

  it("shows fields, sources and observation without exposing uninstall command text", async () => {
    const item = makeInstalledSoftware({
      installLocation: "C:\\Users\\Example\\AppData\\Local\\Fictional",
      displayIconPath: "C:\\Users\\Example\\AppData\\Local\\Fictional\\app.exe",
    });
    const api = createApi({
      scanInstalledSoftware: vi.fn().mockResolvedValue(makeSoftwareScan({ items: [item] })),
    });
    await scan(api);

    const table = screen.getByRole("table", { name: "已安装软件列表" });
    expect(within(table).getByText("虚构样本应用")).toBeInTheDocument();
    expect(within(table).getByText(/版本：1\.2\.3-test/)).toBeInTheDocument();
    expect(within(table).getByText(/Example Publisher/)).toBeInTheDocument();
    expect(within(table).getByText("本机范围")).toBeInTheDocument();
    expect(within(table).getByText("64 位视图")).toBeInTheDocument();
    expect(
      within(table).getByText("C:\\Users\\***\\AppData\\Local\\Fictional"),
    ).toBeInTheDocument();
    expect(within(table).getAllByText("已读取").length).toBeGreaterThan(0);

    fireEvent.click(screen.getByRole("button", { name: "查看 虚构样本应用 的详情" }));
    expect(screen.getByText("存在（内容未读取）")).toBeInTheDocument();
    expect(
      screen.getByText("C:\\Users\\***\\AppData\\Local\\Fictional\\app.exe"),
    ).toBeInTheDocument();
    expect(screen.queryByText(/uninstall\.exe/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/msiexec/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /卸载|删除|清理|运行/ })).not.toBeInTheDocument();
  });

  it("keeps source failures and item access-denied evidence visibly partial", async () => {
    const item = makeInstalledSoftware({
      observation: "accessDenied",
      version: null,
      versionState: "typeMismatch",
      issues: [
        {
          code: "SOFTWARE-FIELD-TYPE-MISMATCH",
          message: "字段 DisplayVersion 期望 REG_SZ/REG_EXPAND_SZ；未按缺失处理。",
          source: "HKEY_CURRENT_USER\\...\\Uninstall\\Fictional",
        },
      ],
    });
    const api = createApi({
      scanInstalledSoftware: vi.fn().mockResolvedValue(
        makeSoftwareScan({
          observation: "partial",
          sources: [
            makeSoftwareSource({ itemCount: 1 }),
            makeSoftwareSource({
              id: "HKEY_CURRENT_USER:shared:...",
              scope: "user",
              registryView: "shared",
              observation: "accessDenied",
              exists: null,
              itemCount: 0,
            }),
            makeSoftwareSource({
              id: "HKEY_LOCAL_MACHINE:registry32:...",
              registryView: "registry32",
              observation: "unavailable",
              exists: false,
              itemCount: 0,
            }),
          ],
          items: [item],
        }),
      ),
    });
    await scan(api);

    expect(screen.getAllByText("无权限").length).toBeGreaterThan(0);
    expect(screen.getByText("不存在")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "查看 虚构样本应用 的详情" }));
    expect(screen.getAllByText(/类型不匹配/).length).toBeGreaterThan(0);
    expect(
      screen.getAllByText("SOFTWARE-FIELD-TYPE-MISMATCH").length,
    ).toBeGreaterThan(0);
  });

  it("does not silently normalize a field read error to not-provided", async () => {
    const item = makeInstalledSoftware({
      observation: "partial",
      publisher: null,
      publisherState: "readError",
      installLocation: null,
      installLocationValueState: "readError",
    });
    const api = createApi({
      scanInstalledSoftware: vi.fn().mockResolvedValue(makeSoftwareScan({ items: [item] })),
    });
    await scan(api);

    expect(screen.getByText(/发布者：读取失败/)).toBeInTheDocument();
    expect(screen.getAllByText(/读取失败/).length).toBeGreaterThanOrEqual(2);

    fireEvent.click(screen.getByRole("button", { name: "查看 虚构样本应用 的详情" }));
    expect(screen.getAllByText(/读取失败/).length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText(/不会回退为普通用户软件/)).toBeInTheDocument();
  });

  it("shows truncation as incomplete rather than a complete inventory", async () => {
    const api = createApi({
      scanInstalledSoftware: vi.fn().mockResolvedValue(
        makeSoftwareScan({
          truncated: true,
          observation: "partial",
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
    await scan(api);

    expect(screen.getByText("软件盘点结果不完整")).toBeInTheDocument();
    expect(screen.getByText(/已达到条目、时间或缓冲上限/)).toBeInTheDocument();
    expect(screen.getByText(/未列出项不代表不存在/)).toBeInTheDocument();
  });

  it("shows a complete first-read failure without an empty normal list", async () => {
    const api = createApi({
      scanInstalledSoftware: vi
        .fn()
        .mockRejectedValue(
          new SentinelApiError("softwareInventoryUnavailable", "软件来源不可用。", true),
        ),
    });
    renderPanel(api);
    fireEvent.click(screen.getByRole("button", { name: "开始只读盘点" }));

    expect(await screen.findByText("软件来源不可用。")).toBeInTheDocument();
    expect(screen.queryByRole("table", { name: "已安装软件列表" })).not.toBeInTheDocument();
    expect(screen.getByText(/未保留上一次结果/)).toBeInTheDocument();
  });

  it("clears the old result when a refresh fails", async () => {
    const scanInstalledSoftware = vi
      .fn()
      .mockResolvedValueOnce(makeSoftwareScan())
      .mockRejectedValueOnce(
        new SentinelApiError("softwareInventoryUnavailable", "软件读取失败。", true),
      );
    const api = createApi({ scanInstalledSoftware });
    await scan(api);
    expect(screen.getByText("虚构样本应用")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "重新盘点" }));
    expect(await screen.findByText("软件读取失败。")).toBeInTheDocument();
    expect(screen.queryByText("虚构样本应用")).not.toBeInTheDocument();
    expect(screen.queryByRole("table", { name: "已安装软件列表" })).not.toBeInTheDocument();
  });

  it("does not read asset associations until the detail action is used", async () => {
    const api = createApi();
    await scan(api);
    fireEvent.click(screen.getByRole("button", { name: "查看 虚构样本应用 的详情" }));

    expect(screen.getByRole("heading", { name: "资产关联" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "读取资产关联" })).toBeInTheDocument();
    expect(api.scanAssetAssociations).not.toHaveBeenCalled();
  });

  it("shows explainable association evidence without command lines or destructive actions", async () => {
    const relation = makeAssetRelation({
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
    });
    const api = createApi({
      scanAssetAssociations: vi.fn().mockResolvedValue(
        makeAssetAssociationResult({
          relations: [relation],
          software: [makeSoftwareAssociationSummary({ relationCount: 1 })],
        }),
      ),
    });
    await scan(api);
    fireEvent.click(screen.getByRole("button", { name: "查看 虚构样本应用 的详情" }));
    fireEvent.click(screen.getByRole("button", { name: "读取资产关联" }));

    expect(await screen.findByText("精确路径")).toBeInTheDocument();
    expect(screen.getByText("PID 4242")).toBeInTheDocument();
    expect(screen.getByText("规范化路径完全一致")).toBeInTheDocument();
    expect(screen.getAllByText(/C:\\Users\\\*\*\*\\/).length).toBeGreaterThan(0);
    expect(screen.queryByText(/C:\\Users\\Example/)).not.toBeInTheDocument();
    expect(screen.queryByText(/uninstall\.exe/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/msiexec/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /卸载|删除|清理|运行/ })).not.toBeInTheDocument();
  });

  it("shows insufficient evidence instead of claiming there is no association", async () => {
    const api = createApi({
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
          truncated: true,
        }),
      ),
    });
    await scan(api);
    fireEvent.click(screen.getByRole("button", { name: "查看 虚构样本应用 的详情" }));
    fireEvent.click(screen.getByRole("button", { name: "读取资产关联" }));

    expect(await screen.findByText("证据不足")).toBeInTheDocument();
    expect(screen.getByText(/不能据此断言/)).toBeInTheDocument();
    expect(screen.queryByText("当前快照未匹配")).not.toBeInTheDocument();
    expect(screen.getByText(/结果不完整/)).toBeInTheDocument();
  });

  it("labels filename-only and multi-candidate evidence as weak rather than certain", async () => {
    const api = createApi({
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
    await scan(api);
    fireEvent.click(screen.getByRole("button", { name: "查看 虚构样本应用 的详情" }));
    fireEvent.click(screen.getByRole("button", { name: "读取资产关联" }));

    expect(await screen.findByText("同名文件弱线索")).toBeInTheDocument();
    expect(screen.getByText(/同一目标同时匹配 2 个软件候选/)).toBeInTheDocument();
    expect(screen.getByText(/弱线索不能证明软件归属或恶意性/)).toBeInTheDocument();
    expect(screen.getByText("仅弱线索")).toBeInTheDocument();
  });

  it("keeps a completed no-match distinct from insufficient evidence", async () => {
    const api = createApi({
      scanAssetAssociations: vi.fn().mockResolvedValue(
        makeAssetAssociationResult({
          software: [
            makeSoftwareAssociationSummary({
              state: "noMatch",
              relationCount: 0,
              strongRelationCount: 0,
              weakRelationCount: 0,
            }),
          ],
          relations: [],
        }),
      ),
    });
    await scan(api);
    fireEvent.click(screen.getByRole("button", { name: "查看 虚构样本应用 的详情" }));
    fireEvent.click(screen.getByRole("button", { name: "读取资产关联" }));

    expect(await screen.findByText("当前快照未匹配")).toBeInTheDocument();
    expect(screen.getByText(/没有关联不等于异常/)).toBeInTheDocument();
    expect(screen.queryByText("证据不足")).not.toBeInTheDocument();
  });

  it("clears an old association result when re-reading fails", async () => {
    const scanAssetAssociations = vi
      .fn()
      .mockResolvedValueOnce(makeAssetAssociationResult())
      .mockRejectedValueOnce(
        new SentinelApiError("associationUnavailable", "关联读取失败。", true),
      );
    const api = createApi({ scanAssetAssociations });
    await scan(api);
    fireEvent.click(screen.getByRole("button", { name: "查看 虚构样本应用 的详情" }));
    fireEvent.click(screen.getByRole("button", { name: "读取资产关联" }));
    expect(await screen.findByText("精确路径")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "重新读取关联" }));
    expect(await screen.findByText("关联读取失败。")).toBeInTheDocument();
    expect(screen.queryByText("精确路径")).not.toBeInTheDocument();
    expect(screen.getByText(/未保留上一次关联结果/)).toBeInTheDocument();
  });
});
