import type { ReactNode } from "react";

import type {
  DefenderStatus,
  FirewallStatus,
  NativeProtectionProviderState,
  NativeProtectionStatus,
} from "../types";
import { formatCheckedAt } from "../lib/presentation";
import {
  boolLabel,
  disabledProtectionSwitches,
  FIREWALL_PROFILE_LABELS,
  NATIVE_PROTECTION_OBSERVATION_LABELS,
  NATIVE_PROTECTION_PROVIDER_STATE_LABELS,
  unknownProtectionFieldCount,
} from "../lib/nativeProtection";
import { useNativeProtectionScan } from "../lib/nativeProtectionScan";
import type { ScanState } from "../lib/useScanAction";

const NATIVE_PROTECTION_INTRO =
  "只读查询 Windows Defender 与防火墙的当前开关和签名状态。查询失败或字段缺失会显示未知，不会回退为“已启用”。";

function NumberValue({ value }: { value: number | null }) {
  if (value === null) {
    return <span className="is-unknown">未知</span>;
  }
  return <>{value} 天</>;
}

function TextValue({ value }: { value: string | null }) {
  if (value === null) {
    return <span className="is-unknown">未知</span>;
  }
  return <>{value}</>;
}

function BoolValue({ value }: { value: boolean | null }) {
  return (
    <span className={value === null ? "is-unknown" : undefined}>{boolLabel(value)}</span>
  );
}

function ValueRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="native-protection-row">
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

function ProviderHeader({
  title,
  state,
}: {
  title: string;
  state: NativeProtectionProviderState;
}) {
  return (
    <header className="native-protection-provider__heading">
      <h3>{title}</h3>
      <span className={`native-protection-provider__state is-${state}`}>
        {NATIVE_PROTECTION_PROVIDER_STATE_LABELS[state]}
      </span>
    </header>
  );
}

function DefenderBlock({ defender }: { defender: DefenderStatus }) {
  return (
    <section className="native-protection-provider">
      <ProviderHeader title="Windows Defender" state={defender.state} />
      <dl className="native-protection-values">
        <ValueRow label="防病毒">
          <BoolValue value={defender.antivirusEnabled} />
        </ValueRow>
        <ValueRow label="实时保护">
          <BoolValue value={defender.realTimeProtectionEnabled} />
        </ValueRow>
        <ValueRow label="反恶意软件服务">
          <BoolValue value={defender.amServiceEnabled} />
        </ValueRow>
        <ValueRow label="签名版本">
          <TextValue value={defender.signatureVersion} />
        </ValueRow>
        <ValueRow label="签名年龄">
          <NumberValue value={defender.signatureAgeDays} />
        </ValueRow>
      </dl>
    </section>
  );
}

function FirewallBlock({ firewall }: { firewall: FirewallStatus }) {
  return (
    <section className="native-protection-provider">
      <ProviderHeader title="Windows 防火墙" state={firewall.state} />
      <dl className="native-protection-values">
        {firewall.profiles.map((profile) => (
          <ValueRow key={profile.profile} label={FIREWALL_PROFILE_LABELS[profile.profile]}>
            <BoolValue value={profile.enabled} />
          </ValueRow>
        ))}
      </dl>
    </section>
  );
}

