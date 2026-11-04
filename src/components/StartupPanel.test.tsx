import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ApiProvider } from "./ApiProvider";
import { StartupPanel } from "./StartupPanel";
import { SentinelApiError, type SentinelApi } from "../lib/api";
import { makeStartupItem, makeStartupScan, makeStartupSource } from "../test/fixtures";

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
    scanStartupItems: vi.fn().mockResolvedValue(makeStartupScan()),
    scanAssetAssociations: vi.fn(),
    listServices: vi.fn(),
    listScheduledTasks: vi.fn(),
    listProcesses: vi.fn(),
    inspectProcessAffinity: vi.fn(),
    ...overrides,
  };
}

function renderPanel(api: SentinelApi): void {
  render(
    <ApiProvider api={api}>
      <StartupPanel />
    </ApiProvider>,
  );
}

async function scan(api: SentinelApi): Promise<void> {
  renderPanel(api);
  fireEvent.click(screen.getByRole("button", { name: "开始启动项检查" }));
  expect(await screen.findByRole("table", { name: "启动项列表" })).toBeInTheDocument();
}

afterEach(cleanup);

describe("StartupPanel", () => {
  it("does not query startup locations before the user asks", () => {
    const api = createApi();
    renderPanel(api);

    expect(screen.getByRole("heading", { level: 2, name: "启动项只读盘点" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始启动项检查" })).toBeInTheDocument();
    expect(api.scanStartupItems).not.toHaveBeenCalled();
  });

  it("shows registry value type, source scope, target and observation state", async () => {
    const api = createApi();
    await scan(api);

    const table = screen.getByRole("table", { name: "启动项列表" });
    expect(within(table).getByText("虚构启动助手")).toBeInTheDocument();
    expect(within(table).getByText("注册表命令 · REG_SZ")).toBeInTheDocument();
    expect(within(table).getByText(/注册表 Run/)).toBeInTheDocument();
    expect(within(table).getByText(/当前用户/)).toBeInTheDocument();
    expect(
      within(table).getByText("C:\\Users\\***\\AppData\\Local\\Temp\\fictional-helper.exe"),
    ).toBeInTheDocument();
    expect(within(table).getAllByText("已读取").length).toBeGreaterThan(0);
    expect(screen.queryByRole("button", { name: /禁用|删除|运行|清理/ })).not.toBeInTheDocument();
    expect(screen.queryByText("恶意软件")).not.toBeInTheDocument();
    expect(screen.queryByText("已确认威胁")).not.toBeInTheDocument();
  });

  it("keeps shortcut target metadata explicit without rendering argument text", async () => {
    const shortcut = makeStartupItem({
      id: "test:startup:shortcut",
      name: "虚构快捷方式",
      valueName: null,
      valueType: null,
      targetKind: "shortcut",
      targetPath: "C:\\Program Files\\Example\\example.exe",
      targetState: "present",
      pathResolution: "shortcutTargetResolved",
      shortcutArgumentsPresent: true,
      shortcutWorkingDirectoryPresent: false,
      observation: "observed",
    });
    const api = createApi({
      scanStartupItems: vi.fn().mockResolvedValue(
        makeStartupScan({
          sources: [
            makeStartupSource({
              itemCount: 1,
            }),
          ],
          items: [shortcut],
        }),
      ),
    });
    await scan(api);
    fireEvent.click(screen.getByRole("button", { name: "查看 虚构快捷方式 的详情" }));

    expect(screen.getByText("快捷方式目标已解析")).toBeInTheDocument();
    expect(screen.getByText("存在（内容未读取）")).toBeInTheDocument();
    expect(screen.getByText("未提供")).toBeInTheDocument();
    expect(screen.queryByText(/--quiet|--run/)).not.toBeInTheDocument();
  });

  it("renders per-source status and item access-denied evidence without treating it as normal", async () => {
    const denied = makeStartupItem({
      id: "test:startup:denied",
      name: "访问受限启动项",
      observation: "accessDenied",
      rawValueReadable: false,
      targetPath: null,
      targetState: "unknown",
      pathResolution: "unresolved",
      issues: [
        {
          code: "STARTUP-VALUE-INVALID",
          message: "注册表值无法解码为文本；条目保留，目标保持未知。",
          source: "HKEY_CURRENT_USER\\...\\Run",
        },
      ],
    });
    const api = createApi({
      scanStartupItems: vi.fn().mockResolvedValue(
        makeStartupScan({
          observation: "partial",
          sources: [
            makeStartupSource({ itemCount: 1 }),
            makeStartupSource({
              id: "startup-folder:HKEY_LOCAL_MACHINE",
              scope: "machine",
              location: "%PROGRAMDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Startup",
              observation: "accessDenied",
              exists: null,
              itemCount: 0,
            }),
            makeStartupSource({
              id: "startup-folder:absent",
              source: "startupFolder",
              scope: "machine",
              location: "%PROGRAMDATA%\\Missing\\Startup",
              observation: "unavailable",
              exists: false,
              itemCount: 0,
            }),
          ],
          items: [denied],
        }),
      ),
    });
    await scan(api);

    expect(screen.getAllByText("无权限").length).toBeGreaterThan(0);
    expect(screen.getByText("不存在")).toBeInTheDocument();
    expect(screen.getByText("访问受限启动项")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "查看 访问受限启动项 的详情" }));
    expect(screen.getByText("注册表值无法解码为文本；条目保留，目标保持未知。")).toBeInTheDocument();
    expect(screen.getAllByText("STARTUP-VALUE-INVALID").length).toBeGreaterThan(0);
  });

  it("shows a truncation alert and never presents a partial list as complete", async () => {
    const api = createApi({
      scanStartupItems: vi.fn().mockResolvedValue(makeStartupScan({ truncated: true })),
    });
    await scan(api);

    expect(screen.getByText("启动项盘点结果不完整")).toBeInTheDocument();
    expect(screen.getByText(/后端将结果标记为不完整/)).toBeInTheDocument();
  });

  it("shows a complete first-read failure without an empty normal list", async () => {
    const api = createApi({
      scanStartupItems: vi
        .fn()
        .mockRejectedValue(
          new SentinelApiError("startupInventoryUnavailable", "启动项来源不可用。", true),
        ),
    });
    renderPanel(api);
    fireEvent.click(screen.getByRole("button", { name: "开始启动项检查" }));

    expect(await screen.findByText("启动项来源不可用。")).toBeInTheDocument();
    expect(screen.queryByRole("table", { name: "启动项列表" })).not.toBeInTheDocument();
    expect(screen.getByText(/未保留上一次结果/)).toBeInTheDocument();
  });

  it("clears the old result when a refresh fails", async () => {
    const scanStartupItems = vi
      .fn()
      .mockResolvedValueOnce(makeStartupScan())
      .mockRejectedValueOnce(
        new SentinelApiError("startupInventoryUnavailable", "启动项读取失败。", true),
      );
    const api = createApi({ scanStartupItems });
    await scan(api);
    expect(screen.getByText("虚构启动助手")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "重新盘点" }));
    expect(await screen.findByText("启动项读取失败。")).toBeInTheDocument();
    expect(screen.queryByText("虚构启动助手")).not.toBeInTheDocument();
    expect(screen.queryByRole("table", { name: "启动项列表" })).not.toBeInTheDocument();
  });
});
