import { redactUserPath } from "../lib/privacy";
import {
  associationCertaintyLabel,
  associationEvidenceTypeLabel,
  associationMatchMethodLabel,
  associationTargetSourceLabel,
  inventoryObservationLabel,
} from "../lib/uiLabels";
import type { AssetRelation } from "../types";

function displayPath(value: string | null): string {
  return redactUserPath(value) ?? "未提供";
}

interface AssociationRelationItemProps {
  relation: AssetRelation;
  showSoftware?: boolean;
}

/**
 * Renders already-read association evidence only. It does not read a command line,
 * execute the target, or expose a destructive action.
 */
export function AssociationRelationItem({
  relation,
  showSoftware = false,
}: AssociationRelationItemProps) {
  return (
    <li className="association-relation">
      <header>
        <div>
          <strong>{relation.target.name}</strong>
          <span className="cell-note">{relation.target.detail}</span>
          {showSoftware ? (
            <span className="cell-note">
              软件：{relation.softwareName}
            </span>
          ) : null}
        </div>
        <div className="association-badges">
          <span
            className={`association-certainty association-certainty--${relation.certainty}`}
          >
            {associationCertaintyLabel(relation.certainty)}
          </span>
          <span
            className={`inventory-badge inventory-badge--${relation.observation}`}
          >
            {inventoryObservationLabel(relation.observation)}
          </span>
        </div>
      </header>
      <p className="association-match">
        {associationMatchMethodLabel(relation.matchMethod)}
        <span className="cell-note">
          来源：{associationTargetSourceLabel(relation.target.kind)} · 证据类型：
          {associationEvidenceTypeLabel(relation.evidenceType)}
        </span>
      </p>
      <p className="fine-print breakable mono-text">
        证据：{displayPath(relation.evidenceValue)}
      </p>
      {relation.softwareEvidencePath ? (
        <p className="fine-print breakable mono-text">
          软件路径线索：{displayPath(relation.softwareEvidencePath)}
        </p>
      ) : null}
      {relation.ambiguous ? (
        <p className="cell-note cell-note--warning">
          同一目标同时匹配 {relation.candidateSoftwareCount} 个软件候选；未强行选择唯一归属。
        </p>
      ) : null}
      {relation.issues.length > 0 ? (
        <ul className="association-issue-list">
          {relation.issues.map((issue) => (
            <li key={`${issue.code}:${issue.message}`}>
              <code>{issue.code}</code>
              <span>{issue.message}</span>
            </li>
          ))}
        </ul>
      ) : null}
    </li>
  );
}
