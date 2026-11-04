import { useId } from "react";

import { AssociationRelationItem } from "./AssociationRelationItem";
import { ExpandableSection } from "./ExpandableSection";
import { NativeProtectionResultView } from "./NativeProtectionCard";
import {
  associationSourceLabel,
  inventoryObservationLabel,
  postureReadStateLabel,
  postureTriageCategoryLabel,
  softwareAssociationStateLabel,
} from "../lib/uiLabels";
import {
  derivePosture,
  derivePostureTriage,
  type AssociationPosture,
  type ProtectionPosture,
} from "../lib/posture";
import type { ScanController } from "../lib/useScanAction";
import type {
  AssetAssociationResult,
  AssetRelation,
  AssociationSource,
  AssociationSourceStatus,
  NativeProtectionStatus,
  SoftwareAssociationState,
} from "../types";

const RELATION_PREVIEW_LIMIT = 6;

interface SecurityPostureProps {
  association: ScanController<AssetAssociationResult>;
  protection: ScanController<NativeProtectionStatus>;
}

function readStateClass(state: string): string {
  return `posture-state posture-state--${state}`;
}


function sourceFor(
  association: AssociationPosture,
  source: AssociationSource,
): AssociationSourceStatus | null {
  return association.result?.sources.find((entry) => entry.source === source) ?? null;
}

function sourceCountSummary(source: AssociationSourceStatus | null): string {
  if (source === null) {
    return "来源未读取";
  }
  if (
    source.observation === "unavailable" ||
    source.observation === "accessDenied" ||
    source.observation === "error"
  ) {
    return `来源${inventoryObservationLabel(source.observation)}`;
  }
  const state =
    source.observation === "observed"
      ? ""
      : ` · ${inventoryObservationLabel(source.observation)}`;
  const truncated = source.truncated ? " · 结果不完整" : "";
  return `${source.itemCount} 项（${source.addressableCount} 项有可比较路径）${state}${truncated}`;
}

function relationCounts(
  relations: AssetRelation[],
  kind: "process" | "service" | "scheduledTask" | "startupItem",
): number {
  return relations.filter((relation) => relation.target.kind === kind).length;
}

function softwareStateClass(state: SoftwareAssociationState): string {
  return `posture-software-state posture-software-state--${state}`;
}

function RelationPreview({
  relations,
  emptyMessage,
}: {
  relations: AssetRelation[];
  emptyMessage: string;
}) {
  if (relations.length === 0) {
    return <p className="posture-empty">{emptyMessage}</p>;
  }

  const preview = relations.slice(0, RELATION_PREVIEW_LIMIT);
  const remainder = relations.slice(RELATION_PREVIEW_LIMIT);

  return (
    <>
      <ul className="association-relation-list">
        {preview.map((relation) => (
          <AssociationRelationItem key={relation.id} relation={relation} showSoftware />
        ))}
      </ul>
      {remainder.length > 0 ? (
        <details className="posture-more">
          <summary>
            查看其余 {remainder.length} 条（共 {relations.length} 条）
          </summary>
          <ul className="association-relation-list">
            {remainder.map((relation) => (
              <AssociationRelationItem key={relation.id} relation={relation} showSoftware />
            ))}
          </ul>
        </details>
      ) : null}
    </>
  );
}

function ProtectionSection({
  protection,
  ready,
  failed,
  loading,
  compact = false,
}: {
  protection: ProtectionPosture;
  ready: boolean;
  failed: boolean;
  loading: boolean;
  compact?: boolean;
}) {
  const headingId = useId();

  if (compact) {
    return ready && protection.status ? (
      <NativeProtectionResultView status={protection.status} compact />
    ) : (
      <p className="posture-empty">
        {failed
          ? "原生防护状态读取未完成。"
          : loading
            ? "正在读取…"
            : "尚未读取。"}
      </p>
    );
  }

  return (
    <section className="posture-block" aria-labelledby={headingId}>
      <header className="posture-block__header">
        <div>
          <h3 id={headingId}>原生防护状态</h3>
          <p>来自 Windows Defender 与 Windows 防火墙的只读开关和签名快照。</p>
        </div>
        <span className={readStateClass(protection.readState)}>
          {ready
            ? postureReadStateLabel(protection.readState)
            : failed
              ? "来源不可用"
              : loading
                ? "正在读取"
                : "尚未读取"}
        </span>
      </header>

      {ready && protection.status ? (
        <NativeProtectionResultView status={protection.status} />
      ) : (
        <p className="posture-empty">
          {failed
            ? "原生防护状态读取未完成；没有保留旧结果。未知不等于正常，也不等于防护已关闭。"
            : loading
              ? "正在读取 Defender 与防火墙的只读快照；不会修改系统设置。"
              : "尚未读取原生防护状态。读取不会修改 Defender、防火墙或其他系统设置。"}
        </p>
      )}

      <p className="fine-print">
        数据来源：Defender WMI `MSFT_MpComputerStatus` 与防火墙 `INetFwPolicy2`。只显示当前观测值，不验证实际拦截能力，不改变 SentinelLite 的整体保护状态。
      </p>
    </section>
  );
}

