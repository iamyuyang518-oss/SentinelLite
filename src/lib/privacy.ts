/**
 * Redacts a user-profile directory from a path shown in the UI.
 * The backend may still need the path transiently for association work; the
 * displayed form avoids exposing the account name unnecessarily.
 */
export function redactUserPath(value: string | null): string | null {
  if (value === null) {
    return null;
  }
  return value
    .replace(/^([A-Za-z]:\\Users\\)[^\\]+/i, "$1***")
    .replace(/^(\\\\\?\\[A-Za-z]:\\Users\\)[^\\]+/i, "$1***");
}

const KNOWN_ACCOUNTS = new Set([
  "SYSTEM",
  "LOCAL SYSTEM",
  "LOCALSYSTEM",
  "LOCAL SERVICE",
  "LOCALSERVICE",
  "NETWORK SERVICE",
  "NETWORKSERVICE",
  "TRUSTEDINSTALLER",
  "EVERYONE",
  "USERS",
  "ADMINISTRATORS",
  "AUTHENTICATED USERS",
  "INTERACTIVE",
  "SERVICE",
  "NETWORK",
  "BATCH",
  "LOCAL",
]);

const SYSTEM_ACCOUNT_DOMAINS = new Set([
  "NT AUTHORITY",
  "NT SERVICE",
  "BUILTIN",
  "WINDOW MANAGER",
  "FONT DRIVER HOST",
]);

/** Redacts author values that are shaped like a domain account, without hiding organization names. */
export function redactAuthorName(value: string | null): string | null {
  if (value === null) {
    return null;
  }
  if (value.includes(":\\")) {
    return redactUserPath(value);
  }
  return value.includes("\\") ? redactAccountName(value) : value;
}

/** Keeps system/group identities readable and hides personal account names. */
export function redactAccountName(value: string | null): string | null {
  if (value === null) {
    return null;
  }
  const trimmed = value.trim();
  if (trimmed.length === 0) {
    return trimmed;
  }
  const upper = trimmed.toUpperCase();
  if (KNOWN_ACCOUNTS.has(upper) || upper.startsWith("S-1-")) {
    return trimmed;
  }
  const separator = trimmed.lastIndexOf("\\");
  if (separator >= 0) {
    const domain = trimmed.slice(0, separator);
    const user = trimmed.slice(separator + 1);
    if (
      user.length === 0 ||
      SYSTEM_ACCOUNT_DOMAINS.has(domain.toUpperCase())
    ) {
      return trimmed;
    }
    return `${domain}\\***`;
  }
  return "***";
}
