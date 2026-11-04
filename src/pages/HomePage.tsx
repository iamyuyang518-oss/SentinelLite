import { useEffect, useState } from "react";

import { useSentinelApi } from "../components/ApiProvider";
import { ExpandableSection } from "../components/ExpandableSection";
import { NativeProtectionInline } from "../components/NativeProtectionCard";
import { getUserFacingApiError } from "../lib/api";
import { deriveHomeStatus } from "../lib/homeStatus";
import { disabledProtectionSwitches } from "../lib/nativeProtection";
import { useNativeProtectionScan } from "../lib/nativeProtectionScan";
import { formatCheckedAt } from "../lib/presentation";
import {
  deriveSensorView,
  type SensorMetricValue,
  type SensorView,
} from "../lib/sensorStatus";
import { useSensorStatus } from "../lib/useSensorStatus";
import type { ProtectionOverview } from "../types";

interface HomePageProps {
  onOpenCleanup: () => void;
}

type OverviewState =
  | { phase: "loading" }
  | { phase: "error"; message: string }
  | { phase: "ready"; overview: ProtectionOverview };

function NotEnabledHome({
  title,
  description,
}: {
  title: string;
  description: string;
}) {
  return (
    <div className="page home home--empty">
      <div className="home-empty">
        <h1>{title}</h1>
        <div className="home-empty__actions">
          <button
            type="button"
            className="home-enable"
            disabled
            aria-describedby="home-enable-note"
          >
            启用
          </button>
          <span className="home-empty__note" id="home-enable-note">
            {description}
          </span>
        </div>
      </div>
    </div>
  );
}

function SensorMetric({ label, metric }: { label: string; metric: SensorMetricValue }) {
  return (
    <div className="home-metric" aria-label={`${label}：${metric.value}`}>
      <dt>{label}</dt>
      <dd className={metric.unknown ? "home-metric__value is-unknown" : "home-metric__value"}>
        {metric.value}
      </dd>
    </div>
  );
}

function AttentionBlock({
  title,
  message,
  tone,
  alert,
  canRetry,
  onRetry,
}: {
  title: string;
  message: string;
  tone: string;
  alert: boolean;
  canRetry: boolean;
  onRetry: () => void;
}) {
  return (
    <section
      className={`home-attention home-attention--${tone}`}
      role={alert ? "alert" : "status"}
    >
      <h2>{title}</h2>
      <p>{message}</p>
      {canRetry ? (
        <button type="button" className="button button--quiet" onClick={onRetry}>
          重新查询
        </button>
      ) : null}
    </section>
  );
}

function RuntimeDetails({ sensorView }: { sensorView: SensorView }) {
  return (
    <div className="home-runtime">
      {sensorView.state === "failed" ? (
        <p className="home-note home-note--warning">
          以下计数为上次响应值，不代表正在采集。
        </p>
      ) : null}

      <dl className="home-metrics" aria-label="传感器采集统计">
        <SensorMetric label="已接收事件" metric={sensorView.metrics.received} />
        <SensorMetric label="已处理事件" metric={sensorView.metrics.processed} />
        <SensorMetric label="丢失事件" metric={sensorView.metrics.lost} />
        <SensorMetric label="运行时间" metric={sensorView.metrics.uptime} />
      </dl>

      {sensorView.diagnostic ? (
        <p className="home-diagnostic">{sensorView.diagnostic}</p>
      ) : null}

      <p className="home-note">最近事件：{sensorView.lastEventLabel}</p>
    </div>
  );
}

function ChecksContent({
  overview,
  overviewError,
  onRetry,
  onOpenCleanup,
}: {
  overview: ProtectionOverview | null;
  overviewError: string | null;
  onRetry: () => void;
  onOpenCleanup: () => void;
}) {
  if (overviewError !== null) {
    return (
      <div className="home-check-state">
        <p>{overviewError}</p>
        <button type="button" className="button button--quiet" onClick={onRetry}>
          重新读取
        </button>
      </div>
    );
  }

  if (overview === null) {
    return (
      <p className="home-note" role="status">
        正在读取…
      </p>
    );
  }

  return (
    <div className="home-checks">
      <ul className="home-check-list">
        {overview.readOnlyChecks.map((check) => (
          <li key={check.id}>{check.name}</li>
        ))}
      </ul>
      <div className="home-inline-actions">
        <button type="button" className="button button--quiet" onClick={onOpenCleanup}>
          打开软件体检
        </button>
      </div>
    </div>
  );
}