function AssociationSources({
  association,
  loading,
}: {
  association: AssociationPosture;
  loading: boolean;
}) {
  const headingId = useId();
  const result = association.result;

  return (
    <section className="posture-block" aria-labelledby={headingId}>
      <header className="posture-block__header">
        <div>
          <h3 id={headingId}>资产与关联来源</h3>
          <p>
            软件、进程、服务、计划任务和启动项分别读取；一个来源不完整不会把其他来源的结果清空。
          </p>
        </div>
        <span className={readStateClass(association.readState)}>
          {association.readState === "notRead"
            ? loading
              ? "正在读取"
              : "尚未读取"
            : postureReadStateLabel(association.readState)}
        </span>
      </header>

      {result ? (
        <>
          {result.truncated ? (
            <p className="posture-warning">
              结果不完整：至少一个来源或关联集合被截断或读取失败；不能视为完整关联集。
            </p>
          ) : null}
          <div className="posture-source-grid">
            {result.sources.map((source) => (
              <article className="posture-source-card" key={source.id}>
                <header>
                  <strong>{associationSourceLabel(source.source)}</strong>
                  <span className={`inventory-badge inventory-badge--${source.observation}`}>
                    {inventoryObservationLabel(source.observation)}
                  </span>
                </header>
                <p>
                  {source.itemCount} 项 · {source.addressableCount} 项有可比较路径
                </p>
                {source.unresolvedPathCount > 0 ? (
                  <p className="fine-print">
                    {source.unresolvedPathCount} 项没有可比较路径（未当作不存在）
                  </p>
                ) : null}
                {source.truncated ? (
                  <p className="cell-note cell-note--warning">
                    来源结果不完整（容量或读取失败），未列出项不代表不存在
                  </p>
                ) : null}
              </article>
            ))}
          </div>
          <p className="fine-print">
            来源为部分读取、不可用或截断时，关联结果显示证据不足；不能据此断言“没有关联”。
          </p>
        </>
      ) : (
        <p className="posture-empty">
          {association.readState === "unavailable"
            ? "资产关联来源没有成功读取；没有保留旧结果。"
            : loading
              ? "正在读取软件、进程、服务、计划任务和启动项的只读快照。"
              : "尚未读取资产关联来源。"}
          未读取不会显示为“没有关联”。
        </p>
      )}
    </section>
  );
}

