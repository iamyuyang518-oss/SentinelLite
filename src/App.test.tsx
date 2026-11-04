import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { App } from "./App";
import { ApiProvider } from "./components/ApiProvider";
import type { SentinelApi } from "./lib/api";
import { THEME_STORAGE_KEY } from "./lib/theme";
import {
  makeNativeProtectionStatus,
  makeOverview,
  makeSensorStatusReport,
} from "./test/fixtures";

function createApi(): SentinelApi {
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
  };
}

describe("App shell", () => {
  it("navigates between the implemented pages", async () => {
    render(
      <ApiProvider api={createApi()}>
        <App />
      </ApiProvider>,
    );

    expect(
      await screen.findByRole("heading", { level: 1, name: "实时监测正在运行。" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "安全首页" })).toHaveAttribute(
      "aria-current",
      "page",
    );

    fireEvent.click(screen.getByRole("button", { name: "软件体检" }));

    expect(screen.getByRole("heading", { level: 1, name: "软件体检" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "软件" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "软件体检" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(
      screen.queryByRole("heading", { level: 1, name: "实时监测正在运行。" }),
    ).toBeNull();
  });

  it("switches and persists the theme from the top bar", async () => {
    render(
      <ApiProvider api={createApi()}>
        <App />
      </ApiProvider>,
    );

    fireEvent.click(
      await screen.findByRole("button", { name: "切换到深色模式" }),
    );

    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("dark");
    expect(
      screen.getByRole("button", { name: "切换到浅色模式" }),
    ).toBeInTheDocument();
  });

  it("reuses one native protection snapshot across the home and posture views", async () => {
    const getNativeProtectionStatus = vi.fn().mockResolvedValue(makeNativeProtectionStatus());
    const api = createApi();
    api.getNativeProtectionStatus = getNativeProtectionStatus;

    render(
      <ApiProvider api={api}>
        <App />
      </ApiProvider>,
    );

    await screen.findByRole("heading", { level: 1, name: "实时监测正在运行。" });
    fireEvent.click(screen.getByRole("button", { name: "系统防护" }));
    fireEvent.click(screen.getByRole("button", { name: "读取原生防护状态" }));
    expect((await screen.findAllByText("已读取全部字段")).length).toBeGreaterThan(0);

    fireEvent.click(screen.getByRole("button", { name: "软件体检" }));

    expect(await screen.findByText(/6 个防护开关已启用/)).toBeInTheDocument();
    expect(screen.getAllByText("已读取全部字段").length).toBeGreaterThan(0);
    expect(getNativeProtectionStatus).toHaveBeenCalledTimes(1);
    expect(api.scanAssetAssociations).not.toHaveBeenCalled();
  });
});
