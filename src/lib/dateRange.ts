const DATE_VALUE = /^(\d{4})-(\d{2})-(\d{2})$/;

/** Parses a local calendar date (`YYYY-MM-DD`) and rejects impossible dates. */
export function parseDateValue(value: string): Date | null {
  const match = DATE_VALUE.exec(value);
  if (match === null) {
    return null;
  }

  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(year, month - 1, day);

  if (
    date.getFullYear() !== year ||
    date.getMonth() !== month - 1 ||
    date.getDate() !== day
  ) {
    return null;
  }

  return date;
}

/**
 * Converts a selected calendar date to a local start/end boundary.
 * Full datetime strings remain accepted for compatibility with older form state.
 */
export function parseDateBoundary(value: string, endOfDay: boolean): Date | null {
  const date = parseDateValue(value);
  if (date === null) {
    const fallback = new Date(value);
    return Number.isNaN(fallback.getTime()) ? null : fallback;
  }

  if (endOfDay) {
    date.setHours(23, 59, 59, 999);
  } else {
    date.setHours(0, 0, 0, 0);
  }
  return date;
}

export function formatDateValue(year: number, monthIndex: number, day: number): string {
  return `${year}-${String(monthIndex + 1).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
}

export function formatDateLabel(value: string): string {
  const date = parseDateValue(value);
  if (date === null) {
    return value;
  }
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(date);
}