function TriageSection({
  protection,
  association,
  protectionReady,
  associationReady,
  protectionFailed,
  associationFailed,
}: {
  protection: ProtectionPosture;
  association: AssociationPosture;
  protectionReady: boolean;
  associationReady: boolean;
  protectionFailed: boolean;
  associationFailed: boolean;
}) {
  const headingId = useId();
  const triage = derivePostureTriage(protection, association);
  const protectionValuesReady = protectionReady;
  const associationValuesReady = associationReady;
  const associationCount = (value: number) =>
    associationValuesReady ? String(value) : "—";
  const protectionCount = (value: number) =>
    protectionValuesReady ? String(value) : "—";
  const sourceStatusNote = [
    associationValuesReady
      ? null
      : associationFailed
        ? "关联来源不可用，相关分类计数显示为 —。"
        : "关联来源尚未读取，相关分类计数显示为 —。",
    protectionValuesReady
      ? null
      : protectionFailed
        ? "防护来源不可用，相关分类计数显示为 —。"
        : "防护来源尚未读取，相关分类计数显示为 —。",
  ]
    .filter((value): value is string => value !== null)
    .join(" ");

  return (
    <section className="posture-block" aria-labelledby={headingId}>
      <header className="posture-block__header">
        <div>
          <h3 id={headingId}>线索分类</h3>
          <p>
            区分已观察事实、值得核实、证据不足和来源不可用；分类不是安全评分，也不是恶意判定。
          </p>
        </div>
      </header>

      <div className="posture-triage-grid">
        <article className="posture-triage-card posture-triage-card--observed">
          <h4>{postureTriageCategoryLabel("observed")}</h4>
          <p>
            {associationCount(triage.observed.exactRelations)} 条精确路径 ·{" "}
            {associationCount(triage.observed.strongRelations)} 条安装目录内路径 ·{" "}
            {protectionCount(triage.observed.enabledProtectionSwitches)} 个已启用开关 ·{" "}
            {associationCount(triage.observed.observedSources)} 个来源已读取
          </p>
          <p className="fine-print">这些只是静态路径或开关事实，不代表软件安全或系统受保护。</p>
        </article>

        <article className="posture-triage-card posture-triage-card--attention">
          <h4>{postureTriageCategoryLabel("attention")}</h4>
          <p>
            {associationCount(triage.attention.weakRelations)} 条弱线索 ·{" "}
            {associationCount(triage.attention.ambiguousRelations)} 条多候选 ·{" "}
            {protectionCount(triage.attention.disabledProtectionSwitches)} 个未启用开关
          </p>
          <p className="fine-print">
            弱线索、多候选和未启用开关值得人工核实；它们不是恶意软件判定，也不能单独证明归属。
          </p>
        </article>

        <article className="posture-triage-card posture-triage-card--insufficient">
          <h4>{postureTriageCategoryLabel("insufficient")}</h4>
          <p>
            {associationCount(triage.insufficient.insufficientSoftware)} 个软件证据不足 ·{" "}
            {associationCount(triage.insufficient.partialSources)} 个来源部分读取 ·{" "}
            {protectionCount(triage.insufficient.unknownProtectionFields)} 个防护字段未知 ·{" "}
            {associationCount(triage.insufficient.partialRelations)} 条关系观察不完整
          </p>
          <p className="fine-print">证据不足不等于安全，也不等于“没有关联”；需要重新读取或人工核实。</p>
        </article>

        <article className="posture-triage-card posture-triage-card--unavailable">
          <h4>{postureTriageCategoryLabel("unavailable")}</h4>
          <p>
            {associationCount(triage.unavailable.unavailableSoftware)} 个软件来源不可用 ·{" "}
            {associationCount(triage.unavailable.unavailableSources)} 个资产来源不可用 ·{" "}
            {protectionCount(triage.unavailable.unavailableProtectionProviders)} 个防护提供程序不可用
          </p>
          <p className="fine-print">来源不可用不能显示为正常；当前无法判断对应对象或防护状态。</p>
        </article>
      </div>
      {sourceStatusNote.length > 0 ? (
        <p className="fine-print posture-triage-note">{sourceStatusNote}</p>
      ) : null}
    </section>
  );
}

function PersistentClues({ association }: { association: AssociationPosture }) {
  const headingId = useId();
  const taskSource = sourceFor(association, "scheduledTaskInventory");
  const startupSource = sourceFor(association, "startupInventory");
  const ready = association.result !== null;

  return (
    <section className="posture-block" aria-labelledby={headingId}>
      <header className="posture-block__header">
        <div>
          <h3 id={headingId}>持久化入口的可追踪线索</h3>
          <p>
            仅展示计划任务与启动项动作路径和软件静态路径的匹配；不读取任务参数、触发器或启动项参数文本。
          </p>
        </div>
        <span className="posture-count">
          {ready ? `${association.persistentRelations.length} 条关系` : "尚未读取"}
        </span>
      </header>

      {ready ? (
        <>
          <p className="posture-source-line">
            计划任务 {sourceCountSummary(taskSource)} · 启动项{" "}
            {sourceCountSummary(startupSource)} · 形成 {association.persistentRelations.length}{" "}
            条可追踪关系
          </p>
          <RelationPreview
            relations={association.persistentRelations}
            emptyMessage={
              association.readState === "complete"
                ? "当前快照未发现计划任务或启动项的路径匹配。没有关联不等于异常，也不说明这些入口没有软件来源。"
                : "当前没有可展示的持久化路径线索；来源不完整，不能据此判断不存在。"
            }
          />
        </>
      ) : (
        <p className="posture-empty">
          {association.readState === "unavailable"
            ? "持久化来源没有成功读取；未保留旧结果。"
            : "尚未读取持久化来源。"}
        </p>
      )}
    </section>
  );
}

