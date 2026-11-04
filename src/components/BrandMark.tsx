export function BrandMark({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 1024 1024"
      role="img"
      aria-label="SentinelLite"
      focusable="false"
    >
      <rect width="1024" height="1024" rx="232" fill="#17342A" />
      <rect
        x="56"
        y="56"
        width="912"
        height="912"
        rx="184"
        fill="none"
        stroke="#3A5D4D"
        strokeWidth="10"
      />
      <path
        d="M706 302H430c-72 0-130 58-130 130s58 130 130 130h164c72 0 130 58 130 130s-58 130-130 130H318"
        fill="none"
        stroke="#EEF3EC"
        strokeWidth="76"
        strokeLinecap="square"
      />
      <circle cx="710" cy="314" r="42" fill="#D4A03D" />
    </svg>
  );
}