export function HomePage({ onOpenCleanup }: HomePageProps) {
  const api = useSentinelApi();
  const [overviewReloadKey, setOverviewReloadKey] = useState(0);
  const [overviewState, setOverviewState] = useState<OverviewState>({ phase: "loading" });
  const sensorQuery = useSensorStatus(api);
  const protection = useNativeProtectionScan();

  const sensorReport =
    sensorQuery.state.phase === "ready" ? sensorQuery.state.report : null;
  const sensorError =
    sensorQuery.state.phase === "error" ? sensorQuery.state.error : null;
  const sensorView = deriveSensorView(sensorReport, sensorError);
  const status = deriveHomeStatus(
    sensorReport,
    sensorError,
    sensorQuery.state.phase,
  );

  const overview = overviewState.phase === "ready" ? overviewState.overview : null;
  const overviewError = overviewState.phase === "error" ? overviewState.message : null;
  const overallLabel =
    overview?.overallState === "riskDetected"
      ? "发现风险"
      : overview?.overallState === "degraded"
        ? "保护降级"
        : "未知";

  const protectionStatus =
    protection.state.phase === "ready" ? protection.state.data : null;
  const disabledNativeSwitches = disabledProtectionSwitches(protectionStatus).map(
    (entry) => entry.label,
  );
  const protectionCheckedAt = protectionStatus
    ? formatCheckedAt(protectionStatus.checkedAt)
    : null;
  const protectionSummary =
    protection.state.phase === "ready"
      ? disabledNativeSwitches.length > 0
        ? `${disabledNativeSwitches.length} 项未启用`
        : protection.state.data.observation === "complete"
          ? "已读取"
          : protection.state.data.observation === "partial"
            ? "部分未知"
            : "无法读取"
      : protection.state.phase === "loading"
        ? "读取中"
        : protection.state.phase === "error"
          ? "读取失败"
          : "按需读取";

  const showRuntimeDetails =
    (status.phase === "active" ||
      status.phase === "attention" ||
      status.phase === "failed") &&
    sensorView.hasMetrics;
  const receivedSummary = sensorView.metrics.received.unknown
    ? "未知"
    : `${sensorView.metrics.received.value} 条`;

  useEffect(() => {
    let active = true;

    api.getProtectionOverview().then(
      (nextOverview) => {
        if (active) {
          setOverviewState({ phase: "ready", overview: nextOverview });
        }
      },
      (error: unknown) => {
        if (active) {
          setOverviewState({
            phase: "error",
            message: getUserFacingApiError(error),
          });
        }
      },
    );

    return () => {
      active = false;
    };
  }, [api, overviewReloadKey]);

  function retryOverview(): void {
    setOverviewState({ phase: "loading" });
    setOverviewReloadKey((key) => key + 1);
  }

  if (status.showEnable) {
    return <NotEnabledHome title={status.title} description={status.description} />;
  }

  return (
    <div className="page home">
      <header className="home-summary">
        <h1>{status.title}</h1>
        {status.phase === "loading" ? null : (
          <p className="home-summary__scope">整体保护：{overallLabel}</p>
        )}
      </header>

      {status.attention ? (
        <AttentionBlock
          title={status.attention.title}
          message={status.attention.message}
          tone={status.tone}
          alert={status.phase === "failed" || status.phase === "disconnected"}
          canRetry={status.canRetry}
          onRetry={() => void sensorQuery.refresh()}
        />
      ) : null}

      {disabledNativeSwitches.length > 0 && protectionCheckedAt ? (
        <p className="home-native-alert" role="status">
          <strong>系统防护：</strong>
          {disabledNativeSwitches.join("、")}未启用
          <span className="home-native-alert__meta">上次读取 {protectionCheckedAt}</span>
        </p>
      ) : null}

      {status.phase === "loading" ? null : (
        <div className="home-details">
          {showRuntimeDetails ? (
            <ExpandableSection title="运行" summary={receivedSummary}>
              <RuntimeDetails sensorView={sensorView} />
            </ExpandableSection>
          ) : null}

          <ExpandableSection title="系统防护" summary={protectionSummary}>
            <NativeProtectionInline compact />
          </ExpandableSection>

          <ExpandableSection
            title="检查与设置"
            summary={overview ? undefined : overviewError ? "读取失败" : "读取中"}
          >
            <ChecksContent
              overview={overview}
              overviewError={overviewError}
              onRetry={retryOverview}
              onOpenCleanup={onOpenCleanup}
            />
          </ExpandableSection>
        </div>
      )}
    </div>
  );
}