function ProcessServiceClues({ association }: { association: AssociationPosture }) {
  const headingId = useId();
  const ready = association.result !== null;
  const processCount = relationCounts(association.processServiceRelations, "process");
  const serviceCount = relationCounts(association.processServiceRelations, "service");

  return (
    <section className="posture-block" aria-labelledby={headingId}>
      <header className="posture-block__header">
        <div>
          <h3 id={headingId}>进程与服务的关联线索</h3>
          <p>
            使用当前进程可执行路径和服务二进制路径；服务同时属于持久化组件，此区只展示一次，不重复计入计划任务/启动项。
          </p>
        </div>
        <span className="posture-count">
          {ready ? `${association.processServiceRelations.length} 条关系` : "尚未读取"}
        </span>
      </header>

      {ready ? (
        <>
          <p className="posture-source-line">
            运行进程 {processCount} 条 · Windows 服务 {serviceCount} 条 · 关联目标包括{" "}
            {association.processServiceRelations.length === 0
              ? "当前没有可展示目标"
              : `${processCount + serviceCount} 个路径证据`}
          </p>
          <RelationPreview
            relations={association.processServiceRelations}
            emptyMessage={
              association.readState === "complete"
                ? "当前快照未发现进程或服务的路径匹配。没有关联不等于异常。"
                : "当前没有可展示的进程或服务路径线索；来源不完整，不能据此判断不存在。"
            }
          />
        </>
      ) : (
        <p className="posture-empty">
          {association.readState === "unavailable"
            ? "进程或服务来源没有成功读取；未保留旧结果。"
            : "尚未读取进程与服务来源。"}
        </p>
      )}
    </section>
  );
}

function WeakClues({ association }: { association: AssociationPosture }) {
  const headingId = useId();

  return (
    <section className="posture-block" aria-labelledby={headingId}>
      <header className="posture-block__header">
        <div>
          <h3 id={headingId}>弱线索与多候选</h3>
          <p>
            弱线索只表示文件名相同但目录不同；多候选表示同一目标可由多个软件解释。
            这是对全部关系的交叉筛选，可能与上方列表重复。
          </p>
        </div>
        <span className="posture-count">
          {association.result
            ? `${association.weakOrAmbiguousRelations.length} 条待核实`
            : "尚未读取"}
        </span>
      </header>

      {association.result ? (
        <RelationPreview
          relations={association.weakOrAmbiguousRelations}
          emptyMessage="本次快照没有弱线索或多候选。没有弱线索不代表所有关联都完整，也不代表没有风险。"
        />
      ) : (
        <p className="posture-empty">尚未读取关联来源。</p>
      )}
    </section>
  );
}

