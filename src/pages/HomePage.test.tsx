import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ApiProvider } from "../components/ApiProvider";
import { SentinelApiError, type SentinelApi } from "../lib/api";
import { NativeProtectionScanProvider } from "../lib/nativeProtectionScan";
import { useScanAction } from "../lib/useScanAction";
import { SENSOR_STATUS_REFRESH_MS } from "../lib/useSensorStatus";
import {
  makeNativeProtectionStatus,
  makeOverview,
  makeSensorStatus,
  makeSensorStatusReport,
} from "../test/fixtures";
import { HomePage } from "./HomePage";

function createApi(overrides: Partial<SentinelApi> = {}): SentinelApi {
  return {
    getProtectionOverview: vi.fn().mockResolvedValue(makeOverview()),
    getNativeProtectionStatus: vi.fn(),
    getSensorStatus: vi.fn().mockResolvedValue(makeSensorStatusReport()),
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

function HomeHarness({ api }: { api: SentinelApi }) {
  const controller = useScanAction(api.getNativeProtectionStatus);
  return (
    <NativeProtectionScanProvider controller={controller}>
      <HomePage onOpenCleanup={() => undefined} />
    </NativeProtectionScanProvider>
  );
}

function renderHome(api: SentinelApi): void {
  render(
    <ApiProvider api={api}>
      <HomeHarness api={api} />
    </ApiProvider>,
  );
}

function stoppedReport() {
  return makeSensorStatusReport({
    linkState: "stopped",
    sensor: null,
    error: "sensor status endpoint does not exist",
  });
}

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("HomePage", () => {
  it("shows only the static not-enabled state", async () => {
    const api = createApi({
      getSensorStatus: vi.fn().mockResolvedValue(stoppedReport()),
    });
    renderHome(api);

    expect(
      await screen.findByRole("heading", { level: 1, name: "让电脑更安全。" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "启用" })).toBeDisabled();
    expect(
      screen.getByText(
        "当前版本暂不支持在应用内启用实时保护。管理员可以在应用外手动启动传感器。",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "运行" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "系统防护" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "检查与设置" })).not.toBeInTheDocument();
    expect(screen.queryByText("已接收事件")).not.toBeInTheDocument();
    expect(screen.queryByText("启动与能力说明")).not.toBeInTheDocument();
  });

  it("does not provide an actionable sensor start path", async () => {
    const api = createApi({
      getSensorStatus: vi.fn().mockResolvedValue(stoppedReport()),
    });
    renderHome(api);

    const button = await screen.findByRole("button", { name: "启用" });
    expect(button).toBeDisabled();
    fireEvent.click(button);

    expect(
      screen.getByRole("heading", { level: 1, name: "让电脑更安全。" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "当前版本暂不支持在应用内启用实时保护。管理员可以在应用外手动启动传感器。",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText("正在启动…")).not.toBeInTheDocument();
    expect(screen.queryByText("请允许管理员权限。")).not.toBeInTheDocument();
  });

  it("shows only the core status by default when monitoring is healthy", async () => {
    renderHome(createApi());

    expect(
      await screen.findByRole("heading", { level: 1, name: "实时监测正在运行。" }),
    ).toBeInTheDocument();
    expect(screen.getByText("整体保护：未知")).toBeInTheDocument();
    expect(screen.queryByText("保护正常")).not.toBeInTheDocument();

    const runtimeTrigger = screen.getByRole("button", { name: "运行" });
    expect(runtimeTrigger).toHaveAttribute("aria-expanded", "false");
    const runtimeRegion = document.getElementById(
      runtimeTrigger.getAttribute("aria-controls") ?? "",
    );
    expect(runtimeRegion).toHaveAttribute("inert");
  });

  it("reveals real counters only after opening the runtime disclosure", async () => {
    renderHome(createApi());

    fireEvent.click(await screen.findByRole("button", { name: "运行" }));

    const metrics = screen.getByLabelText("传感器采集统计");
    expect(within(metrics).getByLabelText("已接收事件：21")).toBeInTheDocument();
    expect(within(metrics).getByLabelText("已处理事件：20")).toBeInTheDocument();
    expect(within(metrics).getByLabelText("丢失事件：0")).toBeInTheDocument();
    expect(within(metrics).getByLabelText("运行时间：1 小时 2 分")).toBeInTheDocument();
    expect(screen.getByText(/最近事件：/)).toBeInTheDocument();
    expect(screen.queryByText("其他监控组件")).not.toBeInTheDocument();
  });

  it("keeps active and unhealthy independent and shows the reason without expanding", async () => {
    renderHome(
      createApi({
        getSensorStatus: vi.fn().mockResolvedValue(
          makeSensorStatusReport({
            linkState: "degraded",
            sensor: makeSensorStatus({
              health_ok: false,
              last_error: "sensor health not proven",
            }),
          }),
        ),
      }),
    );

    expect(
      await screen.findByRole("heading", { level: 1, name: "监测需要关注。" }),
    ).toBeInTheDocument();
    const attention = screen.getByRole("status");
    expect(within(attention).getByText(/传感器尚未证明健康/)).toBeInTheDocument();
    expect(screen.queryByText("监测正常")).not.toBeInTheDocument();
  });

  it("returns to the minimal not-enabled page when the sensor stops", async () => {
    vi.useFakeTimers();
    const getSensorStatus = vi
      .fn()
      .mockResolvedValueOnce(makeSensorStatusReport())
      .mockResolvedValueOnce(stoppedReport());

    renderHome(createApi({ getSensorStatus }));
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(
      screen.getByRole("heading", { level: 1, name: "实时监测正在运行。" }),
    ).toBeInTheDocument();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(SENSOR_STATUS_REFRESH_MS);
    });

    expect(
      screen.getByRole("heading", { level: 1, name: "让电脑更安全。" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "启用" })).toBeDisabled();
    expect(
      screen.getByText(
        "当前版本暂不支持在应用内启用实时保护。管理员可以在应用外手动启动传感器。",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "运行" })).not.toBeInTheDocument();
    expect(screen.queryByText("已接收事件")).not.toBeInTheDocument();
  });

  it("shows a query failure as disconnected and recovers on retry", async () => {
    const getSensorStatus = vi
      .fn()
      .mockRejectedValueOnce(
        new SentinelApiError(
          "backendUnavailable",
          "无法连接 SentinelLite 本机服务。",
          false,
        ),
      )
      .mockResolvedValueOnce(makeSensorStatusReport());

    renderHome(createApi({ getSensorStatus }));

    expect(
      await screen.findByRole("heading", { level: 1, name: "无法确认保护状态。" }),
    ).toBeInTheDocument();
    const alert = screen.getByRole("alert");
    expect(within(alert).getByText("无法连接 SentinelLite 本机服务。")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "启用" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "重新查询" }));

    expect(
      await screen.findByRole("heading", { level: 1, name: "实时监测正在运行。" }),
    ).toBeInTheDocument();
  });

  it("does not claim a normal state before the first query returns", () => {
    renderHome(
      createApi({
        getSensorStatus: vi.fn().mockReturnValue(new Promise(() => undefined)),
      }),
    );

    expect(
      screen.getByRole("heading", { level: 1, name: "正在确认保护状态。" }),
    ).toBeInTheDocument();
    expect(screen.queryByText(/整体保护/)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "启用" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "运行" })).not.toBeInTheDocument();
  });

  it("keeps the live status visible and exposes a readable overview failure", async () => {
    renderHome(
      createApi({
        getProtectionOverview: vi.fn().mockRejectedValue(
          new SentinelApiError(
            "backendUnavailable",
            "无法连接 SentinelLite 本机服务。请从桌面应用启动，或重新打开应用后重试。",
            false,
          ),
        ),
      }),
    );

    expect(
      await screen.findByRole("heading", { level: 1, name: "实时监测正在运行。" }),
    ).toBeInTheDocument();
    expect(await screen.findByText("读取失败")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "检查与设置" }));
    expect(
      screen.getByText("无法连接 SentinelLite 本机服务。请从桌面应用启动，或重新打开应用后重试。"),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重新读取" })).toBeInTheDocument();
  });

  it("keeps read-only checks accessible behind the checks disclosure", async () => {
    renderHome(createApi());

    fireEvent.click(await screen.findByRole("button", { name: "检查与设置" }));

    expect(screen.getByText("软件与组件盘点")).toBeInTheDocument();
    expect(screen.getByText("启动项检查")).toBeInTheDocument();
    expect(screen.getByText("进程与 CPU 亲和性")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "打开软件体检" })).toBeInTheDocument();
    expect(screen.queryByText(/深度防御模式/)).not.toBeInTheDocument();
  });

  it("keeps the native protection snapshot available without changing the overall state", async () => {
    renderHome(
      createApi({
        getNativeProtectionStatus: vi.fn().mockResolvedValue(makeNativeProtectionStatus()),
      }),
    );

    fireEvent.click(await screen.findByRole("button", { name: "系统防护" }));
    fireEvent.click(screen.getByRole("button", { name: "读取原生防护状态" }));

    expect(await screen.findByText("已读取全部字段")).toBeInTheDocument();
    expect(screen.getByText("已读取的开关均为启用；这不等于完整保护。")).toBeInTheDocument();
    expect(screen.queryByText("只读观测，不修改原生防护。")).not.toBeInTheDocument();
    expect(screen.getByText("整体保护：未知")).toBeInTheDocument();
    expect(screen.queryByText("保护正常")).not.toBeInTheDocument();
  });

  it("surfaces observed native protection switches that are not enabled", async () => {
    const base = makeNativeProtectionStatus();
    const status = makeNativeProtectionStatus({
      defender: {
        ...base.defender,
        realTimeProtectionEnabled: false,
      },
    });
    renderHome(
      createApi({
        getNativeProtectionStatus: vi.fn().mockResolvedValue(status),
      }),
    );

    fireEvent.click(await screen.findByRole("button", { name: "系统防护" }));
    fireEvent.click(screen.getByRole("button", { name: "读取原生防护状态" }));

    expect((await screen.findAllByText("已读取全部字段")).length).toBeGreaterThan(0);
    expect(screen.getByText("1 项未启用")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Defender 实时保护");
    expect(screen.queryByText("保护正常")).not.toBeInTheDocument();
  });
});
