import { getRiskPresentation } from "../lib/presentation";
import type { RiskLevel } from "../types";

interface RiskBadgeProps {
  level: RiskLevel;
}

export function RiskBadge({ level }: RiskBadgeProps) {
  const presentation = getRiskPresentation(level);

  return (
    <span className={`risk-badge risk-badge--${presentation.tone}`}>
      <span className="risk-badge__mark" aria-hidden="true" />
      {presentation.label}
    </span>
  );
}