function SoftwareStateSection({ association }: { association: AssociationPosture }) {
  const headingId = useId();
  const ready = association.result !== null;
  const counts = association.softwareStateCounts;
  const topRows = association.linkedSoftwareRows.slice(0, 8);

  return (
    <section className="posture-block" aria-labelledby={headingId}>
      <header className="posture-block__header">
        <div>
          <h3 id={headingId}>软件关联状态</h3>
          <p>
            汇总软件条目的关联状态；完整软件清单和逐项字段在“软件盘点”标签中查看。
          </p>
        </div>
        <span className="posture-count">
          {ready
            ? association.linkedSoftwareRows.length > topRows.length
              ? `显示前 ${topRows.length} / ${association.linkedSoftwareRows.length} 个有线索软件`
              : `${association.linkedSoftwareRows.length} 个有路径线索的软件`
            : "尚未读取"}
        </span>
      </header>

      {ready ? (
        <>
          <div className="posture-state-grid">
            {(
              [
                ["linked", "存在路径关联"],
                ["weakOnly", "仅弱线索"],
                ["noMatch", "当前快照未匹配"],
                ["insufficientEvidence", "证据不足"],
                ["sourceUnavailable", "来源不可用"],
              ] as const
            ).map(([state]) => (
              <div className="posture-state-count" key={state}>
                <span className={softwareStateClass(state)}>
                  {softwareAssociationStateLabel(state)}
                </span>
                <strong>{counts[state]}</strong>
              </div>
            ))}
          </div>

          {topRows.length > 0 ? (
            <div className="posture-software-list">
              {topRows.map((row) => (
                <div className="posture-software-row" key={row.softwareId}>
                  <div>
                    <strong>{row.name}</strong>
                    <span className={softwareStateClass(row.state)}>
                      {softwareAssociationStateLabel(row.state)}
                    </span>
                  </div>
                  <p>
                    {row.exact} 精确 · {row.strong} 安装目录内 · {row.weak} 弱线索 · 进程{" "}
                    {row.process} · 服务 {row.service} · 任务 {row.scheduledTask} · 启动项{" "}
                    {row.startupItem}
                  </p>
                </div>
              ))}
            </div>
          ) : (
            <p className="posture-empty">
              当前快照没有发现带路径证据的软件关联；这不等于系统没有后台组件或启动项。
            </p>
          )}

          <p className="fine-print">
            “当前快照未匹配”只适用于来源完整且没有截断的情况；证据不足和来源不可用不能合并为“没有关联”。
          </p>
        </>
      ) : (
        <p className="posture-empty">
          {association.readState === "unavailable"
            ? "软件关联来源没有成功读取；未保留旧结果。"
            : "尚未读取软件关联状态。"}
        </p>
      )}
    </section>
  );
}

function PostureNotes({
  protection,
  association,
}: {
  protection: ProtectionPosture;
  association: AssociationPosture;
}) {
  const issues = [
    ...(protection.status
      ? [...protection.status.defender.issues, ...protection.status.firewall.issues].map((issue) => ({
          code: issue.code,
          message: issue.message,
          source: "native protection",
        }))
      : []),
    ...(association.result?.issues ?? []),
  ];
  const limitations = [
    ...(protection.status?.limitations ?? []),
    ...(association.result?.limitations ?? []),
  ];

  if (issues.length === 0 && limitations.length === 0) {
    return null;
  }

  return (
    <details className="posture-notes">
      <summary>读取问题与证据边界</summary>
      {issues.length > 0 ? (
        <>
          <ul>
            {issues.slice(0, 16).map((issue, index) => (
              <li key={`${issue.code}:${index}`}>
                <code>{issue.code}</code>
                <span>{issue.message}</span>
              </li>
            ))}
          </ul>
          {issues.length > 16 ? (
            <p className="fine-print">
              界面仅显示前 16 条读取问题；其余问题仍保留在本次结果中。
            </p>
          ) : null}
        </>
      ) : null}
      {limitations.length > 0 ? (
        <ul>
          {limitations.map((limitation) => (
            <li key={limitation}>{limitation}</li>
          ))}
        </ul>
      ) : null}
    </details>
  );
}

/**
 * Read-only integration of the existing native-protection and asset-association snapshots.
 * It deliberately has no aggregate security score and no action buttons.
 */
