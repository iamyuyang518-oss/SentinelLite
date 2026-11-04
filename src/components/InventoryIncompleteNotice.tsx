import {
  deriveIncompleteReason,
  incompleteReasonSummary,
} from "../lib/inventoryCompleteness";
import type { DataIssue } from "../types";

interface InventoryIncompleteNoticeProps {
  scope: string;
  issues: DataIssue[];
  sourceTruncated?: boolean;
  consequence?: string;
}

export function InventoryIncompleteNotice({
  scope,
  issues,
  sourceTruncated = false,
  consequence = "未列出项不代表不存在。",
}: InventoryIncompleteNoticeProps) {
  const reason = deriveIncompleteReason(issues, sourceTruncated);

  return (
    <div className="truncated-notice" role="alert">
      <strong>{scope}结果不完整</strong>
      <p>{incompleteReasonSummary(reason)}</p>
      <p className="fine-print">{consequence}具体原因见下方读取问题。</p>
    </div>
  );
}
