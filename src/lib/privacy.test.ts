import { describe, expect, it } from "vitest";

import { redactAccountName, redactAuthorName, redactUserPath } from "./privacy";

describe("redactUserPath", () => {
  it("hides the account segment while preserving the rest of the path", () => {
    expect(redactUserPath("C:\\Users\\Alice\\AppData\\Local\\App\\app.exe")).toBe(
      "C:\\Users\\***\\AppData\\Local\\App\\app.exe",
    );
  });

  it("handles a device-path prefix and leaves other paths unchanged", () => {
    expect(redactUserPath("\\\\?\\C:\\Users\\Alice\\AppData\\app.exe")).toBe(
      "\\\\?\\C:\\Users\\***\\AppData\\app.exe",
    );
    expect(redactUserPath("C:\\Program Files\\App\\app.exe")).toBe(
      "C:\\Program Files\\App\\app.exe",
    );
  });

  it("keeps null distinct from an empty value", () => {
    expect(redactUserPath(null)).toBeNull();
    expect(redactUserPath("")).toBe("");
  });

  it("keeps system accounts and redacts personal account components", () => {
    expect(redactAccountName("NT AUTHORITY\\SYSTEM")).toBe("NT AUTHORITY\\SYSTEM");
    expect(redactAccountName("NT SERVICE\\TrustedInstaller")).toBe(
      "NT SERVICE\\TrustedInstaller",
    );
    expect(redactAccountName("BUILTIN\\Administrators")).toBe(
      "BUILTIN\\Administrators",
    );
    expect(redactAccountName("CONTOSO\\alice")).toBe("CONTOSO\\***");
    expect(redactAccountName(".\\alice")).toBe(".\\***");
    expect(redactAccountName("alice")).toBe("***");
    expect(redactAccountName(null)).toBeNull();
  });

  it("redacts domain-shaped authors but keeps organization names", () => {
    expect(redactAuthorName("Microsoft Corporation")).toBe("Microsoft Corporation");
    expect(redactAuthorName("CONTOSO\\alice")).toBe("CONTOSO\\***");
    expect(redactAuthorName("C:\\Users\\Alice\\Author")).toBe(
      "C:\\Users\\***\\Author",
    );
  });
});
