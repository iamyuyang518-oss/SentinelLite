import { describe, expect, it } from "vitest";

import {
  disabledProtectionSwitches,
  enabledProtectionSwitches,
  protectionReadState,
  protectionSwitches,
  unavailableProtectionProviderCount,
  unknownProtectionFieldCount,
  unknownProtectionSwitches,
} from "./nativeProtection";
import { makeNativeProtectionStatus } from "../test/fixtures";

describe("native protection posture helpers", () => {
  it("keeps enabled, disabled and unknown switches distinct", () => {
    const complete = makeNativeProtectionStatus();
    expect(protectionSwitches(complete)).toHaveLength(6);
    expect(enabledProtectionSwitches(complete)).toHaveLength(6);
    expect(disabledProtectionSwitches(complete)).toHaveLength(0);
    expect(unknownProtectionSwitches(complete)).toHaveLength(0);
    expect(protectionReadState(complete, false)).toBe("complete");

    const disabled = makeNativeProtectionStatus({
      defender: {
        ...complete.defender,
        amServiceEnabled: false,
      },
    });
    expect(disabledProtectionSwitches(disabled).map((entry) => entry.id)).toEqual([
      "defender:am-service",
    ]);
    expect(protectionReadState(disabled, false)).toBe("complete");

    const unknown = makeNativeProtectionStatus({
      observation: "partial",
      defender: {
        ...complete.defender,
        amServiceEnabled: null,
        signatureVersion: null,
      },
      firewall: {
        ...complete.firewall,
        profiles: [
          { profile: "domain", enabled: true },
          { profile: "private", enabled: null },
          { profile: "public", enabled: true },
        ],
      },
    });
    expect(unknownProtectionSwitches(unknown).map((entry) => entry.id)).toEqual([
      "defender:am-service",
      "firewall:private",
    ]);
    expect(disabledProtectionSwitches(unknown)).toHaveLength(0);
    expect(unknownProtectionFieldCount(unknown)).toBe(3);
    expect(protectionReadState(unknown, false)).toBe("partial");
  });

  it("never treats a missing read as complete or normal", () => {
    expect(protectionReadState(null, false)).toBe("notRead");
    expect(protectionReadState(null, true)).toBe("unavailable");

    const status = makeNativeProtectionStatus({
      observation: "unavailable",
      defender: {
        state: "accessDenied",
        antivirusEnabled: null,
        realTimeProtectionEnabled: null,
        amServiceEnabled: null,
        signatureVersion: null,
        signatureAgeDays: null,
        issues: [],
      },
      firewall: {
        state: "error",
        profiles: [
          { profile: "domain", enabled: null },
          { profile: "private", enabled: null },
          { profile: "public", enabled: null },
        ],
        issues: [],
      },
    });
    expect(protectionReadState(status, false)).toBe("unavailable");
    expect(unavailableProtectionProviderCount(status)).toBe(2);
  });

  it("does not count signature fields as known when the provider is unavailable", () => {
    const status = makeNativeProtectionStatus({
      observation: "unavailable",
      defender: {
        state: "unavailable",
        antivirusEnabled: null,
        realTimeProtectionEnabled: null,
        amServiceEnabled: null,
        signatureVersion: null,
        signatureAgeDays: null,
        issues: [],
      },
      firewall: {
        state: "unavailable",
        profiles: [
          { profile: "domain", enabled: null },
          { profile: "private", enabled: null },
          { profile: "public", enabled: null },
        ],
        issues: [],
      },
    });

    expect(disabledProtectionSwitches(status)).toHaveLength(0);
    expect(unknownProtectionSwitches(status)).toHaveLength(6);
    expect(unknownProtectionFieldCount(status)).toBe(8);
  });
});
