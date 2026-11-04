import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ApiProvider } from "./ApiProvider";
import { NativeProtectionCard } from "./NativeProtectionCard";
import { SentinelApiError, type SentinelApi } from "../lib/api";
import { NativeProtectionScanProvider } from "../lib/nativeProtectionScan";
import { useScanAction } from "../lib/useScanAction";
import { makeNativeProtectionStatus } from "../test/fixtures";

function createApi(overrides: Partial<SentinelApi> = {}): SentinelApi {
  return {
    getProtectionOverview: vi.fn(),
    getNativeProtectionStatus: vi.fn().mockResolvedValue(makeNativeProtectionStatus()),
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

function CardHarness({ api }: { api: SentinelApi }) {
  const controller = useScanAction(api.getNativeProtectionStatus);
  return (
    <NativeProtectionScanProvider controller={controller}>
      <NativeProtectionCard />
    </NativeProtectionScanProvider>
  );
}

function renderCard(api: SentinelApi): void {
  render(
    <ApiProvider api={api}>
      <CardHarness api={api} />
    </ApiProvider>,
  );
}

function sectionFor(title: string): HTMLElement {
  const heading = screen.getByRole("heading", { level: 3, name: title });
  const section = heading.closest("section");
  if (section === null) {
    throw new Error(`section for ${title} not found`);
  }
  return section;
}

afterEach(cleanup);

describe("NativeProtectionCard", () => {
  it("stays unqueried and unknown until the user asks for a read-only check", () => {
    const api = createApi();
    renderCard(api);

    expect(screen.getByText("尚未检查。读取结果只代表当前观测到的原生防护开关，不改变 SentinelLite 的整体保护状态。")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "读取原生防护状态" })).toBeInTheDocument();
    expect(api.getNativeProtectionStatus).not.toHaveBeenCalled();
    expect(screen.queryByText("已启用")).not.toBeInTheDocument();
    expect(screen.queryByText("保护正常")).not.toBeInTheDocument();
  });

  it("shows observed Defender and firewall values without claiming protection", async () => {
    renderCard(createApi());

    fireEvent.click(screen.getByRole("button", { name: "读取原生防护状态" }));

    expect(await screen.findByText("已读取全部字段")).toBeInTheDocument();
    expect(screen.getByText("只读观测，不修改原生防护。")).toBeInTheDocument();
    expect(screen.queryByText("保护正常")).not.toBeInTheDocument();

    const defender = sectionFor("Windows Defender");
    expect(within(defender).getAllByText("已启用")).toHaveLength(3);
    expect(within(defender).getByText("1.2.3.4")).toBeInTheDocument();
    expect(within(defender).getByText("0 天")).toBeInTheDocument();

    const firewall = sectionFor("Windows 防火墙");
    expect(within(firewall).getAllByText("已启用")).toHaveLength(3);
    expect(within(firewall).getByText("域网络")).toBeInTheDocument();
    expect(within(firewall).getByText("专用网络")).toBeInTheDocument();
    expect(within(firewall).getByText("公用网络")).toBeInTheDocument();
  });

  it("shows disabled switches as observed facts and keeps the overall conclusion unknown", async () => {
    const status = makeNativeProtectionStatus({
      defender: {
        ...makeNativeProtectionStatus().defender,
        realTimeProtectionEnabled: false,
      },
      firewall: {
        ...makeNativeProtectionStatus().firewall,
        profiles: [
          { profile: "domain", enabled: true },
          { profile: "private", enabled: true },
          { profile: "public", enabled: false },
        ],
      },
    });
    renderCard(createApi({ getNativeProtectionStatus: vi.fn().mockResolvedValue(status) }));

    fireEvent.click(screen.getByRole("button", { name: "读取原生防护状态" }));

    expect(await screen.findByText(/观测到未启用/)).toHaveTextContent(
      "Defender 实时保护、防火墙 · 公用网络",
    );
    expect(screen.getAllByText("未启用")).toHaveLength(2);
    expect(screen.queryByText("保护正常")).not.toBeInTheDocument();
  });

  it("treats a disabled anti-malware service as a disabled switch, not all enabled", async () => {
    const base = makeNativeProtectionStatus();
    const status = makeNativeProtectionStatus({
      defender: {
        ...base.defender,
        amServiceEnabled: false,
      },
    });
    renderCard(createApi({ getNativeProtectionStatus: vi.fn().mockResolvedValue(status) }));

    fireEvent.click(screen.getByRole("button", { name: "读取原生防护状态" }));

    expect(await screen.findByText(/观测到未启用/)).toHaveTextContent(
      "Defender 反恶意软件服务",
    );
    expect(screen.getByText("未启用")).toBeInTheDocument();
    expect(screen.queryByText(/已读取到的开关均为启用状态/)).not.toBeInTheDocument();
    expect(screen.queryByText("保护正常")).not.toBeInTheDocument();
  });

  it("does not claim all switches are enabled when a field remains unknown", async () => {
    const base = makeNativeProtectionStatus();
    const status = makeNativeProtectionStatus({
      defender: {
        ...base.defender,
        amServiceEnabled: null,
      },
    });
    renderCard(createApi({ getNativeProtectionStatus: vi.fn().mockResolvedValue(status) }));

    fireEvent.click(screen.getByRole("button", { name: "读取原生防护状态" }));

    expect(await screen.findByText("未知")).toBeInTheDocument();
    expect(screen.queryByText(/已读取到的开关均为启用状态/)).not.toBeInTheDocument();
  });

  it("keeps fields unknown and lists issues when the provider data is partial", async () => {
    const status = makeNativeProtectionStatus({
      observation: "partial",
      defender: {
        ...makeNativeProtectionStatus().defender,
        state: "partial",
        amServiceEnabled: null,
        issues: [
          {
            code: "DEFENDER-FIELD-MISSING",
            message: "Defender 状态缺少 AMServiceEnabled 字段；该项保持未知。",
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
            message: "没有权限读取 Windows 防火墙状态。当前状态未知。",
          },
        ],
      },
    });
    renderCard(createApi({ getNativeProtectionStatus: vi.fn().mockResolvedValue(status) }));

    fireEvent.click(screen.getByRole("button", { name: "读取原生防护状态" }));

    expect(await screen.findByText("部分字段未知")).toBeInTheDocument();
    expect(screen.getAllByText("未知").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText("DEFENDER-FIELD-MISSING")).toBeInTheDocument();
    expect(screen.getByText("FIREWALL-ACCESS-DENIED")).toBeInTheDocument();
    expect(screen.queryByText("保护正常")).not.toBeInTheDocument();
  });

  it("does not retain an old result when a refresh fails", async () => {
    const getNativeProtectionStatus = vi
      .fn()
      .mockResolvedValueOnce(makeNativeProtectionStatus())
      .mockRejectedValueOnce(
        new SentinelApiError(
          "nativeProtectionUnavailable",
          "无法读取原生防护状态。",
          true,
        ),
      );
    renderCard(createApi({ getNativeProtectionStatus }));

    fireEvent.click(screen.getByRole("button", { name: "读取原生防护状态" }));
    expect(await screen.findByText("已读取全部字段")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "重新读取" }));

    expect(await screen.findByRole("heading", { level: 3, name: "无法读取原生防护状态" })).toBeInTheDocument();
    expect(screen.getByText("无法读取原生防护状态。")).toBeInTheDocument();
    expect(screen.queryByText("已读取全部字段")).not.toBeInTheDocument();
    expect(screen.queryByText("已启用")).not.toBeInTheDocument();
    expect(screen.getByText(/未保留上一次结果/)).toBeInTheDocument();
  });
});
