import type { DataIssue } from "../types";

export type IncompleteReason =
  | "readFailure"
  | "capacity"
  | "fieldTruncation"
  | "sourceTruncated"
  | "unknown";

const READ_FAILURE_PATTERN =
  /COUNT-UNREADABLE|ENUMERATION-FAILED|READ-ERROR|READERROR|ACCESS-DENIED|UNAVAILABLE|FAILED/i;
const FIELD_TRUNCATION_PATTERN =
  /(?:STRING|FIELD|PATH|NAME|VALUE|ICON).*TRUNCATED|REGISTRYSTRINGTRUNCATED/i;
const CAPACITY_PATTERN =
  /TRUNCATED|TIME-?LIMIT|TIMELIMIT|PAGE-LIMIT|BUFFER-LIMIT|INVENTORYTRUNCATED|STARTUPTruncated|MAX_/i;

export function deriveIncompleteReason(
  issues: DataIssue[],
  sourceTruncated = false,
): IncompleteReason {
  if (sourceTruncated) {
    return "sourceTruncated";
  }
  if (issues.some((issue) => READ_FAILURE_PATTERN.test(issue.code))) {
    return "readFailure";
  }
  if (issues.some((issue) => FIELD_TRUNCATION_PATTERN.test(issue.code))) {
    return "fieldTruncation";
  }
  if (issues.some((issue) => CAPACITY_PATTERN.test(issue.code))) {
    return "capacity";
  }
  return "unknown";
}

export function incompleteReasonSummary(reason: IncompleteReason): string {
  switch (reason) {
    case "readFailure":
      return "读取或集合计数失败，当前结果不是完整快照。";
    case "capacity":
      return "已达到条目、时间或缓冲上限，当前结果不是完整快照。";
    case "fieldTruncation":
      return "部分字段超过长度上限并被截断；字段值不完整。";
    case "sourceTruncated":
      return "至少一个来源结果不完整，关联集可能缺少条目。";
    case "unknown":
      return "后端将结果标记为不完整，但没有提供可识别的原因。";
  }
}
