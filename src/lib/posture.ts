import type {
  AssetAssociationResult,
  AssetRelation,
  DataIssue,
  NativeProtectionStatus,
  SoftwareAssociationState,
} from "../types";
import {
  disabledProtectionSwitches,
  enabledProtectionSwitches,
  protectionIssueCount,
  protectionReadState,
  protectionSwitches,
  unavailableProtectionProviderCount,
  unknownProtectionFieldCount,
  unknownProtectionSwitches,
  type ProtectionSwitch,
} from "./nativeProtection";

export type PostureReadState = "notRead" | "complete" | "partial" | "unavailable";

/**
 * A posture read state describes whether a source could be read. It is not a protection
 * or risk verdict; `complete` only means the sources in scope returned data.
 */
export interface ProtectionPosture {
  readState: PostureReadState;
  status: NativeProtectionStatus | null;
  switches: ProtectionSwitch[];
  enabled: ProtectionSwitch[];
  disabled: ProtectionSwitch[];
  unknown: ProtectionSwitch[];
  unknownFieldCount: number;
  unavailableProviderCount: number;
  issueCount: number;
}

export interface SoftwareAssociationRow {
  softwareId: string;
  name: string;
  state: SoftwareAssociationState;
  exact: number;
  strong: number;
  weak: number;
  total: number;
  process: number;
  service: number;
  scheduledTask: number;
  startupItem: number;
  issues: DataIssue[];
}

export interface AssociationPosture {
  readState: PostureReadState;
  result: AssetAssociationResult | null;
  exactCount: number;
  strongCount: number;
  weakCount: number;
  ambiguousCount: number;
  partialRelationCount: number;
  relationCount: number;
  processServiceRelations: AssetRelation[];
  persistentRelations: AssetRelation[];
  weakOrAmbiguousRelations: AssetRelation[];
  sourceObservedCount: number;
  sourcePartialCount: number;
  sourceUnavailableCount: number;
  unresolvedPathCount: number;
  softwareStateCounts: Record<SoftwareAssociationState, number>;
  softwareRows: SoftwareAssociationRow[];
  linkedSoftwareRows: SoftwareAssociationRow[];
  issues: DataIssue[];
  truncated: boolean;
}

export interface PostureTriage {
  observed: {
    exactRelations: number;
    strongRelations: number;
    enabledProtectionSwitches: number;
    observedSources: number;
  };
  attention: {
    weakRelations: number;
    ambiguousRelations: number;
    disabledProtectionSwitches: number;
  };
  insufficient: {
    insufficientSoftware: number;
    unknownProtectionFields: number;
    partialSources: number;
    partialRelations: number;
  };
  unavailable: {
    unavailableSoftware: number;
    unavailableSources: number;
    unavailableProtectionProviders: number;
  };
}

export interface PostureSummary {
  readState: PostureReadState;
  protection: ProtectionPosture;
  association: AssociationPosture;
  triage: PostureTriage;
}

const SOFTWARE_STATE_ORDER: Record<SoftwareAssociationState, number> = {
  linked: 0,
  weakOnly: 1,
  insufficientEvidence: 2,
  noMatch: 3,
  sourceUnavailable: 4,
};

function emptySoftwareStateCounts(): Record<SoftwareAssociationState, number> {
  return {
    linked: 0,
    weakOnly: 0,
    noMatch: 0,
    insufficientEvidence: 0,
    sourceUnavailable: 0,
  };
}

export function deriveProtectionPosture(
  status: NativeProtectionStatus | null,
  failed: boolean,
): ProtectionPosture {
  const switches = protectionSwitches(status);
  return {
    readState: protectionReadState(status, failed),
    status,
    switches,
    enabled: enabledProtectionSwitches(status),
    disabled: disabledProtectionSwitches(status),
    unknown: unknownProtectionSwitches(status),
    unknownFieldCount: unknownProtectionFieldCount(status),
    unavailableProviderCount: unavailableProtectionProviderCount(status),
    issueCount: protectionIssueCount(status),
  };
}

function softwareRowsFor(result: AssetAssociationResult): SoftwareAssociationRow[] {
  const relationsBySoftware = new Map<string, AssetRelation[]>();
  for (const relation of result.relations) {
    const existing = relationsBySoftware.get(relation.softwareId) ?? [];
    existing.push(relation);
    relationsBySoftware.set(relation.softwareId, existing);
  }

  return result.software
    .map((software) => {
      const relations = relationsBySoftware.get(software.softwareId) ?? [];
      const row: SoftwareAssociationRow = {
        softwareId: software.softwareId,
        name: software.softwareName,
        state: software.state,
        exact: 0,
        strong: 0,
        weak: 0,
        total: relations.length,
        process: 0,
        service: 0,
        scheduledTask: 0,
        startupItem: 0,
        issues: software.issues,
      };
      for (const relation of relations) {
        if (relation.certainty === "exact") {
          row.exact += 1;
        } else if (relation.certainty === "strong") {
          row.strong += 1;
        } else {
          row.weak += 1;
        }
        if (relation.target.kind === "process") {
          row.process += 1;
        } else if (relation.target.kind === "service") {
          row.service += 1;
        } else if (relation.target.kind === "scheduledTask") {
          row.scheduledTask += 1;
        } else {
          row.startupItem += 1;
        }
      }
      return row;
    })
    .sort(
      (left, right) =>
        SOFTWARE_STATE_ORDER[left.state] - SOFTWARE_STATE_ORDER[right.state] ||
        right.exact + right.strong - (left.exact + left.strong) ||
        right.weak - left.weak ||
        left.name.localeCompare(right.name, "zh-CN"),
    );
}

