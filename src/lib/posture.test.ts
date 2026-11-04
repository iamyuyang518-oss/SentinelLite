import { describe, expect, it } from "vitest";

import {
  combineReadState,
  deriveAssociationPosture,
  deriveProtectionPosture,
  derivePosture,
  derivePostureTriage,
} from "./posture";
import {
  makeAssetAssociationResult,
  makeAssetRelation,
  makeAssociationSource,
  makeNativeProtectionStatus,
  makeSoftwareAssociationSummary,
} from "../test/fixtures";
import type { AssetRelation, AssociationTargetKind } from "../types";

function relation(
  id: string,
  softwareId: string,
  kind: AssociationTargetKind,
  overrides: Partial<AssetRelation> = {},
): AssetRelation {
  return makeAssetRelation({
    id,
    softwareId,
    softwareName: softwareId,
    target: {
      kind,
      id: `${kind}:${id}`,
      name: `${kind}-${id}`,
      detail: "test target",
      path: "C:\\Program Files\\Fictional\\app.exe",
      observation: overrides.observation ?? "observed",
    },
    ...overrides,
  });
}

describe("security posture derivation", () => {
  it("counts exact, strong, weak and ambiguous relations without merging them", () => {
    const result = makeAssetAssociationResult({
      observation: "partial",
      sources: [
        makeAssociationSource({
          source: "softwareInventory",
          observation: "observed",
          itemCount: 5,
          addressableCount: 4,
          unresolvedPathCount: 1,
        }),
        makeAssociationSource({
          source: "processSnapshot",
          observation: "observed",
          itemCount: 2,
          addressableCount: 2,
        }),
        makeAssociationSource({
          source: "serviceInventory",
          observation: "partial",
          itemCount: 3,
          addressableCount: 2,
          unresolvedPathCount: 1,
        }),
        makeAssociationSource({
          source: "scheduledTaskInventory",
          observation: "observed",
          itemCount: 1,
          addressableCount: 1,
        }),
        makeAssociationSource({
          source: "startupInventory",
          observation: "error",
          itemCount: 0,
          addressableCount: 0,
        }),
      ],
      software: [
        makeSoftwareAssociationSummary({ softwareId: "linked", softwareName: "linked" }),
        makeSoftwareAssociationSummary({
          softwareId: "weak",
          softwareName: "weak",
          state: "weakOnly",
        }),
        makeSoftwareAssociationSummary({
          softwareId: "no-match",
          softwareName: "no-match",
          state: "noMatch",
          relationCount: 0,
          strongRelationCount: 0,
        }),
        makeSoftwareAssociationSummary({
          softwareId: "insufficient",
          softwareName: "insufficient",
          state: "insufficientEvidence",
          relationCount: 0,
          strongRelationCount: 0,
        }),
        makeSoftwareAssociationSummary({
          softwareId: "unavailable",
          softwareName: "unavailable",
          state: "sourceUnavailable",
          relationCount: 0,
          strongRelationCount: 0,
        }),
      ],
      relations: [
        relation("exact", "linked", "process", { certainty: "exact" }),
        relation("strong-service", "linked", "service", {
          certainty: "strong",
          matchMethod: "containedInInstallLocation",
        }),
        relation("weak-startup", "weak", "startupItem", {
          certainty: "weak",
          matchMethod: "sameFileNameDifferentDirectory",
          ambiguous: true,
          candidateSoftwareCount: 2,
        }),
        relation("partial-task", "linked", "scheduledTask", {
          certainty: "strong",
          observation: "partial",
        }),
      ],
    });

    const posture = deriveAssociationPosture(result, false);

    expect(posture.readState).toBe("partial");
    expect(posture.exactCount).toBe(1);
    expect(posture.strongCount).toBe(2);
    expect(posture.weakCount).toBe(1);
    expect(posture.ambiguousCount).toBe(1);
    expect(posture.partialRelationCount).toBe(1);
    expect(posture.processServiceRelations.map((entry) => entry.id)).toEqual([
      "exact",
      "strong-service",
    ]);
    expect(posture.persistentRelations.map((entry) => entry.id)).toEqual([
      "weak-startup",
      "partial-task",
    ]);
    expect(posture.weakOrAmbiguousRelations.map((entry) => entry.id)).toEqual([
      "weak-startup",
    ]);
    expect(posture.sourceObservedCount).toBe(3);
    expect(posture.sourcePartialCount).toBe(1);
    expect(posture.sourceUnavailableCount).toBe(1);
    expect(posture.unresolvedPathCount).toBe(2);
    expect(posture.softwareStateCounts).toEqual({
      linked: 1,
      weakOnly: 1,
      noMatch: 1,
      insufficientEvidence: 1,
      sourceUnavailable: 1,
    });
    expect(posture.linkedSoftwareRows.map((row) => row.softwareId)).toEqual([
      "linked",
      "weak",
    ]);
  });

  it("derives read coverage from source statuses, not only the aggregate label", () => {
    const unavailable = deriveAssociationPosture(
      makeAssetAssociationResult({
        observation: "observed",
        sources: [
          makeAssociationSource({
            source: "processSnapshot",
            observation: "error",
            itemCount: 0,
            addressableCount: 0,
          }),
        ],
      }),
      false,
    );
    expect(unavailable.readState).toBe("unavailable");

    const partial = deriveAssociationPosture(
      makeAssetAssociationResult({
        observation: "observed",
        sources: [
          makeAssociationSource({ source: "processSnapshot", observation: "observed" }),
          makeAssociationSource({
            source: "serviceInventory",
            observation: "partial",
            itemCount: 0,
            addressableCount: 0,
          }),
        ],
      }),
      false,
    );
    expect(partial.readState).toBe("partial");
  });

  it("keeps protection unknown, disabled and unavailable semantics separate", () => {
    const status = makeNativeProtectionStatus({
      observation: "partial",
      defender: {
        ...makeNativeProtectionStatus().defender,
        amServiceEnabled: false,
        signatureAgeDays: null,
      },
      firewall: {
        ...makeNativeProtectionStatus().firewall,
        state: "accessDenied",
        profiles: [
          { profile: "domain", enabled: null },
          { profile: "private", enabled: null },
          { profile: "public", enabled: null },
        ],
      },
    });

    const posture = deriveProtectionPosture(status, false);
    expect(posture.readState).toBe("partial");
    expect(posture.disabled.map((entry) => entry.id)).toEqual(["defender:am-service"]);
    expect(posture.unknown).toHaveLength(3);
    expect(posture.unknownFieldCount).toBe(4);
    expect(posture.unavailableProviderCount).toBe(1);
  });

  it("combines read coverage without creating a protected state", () => {
    expect(combineReadState("notRead", "notRead")).toBe("notRead");
    expect(combineReadState("complete", "complete")).toBe("complete");
    expect(combineReadState("unavailable", "unavailable")).toBe("unavailable");
    expect(combineReadState("complete", "partial")).toBe("partial");
    expect(combineReadState("complete", "notRead")).toBe("partial");
    expect(combineReadState("complete", "unavailable")).toBe("partial");
  });

  it("derives triage counts without turning noMatch into insufficient or unavailable", () => {
    const protection = deriveProtectionPosture(
      makeNativeProtectionStatus({
        defender: {
          ...makeNativeProtectionStatus().defender,
          amServiceEnabled: false,
        },
      }),
      false,
    );
    const association = deriveAssociationPosture(
      makeAssetAssociationResult({
        observation: "partial",
        software: [
          makeSoftwareAssociationSummary({ state: "linked" }),
          makeSoftwareAssociationSummary({
            softwareId: "no-match",
            softwareName: "no-match",
            state: "noMatch",
            relationCount: 0,
            strongRelationCount: 0,
          }),
          makeSoftwareAssociationSummary({
            softwareId: "insufficient",
            softwareName: "insufficient",
            state: "insufficientEvidence",
            relationCount: 0,
            strongRelationCount: 0,
          }),
          makeSoftwareAssociationSummary({
            softwareId: "unavailable",
            softwareName: "unavailable",
            state: "sourceUnavailable",
            relationCount: 0,
            strongRelationCount: 0,
          }),
        ],
        relations: [makeAssetRelation({ certainty: "exact" })],
        sources: [
          makeAssociationSource({ source: "processSnapshot", observation: "partial" }),
        ],
      }),
      false,
    );

    const triage = derivePostureTriage(protection, association);
    expect(triage.observed.exactRelations).toBe(1);
    expect(triage.attention.disabledProtectionSwitches).toBe(1);
    expect(triage.insufficient.insufficientSoftware).toBe(1);
    expect(triage.insufficient.partialSources).toBe(1);
    expect(triage.unavailable.unavailableSoftware).toBe(1);
    expect(triage.unavailable.unavailableSources).toBe(0);
  });

  it("marks an unread posture as notRead and a failed command as unavailable", () => {
    const unread = derivePosture(null, false, null, false);
    expect(unread.readState).toBe("notRead");
    expect(unread.association.relationCount).toBe(0);

    const failed = derivePosture(null, true, null, true);
    expect(failed.readState).toBe("unavailable");
    expect(failed.protection.readState).toBe("unavailable");
    expect(failed.association.readState).toBe("unavailable");
  });
});
