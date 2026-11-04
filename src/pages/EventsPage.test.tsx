import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ApiProvider } from "../components/ApiProvider";
import type { SentinelApi } from "../lib/api";
import {
  makeEventBatch,
  makeEventDetail,
  makePairingRevision,
  makeProcessEvent,
  makeSensorEventsReport,
  makeSensorStatus,
  makeSensorStatusReport,
  makeSessionEnd,
  makeSessionEndStats,
  makeSessionRecord,
  makeStorageReport,
  makeStoredEvent,
} from "../test/fixtures";
import type {
  ProcessEventWire,
  SensorEventsReport,
  SessionAnalysisWire,
} from "../types";
import { EventsPage } from "./EventsPage";

const START_ID = "session-test:0000000000000001";
const STOP_ID = "session-test:0000000000000002";

function startEvent(): ProcessEventWire {
  return makeProcessEvent({
    event_id: START_ID,
    sequence: 1,
    event_type: "started",
    pairing: { state: "waiting" },
  });
}

function stopEvent(): ProcessEventWire {
  return makeProcessEvent({
    event_id: STOP_ID,
    sequence: 2,
    event_type: "stopped",
    etw: { ...makeProcessEvent().etw, event_id: 2 },
    pairing: {
      state: "unpaired",
      reason: { reason: "stop_without_observed_start" },
    },
  });
}

function liveReport(): SensorEventsReport {
  return makeSensorEventsReport(
    makeEventBatch([
      { cursor: 1, record: { record: "process_event", ...startEvent() } },
      { cursor: 2, record: { record: "process_event", ...stopEvent() } },
      { cursor: 3, record: { record: "pairing_revision", ...makePairingRevision() } },
    ]),
  );
}