export function NativeProtectionResultView({
  status,
  compact = false,
}: {
  status: NativeProtectionStatus;
  compact?: boolean;
}) {
  const issues = [...status.defender.issues, ...status.firewall.issues];
  const disabled = disabledProtectionSwitches(status).map((entry) => entry.label);
  const allKnownSwitchesEnabled =
    status.observation === "complete" &&
    disabled.length === 0 &&
    unknownProtectionFieldCount(status) === 0;

  return (
    <div className="native-protection-result">
      <div className="native-protection-summary">
        <span
          className={`native-protection-badge native-protection-badge--${status.observation}`}
        >
          {NATIVE_PROTECTION_OBSERVATION_LABELS[status.observation]}
        </span>
        <span className="native-protection-checked-at">
          最近读取：{formatCheckedAt(status.checkedAt)}
        </span>
      </div>

      <div className="native-protection-grid">
        <DefenderBlock defender={status.defender} />
        <FirewallBlock firewall={status.firewall} />
      </div>

      {disabled.length > 0 ? (
        <p className="native-protection-attention">
          {compact ? (
            <>未启用：{disabled.join("、")}。这是开关观测，不是整体安全结论。</>
          ) : (
            <>
              观测到未启用：{disabled.join("、")}。这是原生防护开关的只读观测，不是恶意判定，也不代表
              SentinelLite 整体保护状态。
            </>
          )}
        </p>
      ) : null}

      {issues.length > 0 ? (
        <ul className="native-protection-issues" aria-label="原生防护读取问题">
          {issues.map((issue) => (
            <li key={`${issue.code}-${issue.message}`}>
              <span className="native-protection-issue__code">{issue.code}</span>
              {issue.message}
            </li>
          ))}
        </ul>
      ) : null}

      {allKnownSwitchesEnabled ? (
        <p className="native-protection-footnote">
          {compact
            ? "已读取的开关均为启用；这不等于完整保护。"
            : "已读取到的开关均为启用状态。这不能证明实际拦截能力、规则完整性或系统未受其他方式影响。"}
        </p>
      ) : null}

      {!compact && status.limitations.length > 0 ? (
        <ul className="native-protection-limitations" aria-label="原生防护观测限制">
          {status.limitations.map((limitation) => (
            <li key={limitation}>{limitation}</li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

function scanButtonLabel(state: ScanState<NativeProtectionStatus>): string {
  if (state.phase === "loading") {
    return "正在读取…";
  }
  return state.phase === "ready" ? "重新读取" : "读取原生防护状态";
}

function NativeProtectionScanStates({
  state,
  compact = false,
}: {
  state: ScanState<NativeProtectionStatus>;
  compact?: boolean;
}) {
  return (
    <>
      {state.phase === "idle" ? (
        <p className="native-protection-empty">
          {compact
            ? "尚未读取。"
            : "尚未检查。读取结果只代表当前观测到的原生防护开关，不改变 SentinelLite 的整体保护状态。"}
        </p>
      ) : null}

      {state.phase === "loading" ? (
        <p className="native-protection-empty" role="status">
          正在读取原生防护状态…
        </p>
      ) : null}

      {state.phase === "error" ? (
        <div className="notice notice--error" role="alert">
          <h3>无法读取原生防护状态</h3>
          <p>{state.message}</p>
          <p className="fine-print">
            {compact
              ? "未保留上一次结果。"
              : "未保留上一次结果。原生防护状态保持未知，不会按正常处理。"}
          </p>
        </div>
      ) : null}

      {state.phase === "ready" ? (
        <NativeProtectionResultView status={state.data} compact={compact} />
      ) : null}

      {state.phase !== "ready" && !compact ? (
        <p className="fine-print">
          不修改、不关闭原生防护；不启动子进程；不验证实际拦截能力。未知不等于正常。
        </p>
      ) : null}
    </>
  );
}

export function NativeProtectionCard() {
  const { state, start } = useNativeProtectionScan();

  return (
    <section className="panel native-protection-panel" aria-labelledby="native-protection-heading">
      <header className="section-heading">
        <div>
          <h2 id="native-protection-heading">原生防护状态</h2>
          <p>{NATIVE_PROTECTION_INTRO}</p>
        </div>
        <button
          type="button"
          className="button button--quiet"
          onClick={() => void start()}
          disabled={state.phase === "loading"}
        >
          {scanButtonLabel(state)}
        </button>
      </header>

      <NativeProtectionScanStates state={state} />
    </section>
  );
}

/**
 * The same user-triggered native-protection snapshot, without the outer card.
 * Used inside the home page disclosure so the native scan is available on demand.
 */
export function NativeProtectionInline({ compact = false }: { compact?: boolean }) {
  const { state, start } = useNativeProtectionScan();

  return (
    <div
      className={
        compact
          ? "native-protection-inline native-protection-inline--compact"
          : "native-protection-inline"
      }
    >
      <div className="native-protection-inline__header">
        {compact ? null : <p>{NATIVE_PROTECTION_INTRO}</p>}
        <button
          type="button"
          className="button button--quiet"
          onClick={() => void start()}
          disabled={state.phase === "loading"}
        >
          {scanButtonLabel(state)}
        </button>
      </div>

      <NativeProtectionScanStates state={state} compact={compact} />
    </div>
  );
}
