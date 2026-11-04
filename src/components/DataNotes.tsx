import { useId } from "react";

import type { DataIssue } from "../types";

interface DataNotesProps {
  limitations: string[];
  issues: DataIssue[];
}

export function DataNotes({ limitations, issues }: DataNotesProps) {
  const headingId = useId();

  return (
    <section className="data-notes" aria-labelledby={headingId}>
      <h3 id={headingId}>本次检查边界</h3>
      {limitations.length > 0 ? (
        <ul>
          {limitations.map((limitation) => (
            <li key={limitation}>{limitation}</li>
          ))}
        </ul>
      ) : (
        <p className="fine-print">后端没有提供限制说明，这本身属于信息缺口。</p>
      )}

      {issues.length > 0 ? (
        <div className="issue-list">
          <h4>读取问题</h4>
          <ul>
            {issues.map((issue) => (
              <li key={`${issue.code}:${issue.source}:${issue.message}`}>
                <code>{issue.code}</code>
                <span>{issue.message}</span>
                <span className="issue-source">{issue.source}</span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </section>
  );
}
