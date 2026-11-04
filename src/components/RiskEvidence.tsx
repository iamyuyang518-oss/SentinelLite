import { riskScopeLabel } from "../lib/presentation";
import type { Confidence, RiskAssessment } from "../types";
import { RiskBadge } from "./RiskBadge";

const CONFIDENCE_LABELS: Record<Confidence, string> = {
  high: "判断置信度：高",
  medium: "判断置信度：中",
  low: "判断置信度：低",
  unknown: "判断置信度：未知",
};

interface RiskEvidenceProps {
  risk: RiskAssessment;
}

export function RiskEvidence({ risk }: RiskEvidenceProps) {
  return (
    <section
      className="risk-evidence"
      aria-label={`${riskScopeLabel(risk.scope)}风险判断与证据`}
    >
      <div className="risk-evidence__heading">
        <RiskBadge level={risk.level} />
        <span className="risk-scope-label">{riskScopeLabel(risk.scope)}</span>
        <span className="confidence-note">{CONFIDENCE_LABELS[risk.confidence]}</span>
      </div>

      <p className="risk-evidence__summary">{risk.summary}</p>

      {risk.ruleIds.length > 0 ? (
        <div className="rule-line" aria-label="规则编号">
          {risk.ruleIds.map((ruleId) => (
            <code key={ruleId}>{ruleId}</code>
          ))}
        </div>
      ) : (
        <p className="fine-print">没有可展示的规则编号。</p>
      )}

      <div className="evidence-block">
        <h4>证据</h4>
        {risk.evidence.length > 0 ? (
          <ul>
            {risk.evidence.map((item) => (
              <li key={`${item.code}:${item.statement}`}>
                <code>{item.code}</code>
                <p>{item.statement}</p>
                <span>{item.source}</span>
              </li>
            ))}
          </ul>
        ) : (
          <p className="fine-print">没有捕获到可展示的证据。这不能作为安全结论。</p>
        )}
      </div>

      <div className="evidence-block">
        <h4>已知盲区</h4>
        <ul className="plain-list">
          {risk.blindSpots.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
      </div>

      <div className="evidence-block">
        <h4>建议</h4>
        <p>{risk.recommendation}</p>
      </div>
    </section>
  );
}
