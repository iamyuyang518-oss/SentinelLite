interface IssueLike {
  code: string;
  message: string;
}

export function IssueNote({ issues }: { issues: IssueLike[] }) {
  const first = issues[0];
  if (first === undefined) {
    return null;
  }
  const label = issues.map((issue) => `${issue.code}: ${issue.message}`).join("；");
  return (
    <span className="cell-note cell-note--warning" title={label} aria-label={label}>
      {first.code}
      {issues.length > 1 ? ` +${issues.length - 1}` : ""}
    </span>
  );
}
