import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ApiProvider } from "./ApiProvider";
import { ServiceTaskPanel } from "./ServiceTaskPanel";
import { SentinelApiError, type SentinelApi } from "../lib/api";
import {
  makeScheduledTask,
  makeScheduledTaskInventory,
  makeService,
  makeServiceInventory,
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
    listServices: vi.fn().mockResolvedValue(makeServiceInventory()),
    listScheduledTasks: vi.fn().mockResolvedValue(makeScheduledTaskInventory()),
    listProcesses: vi.fn(),
    inspectProcessAffinity: vi.fn(),
    ...overrides,
  };
}

function renderPanel(api: SentinelApi): void {
  render(
    <ApiProvider api={api}>
      <ServiceTaskPanel />
    </ApiProvider>,
  );
}

afterEach(cleanup);

describe("ServiceTaskPanel", () => {
  it("does not query either provider before the user asks", () => {
    const api = createApi();
    renderPanel(api);

    expect(screen.getByRole("heading", { level: 2, name: "Windows 服务与计划任务" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 3, name: "Windows 服务" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 3, name: "计划任务" })).toBeInTheDocument();
    expect(screen.getAllByText(/尚未读取/)).toHaveLength(2);
    expect(api.listServices).not.toHaveBeenCalled();
    expect(api.listScheduledTasks).not.toHaveBeenCalled();
  });

  it("loads services and tasks together and shows their real fields", async () => {
    const api = createApi();
    renderPanel(api);

    fireEvent.click(screen.getByRole("button", { name: "开始只读盘点" }));

    expect(await screen.findByRole("table", { name: "Windows 服务列表" })).toBeInTheDocument();
    expect(await screen.findByRole("table", { name: "计划任务列表" })).toBeInTheDocument();
    expect(api.listServices).toHaveBeenCalledTimes(1);
    expect(api.listScheduledTasks).toHaveBeenCalledTimes(1);

    const services = screen.getByRole("table", { name: "Windows 服务列表" });
    expect(within(services).getByText("ExampleSvc")).toBeInTheDocument();
    expect(within(services).getByText("运行中")).toBeInTheDocument();
    expect(within(services).getByText("自动")).toBeInTheDocument();
    expect(within(services).getByText("LocalSystem")).toBeInTheDocument();

    const tasks = screen.getByRole("table", { name: "计划任务列表" });
    expect(within(tasks).getByText("ExampleTask")).toBeInTheDocument();
    expect(within(tasks).getByText("就绪")).toBeInTheDocument();
    expect(within(tasks).getByText("已启用")).toBeInTheDocument();
    expect(within(tasks).getByText("Example Author")).toBeInTheDocument();
    expect(within(tasks).getByText("启动程序")).toBeInTheDocument();
    expect(within(tasks).getByText("C:\\Windows\\System32\\example.exe")).toBeInTheDocument();

    expect(screen.queryByText(/恶意/)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /启动服务|停止服务|运行任务|删除任务|禁用任务/ })).not.toBeInTheDocument();
  });

  it("redacts personal account and profile-path components from rows", async () => {
    renderPanel(
      createApi({
        listServices: vi.fn().mockResolvedValue(
          makeServiceInventory({
            items: [
              makeService({
                account: "CONTOSO\\alice",
                executablePath: "C:\\Users\\Alice\\AppData\\Local\\svc.exe",
              }),
            ],
          }),
        ),
        listScheduledTasks: vi.fn().mockResolvedValue(
          makeScheduledTaskInventory({
            items: [
              makeScheduledTask({
                author: "CONTOSO\\alice",
                runAs: "CONTOSO\\alice",
                actions: [
                  {
                    actionType: "exec",
                    executablePath: "C:\\Users\\Alice\\AppData\\Local\\task.exe",
                  },
                ],
              }),
            ],
          }),
        ),
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: "开始只读盘点" }));

    const services = await screen.findByRole("table", { name: "Windows 服务列表" });
    expect(within(services).getByText("CONTOSO\\***")).toBeInTheDocument();
    expect(
      within(services).getByText("C:\\Users\\***\\AppData\\Local\\svc.exe"),
    ).toBeInTheDocument();
    expect(within(services).queryByText(/Alice/)).not.toBeInTheDocument();

    const tasks = screen.getByRole("table", { name: "计划任务列表" });
    expect(within(tasks).getAllByText("CONTOSO\\***").length).toBeGreaterThanOrEqual(2);
    expect(
      within(tasks).getByText("C:\\Users\\***\\AppData\\Local\\task.exe"),
    ).toBeInTheDocument();
    expect(within(tasks).queryByText(/Alice/)).not.toBeInTheDocument();
  });

  it("keeps a successful service list visible when the task provider fails", async () => {
    const api = createApi({
      listScheduledTasks: vi.fn().mockRejectedValue(
        new SentinelApiError("taskInventoryUnavailable", "计划任务服务不可用。", true),
      ),
    });
    renderPanel(api);

    fireEvent.click(screen.getByRole("button", { name: "开始只读盘点" }));

    expect(await screen.findByRole("table", { name: "Windows 服务列表" })).toBeInTheDocument();
    expect(await screen.findByRole("heading", { level: 4, name: "计划任务列表未读取" })).toBeInTheDocument();
    expect(screen.getByText("计划任务服务不可用。")).toBeInTheDocument();
    expect(screen.queryByRole("table", { name: "计划任务列表" })).not.toBeInTheDocument();
    expect(screen.queryByText("ExampleTask")).not.toBeInTheDocument();
  });

  it("shows access-denied items as unknown, not as normal or not-provided", async () => {
    const deniedService = makeService({
      observation: "accessDenied",
      account: null,
      executablePath: null,
      issues: [
        {
          code: "SERVICE-CONFIG-UNREADABLE",
          message: "没有权限读取服务配置；启动类型、账户和路径保持未知。",
          source: "SCM",
        },
      ],
    });
    renderPanel(
      createApi({
        listServices: vi.fn().mockResolvedValue(
          makeServiceInventory({
            observation: "partial",
            items: [deniedService],
          }),
        ),
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: "开始只读盘点" }));

    const services = await screen.findByRole("table", { name: "Windows 服务列表" });
    expect(within(services).getByText("无权限")).toBeInTheDocument();
    expect(within(services).getAllByText("未知").length).toBeGreaterThanOrEqual(2);
    expect(within(services).queryByText("未提供")).not.toBeInTheDocument();
    expect(within(services).getByText("SERVICE-CONFIG-UNREADABLE")).toBeInTheDocument();
    expect(within(services).queryByText("已读取")).not.toBeInTheDocument();
  });

  it("marks truncated inventories as incomplete instead of silently dropping items", async () => {
    renderPanel(
      createApi({
        listServices: vi.fn().mockResolvedValue(
          makeServiceInventory({
            truncated: true,
            items: Array.from({ length: 50 }, (_, index) =>
              makeService({ id: `service:item${index}`, name: `item${index}` }),
            ),
          }),
        ),
        listScheduledTasks: vi.fn().mockResolvedValue(
          makeScheduledTaskInventory({
            truncated: true,
            items: [makeScheduledTask()],
          }),
        ),
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: "开始只读盘点" }));

    expect(await screen.findByText("服务结果不完整")).toBeInTheDocument();
    expect(screen.getByText("计划任务结果不完整")).toBeInTheDocument();
    expect(
      screen.getAllByText(/后端将结果标记为不完整，但没有提供可识别的原因/).length,
    ).toBeGreaterThanOrEqual(2);
  });

  it("clears old results when a refresh fails", async () => {
    const listServices = vi
      .fn()
      .mockResolvedValueOnce(makeServiceInventory())
      .mockRejectedValueOnce(
        new SentinelApiError("serviceInventoryUnavailable", "服务控制管理器不可用。", true),
      );
    renderPanel(createApi({ listServices }));

    fireEvent.click(screen.getByRole("button", { name: "开始只读盘点" }));
    expect(await screen.findByText("ExampleSvc")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "重新盘点" }));

    expect(await screen.findByRole("heading", { level: 4, name: "服务列表未读取" })).toBeInTheDocument();
    expect(screen.getByText("服务控制管理器不可用。")).toBeInTheDocument();
    expect(screen.queryByText("ExampleSvc")).not.toBeInTheDocument();
    expect(screen.getByText(/未保留上一次结果/)).toBeInTheDocument();
  });
});