function createApi(overrides: Partial<SentinelApi> = {}): SentinelApi {
  const storedStop = makeStoredEvent({
    record: stopEvent(),
    pairing_state: "paired",
    pairing_order: "out_of_order",
    paired_event_id: START_ID,
  });
  const storedStart = makeStoredEvent({ record: startEvent() });
  const detail = makeEventDetail({
    event: storedStop,
    paired_event: storedStart,
    revisions: [makePairingRevision()],
  });
  return {
    getProtectionOverview: vi.fn(),
    getNativeProtectionStatus: vi.fn(),
    getSensorStatus: vi.fn().mockResolvedValue(makeSensorStatusReport()),
    getSensorEvents: vi.fn().mockResolvedValue(liveReport()),
    getStorageStatus: vi.fn().mockResolvedValue(makeStorageReport()),
    queryStoredEvents: vi.fn().mockResolvedValue({
      total: 1,
      limit: 100,
      offset: 0,
      events: [storedStop],
    }),
    getStoredEvent: vi.fn().mockImplementation((eventId: string) =>
      Promise.resolve(eventId === START_ID ? makeEventDetail({ event: storedStart }) : detail),
    ),
    listSensorSessions: vi.fn().mockResolvedValue([makeSessionRecord()]),
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

function renderPage(api: SentinelApi): void {
  render(
    <ApiProvider api={api}>
      <EventsPage />
    </ApiProvider>,
  );
}

afterEach(() => {
  cleanup();
});

describe("EventsPage", () => {
  it("shows real live events, applies pairing corrections, and queries stored history", async () => {
    const api = createApi();
    renderPage(api);

    expect(await screen.findByText("已连接")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "实时事件" }));

    const startRow = await screen.findByRole("button", {
      name: "进程启动 notepad.exe PID 4242",
    });
    expect(startRow).toBeInTheDocument();
    const stopRow = screen.getByRole("button", {
      name: "进程退出 notepad.exe PID 4242",
    });
    expect(within(stopRow).queryByText("已配对（乱序修正）")).not.toBeInTheDocument();

    fireEvent.click(stopRow);
    const detail = (
      await screen.findByText(/已收到并应用 1 条配对修正/)
    ).closest(".event-detail");
    expect(detail).not.toBeNull();
    expect(
      within(detail as HTMLElement).getByText("C:\\Windows\\System32\\notepad.exe"),
    ).toBeInTheDocument();

    expect(await screen.findByText("1 条")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "历史事件" }));
    fireEvent.click(screen.getByRole("button", { name: `查看事件 ${STOP_ID}` }));

    expect(
      await screen.findByText("start arrived after an orphan stop"),
    ).toBeInTheDocument();
    expect(api.getStoredEvent).toHaveBeenCalledWith(STOP_ID);
  });

  it("shows cursor expiry as a data gap, not an empty healthy list", async () => {
    const api = createApi({
      getSensorEvents: vi.fn().mockResolvedValue(
        makeSensorEventsReport({
          ...makeEventBatch([]),
          cursor_expired: true,
          missed_records: 12,
          next_cursor: 40,
          latest_cursor: 40,
        }),
      ),
    });
    renderPage(api);

    expect(await screen.findByText(/条数据提醒/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "实时事件" }));
    expect(
      await screen.findByRole("heading", { name: "事件投递窗口已覆盖" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/至少缺少 12 条记录/)).toBeInTheDocument();
  });

  it("does not present a disconnected sensor as an empty normal stream", async () => {
    const api = createApi({
      getSensorEvents: vi.fn().mockResolvedValue({
        linkState: "stopped",
        batch: null,
        error: "sensor event endpoint does not exist",
      }),
      getSensorStatus: vi.fn().mockResolvedValue({
        linkState: "stopped",
        sensor: null,
        error: "sensor status endpoint does not exist",
      }),
      getStorageStatus: vi.fn().mockResolvedValue(
        makeStorageReport({
          state: "degraded",
          ingestion: {
            ...makeStorageReport().ingestion,
            state: "degraded",
            source_available: false,
            last_error: "sensor event endpoint does not exist",
          },
        }),
      ),
    });
    renderPage(api);

    expect(await screen.findByText("未连接")).toBeInTheDocument();
    expect(screen.queryByText("已连接")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "实时事件" }));
    expect(
      await screen.findByRole("heading", { name: "传感器未连接" }),
    ).toBeInTheDocument();
    expect(screen.getByText("暂无事件。")).toBeInTheDocument();
    expect(screen.getByText("本地事件持久化降级")).toBeInTheDocument();
    expect(screen.queryByText("0 条")).not.toBeInTheDocument();
  });

  it("marks a closed session as ended and reconnects to a new session without mixing events", async () => {
    const oldEvent = makeProcessEvent({
      event_id: "session-test:0000000000000009",
      sequence: 9,
      image_name: "C:\\Tools\\old-live.exe",
      pairing: { state: "waiting" },
    });
    const closedBatch = {
      ...makeEventBatch([
        { cursor: 1, record: { record: "process_event", ...oldEvent } },
        { cursor: 2, record: { record: "session_end", ...makeSessionEnd() } },
      ]),
      session_closed: true,
    };
    const newEvent = makeProcessEvent({
      event_id: "session-new:0000000000000001",
      session_id: "session-new",
      sequence: 1,
      image_name: "C:\\Tools\\new-live.exe",
      pairing: { state: "waiting" },
    });
    const newBatch = {
      ...makeEventBatch([
        { cursor: 1, record: { record: "process_event", ...newEvent } },
      ]),
      session_id: "session-new",
    };
    const getSensorEvents = vi
      .fn()
      .mockResolvedValueOnce(makeSensorEventsReport(closedBatch))
      .mockResolvedValueOnce(makeSensorEventsReport(newBatch));
    const api = createApi({ getSensorEvents });
    renderPage(api);

    expect(await screen.findByText("会话已结束")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "实时事件" }));
    const oldRow = await screen.findByRole("button", {
      name: "进程启动 old-live.exe PID 4242",
    });
    fireEvent.click(oldRow);
    expect(screen.getByText("C:\\Tools\\old-live.exe")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "重新连接" }));

    expect(
      await screen.findByRole("button", {
        name: "进程启动 new-live.exe PID 4242",
      }),
    ).toBeInTheDocument();
    expect(screen.queryByText("C:\\Tools\\old-live.exe")).not.toBeInTheDocument();
    expect(screen.getByText("已连接")).toBeInTheDocument();
  });

  it("shows final ETW and pipeline statistics from a stored SessionEnd", async () => {
    const api = createApi({
      getStoredEvent: vi.fn().mockResolvedValue(
        makeEventDetail({
          event: makeStoredEvent({ record: stopEvent() }),
          session_end: makeSessionEnd({
            final_stats: makeSessionEndStats({
              events_lost: 3,
              pipeline_backlog: 2,
              pipeline_shutdown_timed_out: true,
              pipeline_backlog_at_shutdown_timeout: 2,
            }),
          }),
        }),
      ),
    });
    renderPage(api);

    expect(await screen.findByText("1 条")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "历史事件" }));
    fireEvent.click(screen.getByRole("button", { name: `查看事件 ${STOP_ID}` }));
    const detail = (await screen.findByText("最终统计")).closest(".event-detail");
    expect(detail).not.toBeNull();

    expect(within(detail as HTMLElement).getByText("最终统计")).toBeInTheDocument();
    expect(within(detail as HTMLElement).getByText("ETW 事件丢失")).toBeInTheDocument();
    expect(within(detail as HTMLElement).getByText("3")).toBeInTheDocument();
    expect(within(detail as HTMLElement).getByText(/超时积压 2/)).toBeInTheDocument();
  });

  it("shows explainable behavior results and traceable evidence", async () => {
    const analysis: SessionAnalysisWire = {
      session_id: "session-test",
      generated_at: "2026-10-29T02:31:00.000Z",
      rule_catalog_version: "1",
      overall: "suspicious",
      summary: "观察到需要人工核实的模式。可疑不等于恶意。",
      input: {
        total_events: 30,
        start_events: 30,
        stop_events: 0,
        paired_events: 0,
        unpaired_events: 0,
        indeterminate_events: 0,
        missing_time_events: 0,
        missing_image_events: 0,
        invalid_records: 0,
        ingestion_gaps: 0,
        truncated: false,
        session_closed: true,
        session_end_reason: "completed",
        etw_events_lost: 0,
        realtime_buffers_lost: 0,
        pipeline_dropped: 0,
        pipeline_backlog: 0,
        pipeline_shutdown_timed_out: false,
        sensor_health_known: true,
        sensor_healthy: true,
      },
      results: [
        {
          rule_id: "PROC-BURST-001",
          rule_version: "1",
          status: "suspicious",
          severity: "medium",
          confidence: "medium",
          title: "短时间内大量进程启动",
          explanation:
            "在约 10 秒窗口内观察到 30 次进程启动。安装器或构建脚本可能产生同样模式。",
          false_positive_notes: ["软件安装、更新或构建脚本可能产生同样的模式。"],
          thresholds: [{ name: "event_count", value: "30" }],
          evidence: [
            {
              statement: "窗口内共有 30 次进程启动。",
              event_ids: [START_ID, STOP_ID],
              value: "30",
            },
          ],
          affected_event_count: 30,
          window_start: "2026-10-29T02:30:00.000Z",
          window_end: "2026-10-29T02:30:03.000Z",
        },
      ],
    };
    const api = createApi({
      analyzeSession: vi.fn().mockResolvedValue(analysis),
    });
    renderPage(api);

    fireEvent.click(await screen.findByRole("button", { name: "更多" }));
    fireEvent.click(screen.getByRole("button", { name: "分析" }));

    expect(await screen.findByText("短时间内大量进程启动")).toBeInTheDocument();
    expect(screen.getAllByText("可疑模式").length).toBeGreaterThan(0);
    expect(screen.getByText("PROC-BURST-001 · v1")).toBeInTheDocument();
    expect(screen.getByText(/可能的误报来源/)).toBeInTheDocument();
    expect(screen.getByText(/event_count=30/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: START_ID }));
    expect(api.getStoredEvent).toHaveBeenCalledWith(START_ID);
    expect(await screen.findByText("配对修正历史")).toBeInTheDocument();
  });

  it("does not render insufficient analysis as a clean result", async () => {
    const analysis: SessionAnalysisWire = {
      session_id: "session-test",
      generated_at: null,
      rule_catalog_version: "1",
      overall: "data_insufficient",
      summary: "输入数据不足或存在已知缺口，无法形成正常或异常结论。",
      input: {
        total_events: 2,
        start_events: 1,
        stop_events: 1,
        paired_events: 0,
        unpaired_events: 1,
        indeterminate_events: 0,
        missing_time_events: 0,
        missing_image_events: 0,
        invalid_records: 0,
        ingestion_gaps: 1,
        truncated: false,
        session_closed: false,
        session_end_reason: null,
        etw_events_lost: 0,
        realtime_buffers_lost: 0,
        pipeline_dropped: 0,
        pipeline_backlog: 0,
        pipeline_shutdown_timed_out: false,
        sensor_health_known: true,
        sensor_healthy: true,
      },
      results: [
        {
          rule_id: "DATA-SMALL-001",
          rule_version: "1",
          status: "data_insufficient",
          severity: "info",
          confidence: "high",
          title: "事件样本不足",
          explanation: "本会话仅有 2 条标准化进程事件，不能据此判断行为正常或异常。",
          false_positive_notes: [],
          thresholds: [{ name: "min_events", value: "10" }],
          evidence: [],
          affected_event_count: 0,
          window_start: null,
          window_end: null,
        },
      ],
    };
    const api = createApi({ analyzeSession: vi.fn().mockResolvedValue(analysis) });
    renderPage(api);

    fireEvent.click(await screen.findByRole("button", { name: "更多" }));
    fireEvent.click(screen.getByRole("button", { name: "分析" }));

    expect(await screen.findByText("事件样本不足")).toBeInTheDocument();
    expect(screen.getAllByText("数据不足").length).toBeGreaterThan(0);
    expect(screen.queryByText("未发现规则覆盖的异常")).not.toBeInTheDocument();
    expect(screen.queryByText("否（不完整）")).not.toBeInTheDocument();
  });

  it("reports an unknown ETW loss count instead of showing zero", async () => {
    const api = createApi({
      getSensorStatus: vi.fn().mockResolvedValue(
        makeSensorStatusReport({
          sensor: makeSensorStatus({
            events_lost: null,
            realtime_buffers_lost: null,
            health_ok: false,
          }),
        }),
      ),
    });
    renderPage(api);

    expect(await screen.findByText(/条数据提醒/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "实时事件" }));
    expect(
      await screen.findByRole("heading", { name: "ETW 丢失统计未知" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/不能把未知当作零丢失/)).toBeInTheDocument();
  });
});
