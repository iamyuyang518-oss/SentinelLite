import { formatCheckedAt } from "../lib/presentation";
import type { FileEvidence } from "../types";

export function FileEvidenceView({ evidence }: { evidence: FileEvidence }) {
  return (
    <div className="file-evidence">
      <dl className="fact-list fact-list--compact">
        <div>
          <dt>检查路径</dt>
          <dd className="breakable mono-text">
            {evidence.checkedPath ?? "未访问磁盘路径"}
          </dd>
        </div>
        <div>
          <dt>文件大小</dt>
          <dd>{evidence.sizeBytes === null ? "未知" : `${evidence.sizeBytes} 字节`}</dd>
        </div>
        <div>
          <dt>修改时间</dt>
          <dd>{evidence.modifiedAt ? formatCheckedAt(evidence.modifiedAt) : "未知"}</dd>
        </div>
        <div>
          <dt>SHA-256（磁盘文件）</dt>
          <dd className="breakable mono-text">
            {evidence.sha256 ?? "未计算或不可用"}
          </dd>
        </div>
      </dl>
      <p className="fine-print">{evidence.note}</p>
      <p className="fine-print">
        磁盘文件签名与哈希仅对应检查时的磁盘文件。内存映像未验证；磁盘证据不等于当前运行进程绝对可信。
      </p>
    </div>
  );
}