export function SecurityPosture({ association, protection }: SecurityPostureProps) {
  const protectionData = protection.state.phase === "ready" ? protection.state.data : null;
  const associationData = association.state.phase === "ready" ? association.state.data : null;
  const protectionError = protection.state.phase === "error" ? protection.state.message : null;
  const associationError = association.state.phase === "error" ? association.state.message : null;
  const protectionFailed = protectionError !== null;
  const associationFailed = associationError !== null;
  const loading = protection.state.phase === "loading" || association.state.phase === "loading";

  const posture = derivePosture(
    protectionData,
    protectionFailed,
    associationData,
    associationFailed,
  );

  const readLabel = loading
    ? "正在读取"
    : posture.readState === "notRead"
      ? "尚未读取"
      : postureReadStateLabel(posture.readState);

  const protectionValuesReady = protectionData !== null;
  const associationValuesReady = associationData !== null;
  const disabledCount = posture.protection.disabled.length;
  const unknownCount = posture.protection.unknown.length;
  const unavailableSources = posture.association.sourceUnavailableCount;
  const hasData = protectionData !== null || associationData !== null;

  const buttonLabel = loading
    ? "正在读取…"
    : protectionData || associationData
      ? "重新读取安全态势"
      : protectionFailed || associationFailed
        ? "重新尝试读取"
        : "读取安全态势";

  const glanceParts: string[] = [];
  if (loading && !hasData) {
    glanceParts.push("正在读取…");
  } else if (!hasData) {
    glanceParts.push(protectionFailed || associationFailed ? "读取失败" : "尚未读取");
  } else {
    if (!loading && posture.readState !== "complete") {
      glanceParts.push(readLabel);
    }
    if (protectionValuesReady) {
      glanceParts.push(
        disabledCount > 0
          ? `${disabledCount} 个防护开关未启用`
          : `${posture.protection.enabled.length} 个防护开关已启用`,
      );
      if (unknownCount > 0) {
        glanceParts.push(`${unknownCount} 个未知`);
      }
    }
    if (associationValuesReady) {
      glanceParts.push(`${posture.association.relationCount} 条关联线索`);
      if (unavailableSources > 0) {
        glanceParts.push(`${unavailableSources} 个来源不可用`);
      }
    }
    if (protectionFailed || associationFailed) {
      glanceParts.push("部分读取失败");
    }
  }
  const glanceText = glanceParts.join(" · ");
  const glanceWarning =
    disabledCount > 0 ||
    unavailableSources > 0 ||
    protectionFailed ||
    associationFailed ||
    (hasData && posture.readState !== "complete");

  function runPosture(): void {
    if (loading) {
      return;
    }
    void Promise.all([association.start(), protection.start()]);
  }

  return (
    <section className="security-posture" aria-label="安全状态">
      <div className="posture-glance">
        <p
          className={
            glanceWarning
              ? "posture-glance__text is-warning"
              : "posture-glance__text"
          }
          aria-live="polite"
        >
          {glanceText}
        </p>
        <button
          type="button"
          className="button button--primary"
          onClick={runPosture}
          disabled={loading}
          aria-busy={loading}
        >
          {buttonLabel}
        </button>
      </div>

      {protectionFailed || associationFailed ? (
        <div className="posture-errors">
          {protectionFailed ? (
            <div className="notice notice--error" role="alert">
              <div>
                <h3>原生防护状态读取未完成</h3>
                <p>{protectionError}</p>
              </div>
            </div>
          ) : null}
          {associationFailed ? (
            <div className="notice notice--error" role="alert">
              <div>
                <h3>资产关联读取未完成</h3>
                <p>{associationError}</p>
              </div>
            </div>
          ) : null}
        </div>
      ) : null}

      {protectionData ? (
        <ExpandableSection
          title="系统防护"
          summary={
            protectionValuesReady
              ? disabledCount > 0
                ? `${disabledCount} 个未启用`
                : `${posture.protection.enabled.length} 个已启用`
              : readLabel
          }
        >
          <ProtectionSection
            protection={posture.protection}
            ready={protectionValuesReady}
            failed={protectionFailed}
            loading={loading}
            compact
          />
        </ExpandableSection>
      ) : null}

      {associationData || protectionData ? (
        <ExpandableSection
          title="关联信息"
          summary={
            associationData
              ? `${posture.association.relationCount} 条线索`
              : "部分不可用"
          }
        >
          {associationData ? (
            <AssociationSources association={posture.association} loading={loading} />
          ) : null}
          <TriageSection
            protection={posture.protection}
            association={posture.association}
            protectionReady={protectionValuesReady}
            associationReady={associationValuesReady}
            protectionFailed={protectionFailed}
            associationFailed={associationFailed}
          />
          {associationData ? (
            <>
              <PersistentClues association={posture.association} />
              <ProcessServiceClues association={posture.association} />
              <WeakClues association={posture.association} />
              <SoftwareStateSection association={posture.association} />
              <PostureNotes
                protection={posture.protection}
                association={posture.association}
              />
            </>
          ) : null}
        </ExpandableSection>
      ) : null}
    </section>
  );
}
