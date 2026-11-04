import { useId } from "react";

import { AssociationRelationItem } from "./AssociationRelationItem";
import { InventoryIncompleteNotice } from "./InventoryIncompleteNotice";
import { formatCheckedAt } from "../lib/presentation";
import {
  associationSourceLabel,
  associationTargetKindLabel,
  inventoryObservationLabel,
  softwareAssociationStateLabel,
} from "../lib/uiLabels";
import type { ScanState } from "../lib/useScanAction";
import type {
  AssetAssociationResult,
  AssetRelation,
  AssociationTargetKind,
  SoftwareAssociationState,
} from "../types";

const TARGET_KIND_ORDER: AssociationTargetKind[] = [
  "process",
  "service",
  "scheduledTask",
  "startupItem",
];

interface SoftwareAssociationsProps {
  softwareId: string;
  state: ScanState<AssetAssociationResult>;
  onStart: () => void;
}

function stateClass(state: SoftwareAssociationState): string {
  return `association-state association-state--${state}`;
}

function relationGroups(
  relations: AssetRelation[],
): { kind: AssociationTargetKind; relations: AssetRelation[] }[] {
  return TARGET_KIND_ORDER.map((kind) => ({
    kind,
    relations: relations.filter((relation) => relation.target.kind === kind),
  })).filter((group) => group.relations.length > 0);
}

/**
 * Detail-level association evidence. This component only renders already-read
 * snapshots; it never triggers an association target, reads a command line, or
 * offers a destructive action.
 */
export function SoftwareAssociations({
  softwareId,
  state,
  onStart,
}: SoftwareAssociationsProps) {
  const headingId = useId();
  const data = state.phase === "ready" ? state.data : null;
  const summary = data?.software.find((entry) => entry.softwareId === softwareId) ?? null;
  const relations = data?.relations.filter((relation) => relation.softwareId === softwareId) ?? [];
  const groups = relationGroups(relations);

  const buttonLabel =
    state.phase === "loading"
      ? "正在读取关联…"
      : state.phase === "ready"
        ? "重新读取关联"
        : state.phase === "error"
          ? "重新尝试关联"
          : "读取资产关联";

  return (
    <section className="software-associations" aria-labelledby={headingId}>
      <header className="software-associations__header">
        <div>
          <h4 id={headingId}>资产关联</h4>
          <p>
            使用当前进程、服务、计划任务和启动项的静态路径证据，解释这个软件条目与哪些对象相关。
            只读操作，不启动、停止或修改任何对象。
          </p>
        </div>
        <button
          type="button"
          className="button button--quiet"
          onClick={onStart}
          disabled={state.phase === "loading"}
        >
          {buttonLabel}
        </button>
      </header>

      {state.phase === "idle" ? (
        <p className="association-idle">
          尚未读取关联来源。没有读取到关联不代表软件没有运行组件或持久化项。
        </p>
      ) : null}

      {state.phase === "error" ? (
        <div className="notice notice--error" role="alert">
          <div>
            <strong>关联读取未完成</strong>
            <p>{state.message}</p>
            <p className="fine-print">
              未保留上一次关联结果；空结果或读取失败都不表示“没有关联”。
            </p>
          </div>
        </div>
      ) : null}

      {state.phase === "loading" ? (
        <p className="association-idle" role="status">
          正在读取进程快照、服务、计划任务和启动项；不会执行任何目标程序。
        </p>
      ) : null}

      {data ? (
        <>
          <div className="association-summary">
            <span className={`inventory-badge inventory-badge--${data.observation}`}>
              {inventoryObservationLabel(data.observation)}
            </span>
            <p>
              {relations.length} 条当前软件关系 · {data.sources.length} 个只读来源 · 读取时间{" "}
              {formatCheckedAt(data.checkedAt)}
            </p>
            {data.truncated ? (
              <InventoryIncompleteNotice
                scope="关联"
                issues={data.issues}
                sourceTruncated={data.sources.some((source) => source.truncated)}
                consequence="未列出关系不代表不存在关联。"
              />
            ) : null}
          </div>

          <div className="association-source-grid">
            {data.sources.map((source) => (
              <article className="association-source-card" key={source.id}>
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
              </article>
            ))}
          </div>

          {summary ? (
            <div className="association-verdict">
              <span className={stateClass(summary.state)}>
                {softwareAssociationStateLabel(summary.state)}
              </span>
              <p>
                路径证据 {summary.pathEvidenceCount} 个 · 强关联 {summary.strongRelationCount} 个 ·
                弱线索 {summary.weakRelationCount} 个
              </p>
              {summary.state === "noMatch" ? (
                <p className="fine-print">
                  所有可读来源均未发现规范化路径匹配。没有关联不等于异常，也不证明软件没有组件。
                </p>
              ) : null}
              {summary.state === "insufficientEvidence" ? (
                <p className="fine-print">
                  软件路径、目标来源或读取完整性不足；不能据此断言“没有关联”。
                </p>
              ) : null}
              {summary.state === "sourceUnavailable" ? (
                <p className="fine-print">
                  没有可用的关联目标来源；这不是“没有关联”，而是当前无法判断。
                </p>
              ) : null}
              {summary.state === "weakOnly" ? (
                <p className="fine-print">
                  只发现文件名弱线索；弱线索不能证明软件归属或恶意性。
                </p>
              ) : null}
              {summary.issues.length > 0 ? (
                <ul className="association-issue-list">
                  {summary.issues.map((issue) => (
                    <li key={`${issue.code}:${issue.message}`}>
                      <code>{issue.code}</code>
                      <span>{issue.message}</span>
                    </li>
                  ))}
                </ul>
              ) : null}
            </div>
          ) : (
            <div className="notice notice--warning">
              <div>
                <strong>关联快照中没有这个软件条目</strong>
                <p>
                  软件可能在两次读取之间发生变化，或本次关联读取没有返回同一注册表条目。
                  请重新读取关联，不要把缺失当作“没有关系”。
                </p>
              </div>
            </div>
          )}

          {groups.length > 0 ? (
            <div className="association-groups">
              {groups.map((group) => (
                <section className="association-group" key={group.kind}>
                  <h5>
                    {associationTargetKindLabel(group.kind)} ({group.relations.length})
                  </h5>
                  <ul className="association-relation-list">
                    {group.relations.map((relation) => (
                      <AssociationRelationItem
                        key={relation.id}
                        relation={relation}
                      />
                    ))}
                  </ul>
                </section>
              ))}
            </div>
          ) : (
            <p className="association-idle">
              {summary?.state === "insufficientEvidence" || summary?.state === "sourceUnavailable"
                ? "证据不足：当前结果不能判断这个软件是否存在关联对象。"
                : "当前只读快照中没有可显示的路径关联。"}
            </p>
          )}

          {data.issues.length > 0 ? (
            <details className="association-notes">
              <summary>关联读取问题 ({data.issues.length})</summary>
              <ul>
                {data.issues.slice(0, 8).map((issue) => (
                  <li key={`${issue.code}:${issue.message}`}>
                    <code>{issue.code}</code>
                    <span>{issue.message}</span>
                  </li>
                ))}
              </ul>
            </details>
          ) : null}

          <details className="association-notes">
            <summary>关联证据边界</summary>
            <ul>
              {data.limitations.map((limitation) => (
                <li key={limitation}>{limitation}</li>
              ))}
            </ul>
          </details>
        </>
      ) : null}
    </section>
  );
}