export function deriveAssociationPosture(
  result: AssetAssociationResult | null,
  failed: boolean,
): AssociationPosture {
  if (result === null) {
    return {
      readState: failed ? "unavailable" : "notRead",
      result: null,
      exactCount: 0,
      strongCount: 0,
      weakCount: 0,
      ambiguousCount: 0,
      partialRelationCount: 0,
      relationCount: 0,
      processServiceRelations: [],
      persistentRelations: [],
      weakOrAmbiguousRelations: [],
      sourceObservedCount: 0,
      sourcePartialCount: 0,
      sourceUnavailableCount: 0,
      unresolvedPathCount: 0,
      softwareStateCounts: emptySoftwareStateCounts(),
      softwareRows: [],
      linkedSoftwareRows: [],
      issues: [],
      truncated: false,
    };
  }

  const softwareStateCounts = emptySoftwareStateCounts();
  for (const software of result.software) {
    softwareStateCounts[software.state] += 1;
  }

  const softwareRows = softwareRowsFor(result);
  const sourceObservedCount = result.sources.filter(
    (source) => source.observation === "observed" && !source.truncated,
  ).length;
  const sourcePartialCount = result.sources.filter(
    (source) => source.observation === "partial" || source.truncated,
  ).length;
  const sourceUnavailableCount =
    result.sources.length - sourceObservedCount - sourcePartialCount;
  const readState =
    result.observation === "unavailable" ||
    result.observation === "accessDenied" ||
    result.observation === "error"
      ? "unavailable"
      : result.observation === "partial" || result.truncated
        ? "partial"
        : result.sources.length > 0 && sourceObservedCount === 0 && sourcePartialCount === 0
          ? "unavailable"
          : sourceObservedCount === result.sources.length && result.sources.length > 0
            ? "complete"
            : "partial";

  return {
    readState,
    result,
    exactCount: result.relations.filter((relation) => relation.certainty === "exact").length,
    strongCount: result.relations.filter((relation) => relation.certainty === "strong").length,
    weakCount: result.relations.filter((relation) => relation.certainty === "weak").length,
    ambiguousCount: result.relations.filter((relation) => relation.ambiguous).length,
    partialRelationCount: result.relations.filter(
      (relation) => relation.observation !== "observed",
    ).length,
    relationCount: result.relations.length,
    processServiceRelations: result.relations.filter(
      (relation) =>
        relation.target.kind === "process" || relation.target.kind === "service",
    ),
    persistentRelations: result.relations.filter(
      (relation) =>
        relation.target.kind === "scheduledTask" ||
        relation.target.kind === "startupItem",
    ),
    weakOrAmbiguousRelations: result.relations.filter(
      (relation) => relation.certainty === "weak" || relation.ambiguous,
    ),
    sourceObservedCount,
    sourcePartialCount,
    sourceUnavailableCount,
    unresolvedPathCount: result.sources.reduce(
      (total, source) => total + source.unresolvedPathCount,
      0,
    ),
    softwareStateCounts,
    softwareRows,
    linkedSoftwareRows: softwareRows.filter(
      (row) => row.state === "linked" || row.state === "weakOnly",
    ),
    issues: result.issues,
    truncated: result.truncated,
  };
}

export function combineReadState(
  protection: PostureReadState,
  association: PostureReadState,
): PostureReadState {
  if (protection === "notRead" && association === "notRead") {
    return "notRead";
  }
  if (protection === "unavailable" && association === "unavailable") {
    return "unavailable";
  }
  if (protection === "complete" && association === "complete") {
    return "complete";
  }
  return "partial";
}

export function derivePostureTriage(
  protection: ProtectionPosture,
  association: AssociationPosture,
): PostureTriage {
  return {
    observed: {
      exactRelations: association.exactCount,
      strongRelations: association.strongCount,
      enabledProtectionSwitches: protection.enabled.length,
      observedSources: association.sourceObservedCount,
    },
    attention: {
      weakRelations: association.weakCount,
      ambiguousRelations: association.ambiguousCount,
      disabledProtectionSwitches: protection.disabled.length,
    },
    insufficient: {
      insufficientSoftware: association.softwareStateCounts.insufficientEvidence,
      unknownProtectionFields: protection.unknownFieldCount,
      partialSources: association.sourcePartialCount,
      partialRelations: association.partialRelationCount,
    },
    unavailable: {
      unavailableSoftware: association.softwareStateCounts.sourceUnavailable,
      unavailableSources: association.sourceUnavailableCount,
      unavailableProtectionProviders: protection.unavailableProviderCount,
    },
  };
}

export function derivePosture(
  protectionStatus: NativeProtectionStatus | null,
  protectionFailed: boolean,
  associationResult: AssetAssociationResult | null,
  associationFailed: boolean,
): PostureSummary {
  const protection = deriveProtectionPosture(protectionStatus, protectionFailed);
  const association = deriveAssociationPosture(associationResult, associationFailed);
  return {
    readState: combineReadState(protection.readState, association.readState),
    protection,
    association,
    triage: derivePostureTriage(protection, association),
  };
}
