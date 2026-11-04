//! Deterministic, read-only associations between installed software and the
//! inventory objects that can be compared without executing anything.
//!
//! The engine consumes snapshots produced by the existing Phase 2/3 scanners. It
//! never enumerates registry keys, opens a service, runs a task, launches a
//! process, or reads a command line of its own. All matching is lexical path
//! matching; no filesystem, symlink, 8.3-name, or environment expansion is
//! performed here.

use std::collections::{HashMap, HashSet};

use crate::limits::{
    MAX_ASSOCIATION_CANDIDATES_PER_TARGET, MAX_ASSOCIATION_EVIDENCE_CHARS, MAX_ASSOCIATION_ISSUES,
    MAX_ASSOCIATION_RELATIONS, MAX_ASSOCIATION_RELATIONS_PER_SOFTWARE,
    MAX_ASSOCIATION_WEAK_RELATIONS, bound_text,
};
use crate::model::{
    AssetAssociationResult, AssetRelation, AssociationCertainty, AssociationEvidenceType,
    AssociationMatchMethod, AssociationRelationType, AssociationSource, AssociationSourceStatus,
    AssociationTarget, AssociationTargetKind, DataIssue, InstalledSoftware, InventoryObservation,
    ProcessListResult, ScheduledTaskInventoryResult, ScheduledTaskSummary, ServiceInventoryResult,
    ServiceSummary, SoftwareAssociationState, SoftwareAssociationSummary, SoftwareClassification,
    SoftwareScanResult, StartupItem, StartupScanResult,
};
use crate::redaction::redact_user_profile_path;
use crate::util::{now_rfc3339, stable_token};

const UNREADABLE_PATH_CODE: &str = "ASSOCIATION-PATH-NOT-COMPARABLE";
const WEAK_FILE_NAME_CODE: &str = "ASSOCIATION-WEAK-FILENAME-ONLY";
const MULTIPLE_CANDIDATES_CODE: &str = "ASSOCIATION-MULTIPLE-SOFTWARE-CANDIDATES";
const INCOMPLETE_SOURCE_CODE: &str = "ASSOCIATION-SOURCE-INCOMPLETE";

/// Inputs to the pure association engine. The scanner wrapper supplies these
/// from the existing read-only inventory commands.
pub struct AssociationSnapshots<'a> {
    pub software: &'a SoftwareScanResult,
    pub processes: Option<&'a ProcessListResult>,
    pub process_error: bool,
    pub services: &'a ServiceInventoryResult,
    pub tasks: &'a ScheduledTaskInventoryResult,
    pub startup: &'a StartupScanResult,
}

/// Performs one read-only association pass over five existing inventory snapshots.
///
/// This function does not add a scanner. Each snapshot is produced by the
/// corresponding Phase 2/3 read-only implementation. A process snapshot failure
/// is represented as a source error rather than as an empty process list.
pub fn scan_asset_associations() -> AssetAssociationResult {
    let checked_at = now_rfc3339();
    let software = crate::software::scan_installed_software();
    let startup = crate::startup::scan_startup_items();
    let services = crate::services::list_services();
    let tasks = crate::scheduled_tasks::list_scheduled_tasks();
    let process_result = crate::process::list_processes();
    let (processes, process_error) = match process_result {
        Ok(result) => (Some(result), false),
        Err(_) => (None, true),
    };

    associate_snapshots(
        checked_at,
        &AssociationSnapshots {
            software: &software,
            processes: processes.as_ref(),
            process_error,
            services: &services,
            tasks: &tasks,
            startup: &startup,
        },
    )
}

/// Pure association step used by tests and by the command wrapper.
pub fn associate_snapshots(
    checked_at: String,
    snapshots: &AssociationSnapshots<'_>,
) -> AssetAssociationResult {
    let mut software_descriptors = Vec::with_capacity(snapshots.software.items.len());
    let mut broad_root_count = 0usize;
    let mut invalid_software_path_count = 0usize;
    let mut no_static_path_count = 0usize;

    for item in &snapshots.software.items {
        let descriptor = describe_software(
            item,
            &mut broad_root_count,
            &mut invalid_software_path_count,
            &mut no_static_path_count,
        );
        software_descriptors.push(descriptor);
    }

    let (targets, mut sources) = build_target_sources(snapshots);
    sources.insert(
        0,
        software_source_status(snapshots.software, &software_descriptors),
    );

    let mut candidates = collect_candidates(&software_descriptors, &targets);
    let target_candidate_counts = candidate_counts_by_target(&candidates);
    let mut truncated = sources.iter().any(|source| source.truncated);
    let mut dropped_candidates = false;

    // Prefer exact evidence, then strong containment, then filename-only clues.
    candidates.sort_by(|left, right| {
        certainty_rank(left.certainty)
            .cmp(&certainty_rank(right.certainty))
            .then_with(|| {
                software_descriptors[left.software_index]
                    .name
                    .to_lowercase()
                    .cmp(
                        &software_descriptors[right.software_index]
                            .name
                            .to_lowercase(),
                    )
            })
            .then_with(|| {
                target_kind_rank(targets[left.target_index].kind)
                    .cmp(&target_kind_rank(targets[right.target_index].kind))
            })
            .then_with(|| {
                targets[left.target_index]
                    .name
                    .cmp(&targets[right.target_index].name)
            })
            .then_with(|| left.evidence_value.cmp(&right.evidence_value))
            .then_with(|| {
                software_descriptors[left.software_index]
                    .id
                    .cmp(&software_descriptors[right.software_index].id)
            })
    });

    let mut per_target_candidates: HashMap<usize, usize> = HashMap::new();
    let mut per_software_candidates: HashMap<String, usize> = HashMap::new();
    let mut weak_count = 0usize;
    let mut retained = Vec::new();

    for candidate in candidates {
        let target_count = per_target_candidates
            .entry(candidate.target_index)
            .or_insert(0);
        if *target_count >= MAX_ASSOCIATION_CANDIDATES_PER_TARGET {
            dropped_candidates = true;
            continue;
        }
        if candidate.certainty == AssociationCertainty::Weak {
            if weak_count >= MAX_ASSOCIATION_WEAK_RELATIONS {
                dropped_candidates = true;
                continue;
            }
            weak_count += 1;
        }
        let software_id = &software_descriptors[candidate.software_index].id;
        let software_count = per_software_candidates
            .entry(software_id.clone())
            .or_insert(0);
        if *software_count >= MAX_ASSOCIATION_RELATIONS_PER_SOFTWARE {
            dropped_candidates = true;
            continue;
        }
        if retained.len() >= MAX_ASSOCIATION_RELATIONS {
            dropped_candidates = true;
            break;
        }
        *target_count += 1;
        *software_count += 1;
        retained.push(candidate);
    }

    if dropped_candidates {
        truncated = true;
    }

    let mut relations = Vec::with_capacity(retained.len());
    for candidate in retained {
        let descriptor = &software_descriptors[candidate.software_index];
        let target = &targets[candidate.target_index];
        let candidate_count = target_candidate_counts
            .get(&candidate.target_index)
            .copied()
            .unwrap_or(1)
            .max(1);
        relations.push(build_relation(
            descriptor,
            target,
            candidate_count,
            &candidate,
        ));
    }

    relations.sort_by(|left, right| {
        left.software_name
            .to_lowercase()
            .cmp(&right.software_name.to_lowercase())
            .then_with(|| {
                target_kind_rank(left.target.kind).cmp(&target_kind_rank(right.target.kind))
            })
            .then_with(|| left.target.name.cmp(&right.target.name))
            .then_with(|| left.id.cmp(&right.id))
    });

    let relation_counts = count_relations(&relations);
    let target_sources: Vec<&AssociationSourceStatus> = sources.iter().skip(1).collect();
    let any_target_source_readable = target_sources.iter().any(|source| {
        matches!(
            source.observation,
            InventoryObservation::Observed | InventoryObservation::Partial
        )
    });
    let all_target_sources_observed = target_sources
        .iter()
        .all(|source| source.observation == InventoryObservation::Observed && !source.truncated);
    let all_sources_observed = sources
        .iter()
        .all(|source| source.observation == InventoryObservation::Observed && !source.truncated);

    let software = software_descriptors
        .iter()
        .map(|descriptor| {
            let (strong, weak) = relation_counts
                .get(&descriptor.id)
                .copied()
                .unwrap_or((0, 0));
            let relation_count = strong + weak;
            let mut issues = descriptor.issues.clone();

            if descriptor.clues.is_empty() {
                push_issue_unique(
                    &mut issues,
                    association_issue(
                        "ASSOCIATION-NO-STATIC-PATH",
                        "没有可用于路径比较的安装位置或 DisplayIcon 路径；这不是“没有关联”，而是证据不足。",
                        &descriptor.id,
                    ),
                );
            }
            if descriptor.observation != InventoryObservation::Observed {
                push_issue_unique(
                    &mut issues,
                    association_issue(
                        INCOMPLETE_SOURCE_CODE,
                        "软件条目本身为部分读取；关联结论不能高于其字段证据。",
                        &descriptor.id,
                    ),
                );
            }
            if relation_count == 0 && (!any_target_source_readable || !all_target_sources_observed) {
                push_issue_unique(
                    &mut issues,
                    association_issue(
                        INCOMPLETE_SOURCE_CODE,
                        "至少一个关联来源不可用、部分读取或发生截断；不能据此断言不存在关联。",
                        &descriptor.id,
                    ),
                );
            }

            let state = if strong > 0 {
                SoftwareAssociationState::Linked
            } else if weak > 0 {
                SoftwareAssociationState::WeakOnly
            } else if !any_target_source_readable {
                SoftwareAssociationState::SourceUnavailable
            } else if descriptor.clues.is_empty()
                || descriptor.observation != InventoryObservation::Observed
                || !all_target_sources_observed
                || truncated
            {
                SoftwareAssociationState::InsufficientEvidence
            } else {
                SoftwareAssociationState::NoMatch
            };

            SoftwareAssociationSummary {
                software_id: descriptor.id.clone(),
                software_name: descriptor.name.clone(),
                state,
                path_evidence_count: descriptor.clues.len(),
                relation_count,
                strong_relation_count: strong,
                weak_relation_count: weak,
                issues,
            }
        })
        .collect();

    let mut issues = Vec::new();
    for source in &sources {
        if source.unresolved_path_count > 0 {
            issues.push(association_issue(
                UNREADABLE_PATH_CODE,
                format!(
                    "{} 中有 {} 个条目没有可比较的本地绝对路径；它们未参与关联，也没有被当作“不存在”。",
                    source_label(source.source),
                    source.unresolved_path_count
                ),
                source.id.as_str(),
            ));
        }
        if source.observation != InventoryObservation::Observed {
            issues.push(association_issue(
                INCOMPLETE_SOURCE_CODE,
                format!(
                    "{} 为“{}”；其关联结果可能不完整。",
                    source_label(source.source),
                    observation_label(source.observation)
                ),
                source.id.as_str(),
            ));
        }
        if source.truncated {
            truncated = true;
        }
    }
    if broad_root_count > 0 {
        issues.push(association_issue(
            "ASSOCIATION-BROAD-INSTALL-LOCATION",
            format!(
                "{broad_root_count} 个软件的安装位置过于宽泛（例如磁盘根、系统目录或用户配置根），未用作归属证据。"
            ),
            "software",
        ));
    }
    if invalid_software_path_count > 0 {
        issues.push(association_issue(
            "ASSOCIATION-INVALID-INSTALL-PATH",
            format!(
                "{invalid_software_path_count} 个软件的安装位置或图标路径不是可比较的本地绝对路径，未参与关联。"
            ),
            "software",
        ));
    }
    if no_static_path_count > 0 {
        issues.push(association_issue(
            "ASSOCIATION-NO-STATIC-PATH",
            format!("{no_static_path_count} 个软件没有可用的静态路径证据；其关联状态为证据不足。"),
            "software",
        ));
    }
    if target_candidate_counts.values().any(|count| *count > 1) {
        let ambiguous_targets = target_candidate_counts
            .values()
            .filter(|count| **count > 1)
            .count();
        issues.push(association_issue(
            MULTIPLE_CANDIDATES_CODE,
            format!(
                "{ambiguous_targets} 个目标同时匹配多个软件候选；保留全部候选，不强行合并归属。"
            ),
            "association",
        ));
    }
    if dropped_candidates {
        issues.push(association_issue(
            "ASSOCIATION-TRUNCATED",
            "关联结果达到安全上限；只保留优先级最高的关系，结果不是完整关联集。",
            "association",
        ));
    }
    issues.truncate(MAX_ASSOCIATION_ISSUES);

    let observation = aggregate_observation(
        &sources,
        any_target_source_readable,
        all_sources_observed,
        truncated,
    );

    AssetAssociationResult {
        checked_at,
        observation,
        sources,
        software,
        relations,
        issues,
        limitations: association_limitations(),
        truncated,
    }
}

#[derive(Clone, Debug)]
struct NormalizedLocalPath {
    key: String,
    display: String,
}

#[derive(Clone, Debug)]
struct SoftwarePathClue {
    path: NormalizedLocalPath,
    evidence_type: AssociationEvidenceType,
    is_file: bool,
}

#[derive(Clone, Debug)]
struct SoftwareDescriptor {
    id: String,
    name: String,
    classification: SoftwareClassification,
    observation: InventoryObservation,
    clues: Vec<SoftwarePathClue>,
    file_name: Option<String>,
    issues: Vec<DataIssue>,
}

#[derive(Clone, Debug)]
struct TargetAsset {
    kind: AssociationTargetKind,
    id: String,
    name: String,
    detail: String,
    path: NormalizedLocalPath,
    file_name: Option<String>,
    observation: InventoryObservation,
    issues: Vec<DataIssue>,
}

#[derive(Clone, Debug)]
struct PathMatch {
    evidence_type: AssociationEvidenceType,
    match_method: AssociationMatchMethod,
    certainty: AssociationCertainty,
    evidence_value: String,
    software_evidence_path: Option<String>,
}

#[derive(Clone, Debug)]
struct Candidate {
    software_index: usize,
    target_index: usize,
    evidence_type: AssociationEvidenceType,
    match_method: AssociationMatchMethod,
    certainty: AssociationCertainty,
    evidence_value: String,
    software_evidence_path: Option<String>,
    issues: Vec<DataIssue>,
}

fn build_target_sources(
    snapshots: &AssociationSnapshots<'_>,
) -> (Vec<TargetAsset>, Vec<AssociationSourceStatus>) {
    let (mut targets, process_source) =
        process_targets(snapshots.processes, snapshots.process_error);
    let (service_targets, service_source) = service_targets(snapshots.services);
    let (task_targets, task_source) = task_targets(snapshots.tasks);
    let (startup_targets, startup_source) = startup_targets(snapshots.startup);
    targets.extend(service_targets);
    targets.extend(task_targets);
    targets.extend(startup_targets);
    (
        targets,
        vec![process_source, service_source, task_source, startup_source],
    )
}

fn process_targets(
    processes: Option<&ProcessListResult>,
    process_error: bool,
) -> (Vec<TargetAsset>, AssociationSourceStatus) {
    let mut targets = Vec::new();
    let mut source = AssociationSourceStatus {
        id: "processes:toolhelp32".to_string(),
        source: AssociationSource::ProcessSnapshot,
        observation: if process_error {
            InventoryObservation::Error
        } else {
            InventoryObservation::Observed
        },
        item_count: 0,
        addressable_count: 0,
        unresolved_path_count: 0,
        truncated: false,
    };
    let Some(result) = processes else {
        return (targets, source);
    };

    source.item_count = result.processes.len();
    source.truncated = result.truncated;
    let mut saw_unavailable_path = false;
    for process in &result.processes {
        let Some(raw_path) = process.executable_path.as_deref() else {
            source.unresolved_path_count += 1;
            saw_unavailable_path = true;
            continue;
        };
        if process.path_access != crate::model::ProcessPathAccess::Available {
            source.unresolved_path_count += 1;
            saw_unavailable_path = true;
            continue;
        }
        let Some(path) = normalize_local_path(raw_path) else {
            source.unresolved_path_count += 1;
            continue;
        };

        let (id, mut issues, observation) = match process.identity.as_deref() {
            Some(identity) if !identity.is_empty() => (
                format!("process:{}:{}", process.pid, identity),
                Vec::new(),
                InventoryObservation::Observed,
            ),
            _ => (
                format!("process:{}:identity-unavailable", process.pid),
                vec![association_issue(
                    "ASSOCIATION-PROCESS-IDENTITY-UNKNOWN",
                    "进程没有可核验的身份令牌；PID 可能被复用，关联目标只能作为部分证据。",
                    "process snapshot",
                )],
                InventoryObservation::Partial,
            ),
        };
        if process.name.trim().is_empty() {
            push_issue_unique(
                &mut issues,
                association_issue(
                    "ASSOCIATION-PROCESS-NAME-MISSING",
                    "进程名称为空；目标仍保留路径证据。",
                    "process snapshot",
                ),
            );
        }
        targets.push(TargetAsset {
            kind: AssociationTargetKind::Process,
            id,
            name: if process.name.trim().is_empty() {
                "未命名进程".to_string()
            } else {
                process.name.clone()
            },
            detail: format!("PID {}", process.pid),
            file_name: path_file_name(&path.key),
            path,
            observation,
            issues,
        });
        source.addressable_count += 1;
    }

    source.observation = if result.truncated
        || saw_unavailable_path
        || targets
            .iter()
            .any(|target| target.observation != InventoryObservation::Observed)
    {
        InventoryObservation::Partial
    } else if result.processes.is_empty() {
        InventoryObservation::Unavailable
    } else {
        InventoryObservation::Observed
    };
    (targets, source)
}

fn service_targets(
    services: &ServiceInventoryResult,
) -> (Vec<TargetAsset>, AssociationSourceStatus) {
    let mut targets = Vec::new();
    let mut source = AssociationSourceStatus {
        id: "services:scm".to_string(),
        source: AssociationSource::ServiceInventory,
        observation: services.observation,
        item_count: services.items.len(),
        addressable_count: 0,
        unresolved_path_count: 0,
        truncated: services.truncated,
    };
    for service in &services.items {
        push_service_target(service, &mut targets, &mut source);
    }
    if source.observation == InventoryObservation::Observed
        && services
            .items
            .iter()
            .any(|service| service.observation != InventoryObservation::Observed)
    {
        source.observation = InventoryObservation::Partial;
    }
    (targets, source)
}

fn push_service_target(
    service: &ServiceSummary,
    targets: &mut Vec<TargetAsset>,
    source: &mut AssociationSourceStatus,
) {
    let Some(raw_path) = service.executable_path.as_deref() else {
        source.unresolved_path_count += 1;
        return;
    };
    let Some(path) = normalize_local_path(raw_path) else {
        source.unresolved_path_count += 1;
        return;
    };
    let mut issues = Vec::new();
    if service.observation != InventoryObservation::Observed {
        push_issue_unique(
            &mut issues,
            association_issue(
                INCOMPLETE_SOURCE_CODE,
                "服务配置为部分读取；服务二进制路径仍作为单独证据使用。",
                "SCM",
            ),
        );
    }
    targets.push(TargetAsset {
        kind: AssociationTargetKind::Service,
        id: service.id.clone(),
        name: service.display_name.clone(),
        detail: format!("服务名 {}", service.name),
        file_name: path_file_name(&path.key),
        path,
        observation: service.observation,
        issues,
    });
    source.addressable_count += 1;
}

fn task_targets(
    tasks: &ScheduledTaskInventoryResult,
) -> (Vec<TargetAsset>, AssociationSourceStatus) {
    let mut targets = Vec::new();
    let mut source = AssociationSourceStatus {
        id: "scheduled-tasks:task-scheduler".to_string(),
        source: AssociationSource::ScheduledTaskInventory,
        observation: tasks.observation,
        item_count: tasks.items.len(),
        addressable_count: 0,
        unresolved_path_count: 0,
        truncated: tasks.truncated,
    };
    for task in &tasks.items {
        push_task_targets(task, &mut targets, &mut source);
    }
    if source.observation == InventoryObservation::Observed
        && tasks
            .items
            .iter()
            .any(|task| task.observation != InventoryObservation::Observed)
    {
        source.observation = InventoryObservation::Partial;
    }
    (targets, source)
}

fn push_task_targets(
    task: &ScheduledTaskSummary,
    targets: &mut Vec<TargetAsset>,
    source: &mut AssociationSourceStatus,
) {
    for (index, action) in task.actions.iter().enumerate() {
        let Some(raw_path) = action.executable_path.as_deref() else {
            source.unresolved_path_count += 1;
            continue;
        };
        let Some(path) = normalize_local_path(raw_path) else {
            source.unresolved_path_count += 1;
            continue;
        };
        let mut issues = Vec::new();
        if task.observation != InventoryObservation::Observed {
            push_issue_unique(
                &mut issues,
                association_issue(
                    INCOMPLETE_SOURCE_CODE,
                    "任务定义为部分读取；动作路径仍作为单独证据使用。",
                    "Task Scheduler",
                ),
            );
        }
        targets.push(TargetAsset {
            kind: AssociationTargetKind::ScheduledTask,
            id: format!("{}:action:{}", task.id, index),
            name: task.name.clone(),
            detail: format!("任务动作 {}", index + 1),
            file_name: path_file_name(&path.key),
            path,
            observation: task.observation,
            issues,
        });
        source.addressable_count += 1;
    }
    if task.actions.is_empty() {
        source.unresolved_path_count += 1;
    }
}

fn startup_targets(startup: &StartupScanResult) -> (Vec<TargetAsset>, AssociationSourceStatus) {
    let mut targets = Vec::new();
    let mut source = AssociationSourceStatus {
        id: "startup:run-and-folders".to_string(),
        source: AssociationSource::StartupInventory,
        observation: startup.observation,
        item_count: startup.items.len(),
        addressable_count: 0,
        unresolved_path_count: 0,
        truncated: startup.truncated,
    };
    for item in &startup.items {
        push_startup_target(item, &mut targets, &mut source);
    }
    if source.observation == InventoryObservation::Observed
        && startup
            .items
            .iter()
            .any(|item| item.observation != InventoryObservation::Observed)
    {
        source.observation = InventoryObservation::Partial;
    }
    (targets, source)
}

fn push_startup_target(
    item: &StartupItem,
    targets: &mut Vec<TargetAsset>,
    source: &mut AssociationSourceStatus,
) {
    let Some(raw_path) = item.target_path.as_deref() else {
        source.unresolved_path_count += 1;
        return;
    };
    let Some(path) = normalize_local_path(raw_path) else {
        source.unresolved_path_count += 1;
        return;
    };
    let mut issues = Vec::new();
    if item.observation != InventoryObservation::Observed {
        push_issue_unique(
            &mut issues,
            association_issue(
                INCOMPLETE_SOURCE_CODE,
                "启动项为部分读取；目标路径仍作为单独证据使用。",
                "startup inventory",
            ),
        );
    }
    targets.push(TargetAsset {
        kind: AssociationTargetKind::StartupItem,
        id: item.id.clone(),
        name: item.name.clone(),
        detail: format!("启动项来源：{}", startup_source_label(item)),
        file_name: path_file_name(&path.key),
        path,
        observation: item.observation,
        issues,
    });
    source.addressable_count += 1;
}

fn software_source_status(
    software: &SoftwareScanResult,
    descriptors: &[SoftwareDescriptor],
) -> AssociationSourceStatus {
    let addressable_count = descriptors
        .iter()
        .filter(|descriptor| !descriptor.clues.is_empty())
        .count();
    AssociationSourceStatus {
        id: "software:uninstall".to_string(),
        source: AssociationSource::SoftwareInventory,
        observation: software.observation,
        item_count: software.items.len(),
        addressable_count,
        unresolved_path_count: software.items.len().saturating_sub(addressable_count),
        truncated: software.truncated,
    }
}

fn describe_software(
    item: &InstalledSoftware,
    broad_root_count: &mut usize,
    invalid_path_count: &mut usize,
    no_static_path_count: &mut usize,
) -> SoftwareDescriptor {
    let mut clues = Vec::with_capacity(2);
    let mut file_name = None;
    let mut issues = Vec::new();
    let mut seen_clues = HashSet::new();

    if let Some(raw) = item.display_icon_path.as_deref() {
        if let Some(path) = normalize_local_path(raw) {
            if seen_clues.insert(format!("display:{}", path.key)) {
                clues.push(SoftwarePathClue {
                    path,
                    evidence_type: AssociationEvidenceType::DisplayIconPath,
                    is_file: true,
                });
            }
            file_name = path_file_name(
                clues
                    .last()
                    .map(|clue| clue.path.key.as_str())
                    .unwrap_or(""),
            )
            .filter(|name| is_process_file_name(name));
        } else {
            *invalid_path_count += 1;
            issues.push(association_issue(
                "ASSOCIATION-INVALID-INSTALL-PATH",
                "DisplayIcon 不是可比较的本地绝对路径；该值未参与关联。",
                &item.id,
            ));
        }
    }

    if let Some(raw) = item.install_location.as_deref() {
        if let Some(path) = normalize_local_path(raw) {
            let is_file = has_executable_extension(&path.key);
            if !is_file && is_broad_software_root(&path.key, &item.display_name) {
                *broad_root_count += 1;
                issues.push(association_issue(
                    "ASSOCIATION-BROAD-INSTALL-LOCATION",
                    "安装位置过于宽泛，不能作为软件归属证据；该值未参与路径包含匹配。",
                    &item.id,
                ));
            } else if seen_clues.insert(format!("install:{}", path.key)) {
                if is_file && file_name.is_none() {
                    file_name = path_file_name(&path.key).filter(|name| is_process_file_name(name));
                }
                clues.push(SoftwarePathClue {
                    path,
                    evidence_type: AssociationEvidenceType::InstallLocationPath,
                    is_file,
                });
            }
        } else {
            *invalid_path_count += 1;
            issues.push(association_issue(
                "ASSOCIATION-INVALID-INSTALL-PATH",
                "安装位置不是可比较的本地绝对路径；该值未参与关联。",
                &item.id,
            ));
        }
    }

    if clues.is_empty() {
        *no_static_path_count += 1;
    }

    SoftwareDescriptor {
        id: item.id.clone(),
        name: item.display_name.clone(),
        classification: item.classification,
        observation: item.observation,
        clues,
        file_name,
        issues,
    }
}

fn collect_candidates(software: &[SoftwareDescriptor], targets: &[TargetAsset]) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    let mut strong_pairs = HashSet::new();

    for (software_index, descriptor) in software.iter().enumerate() {
        for (target_index, target) in targets.iter().enumerate() {
            if let Some(path_match) = strong_match(descriptor, target) {
                strong_pairs.insert((software_index, target_index));
                candidates.push(Candidate {
                    software_index,
                    target_index,
                    evidence_type: path_match.evidence_type,
                    match_method: path_match.match_method,
                    certainty: path_match.certainty,
                    evidence_value: path_match.evidence_value,
                    software_evidence_path: path_match.software_evidence_path,
                    issues: target.issues.clone(),
                });
            }
        }
    }

    // Filename-only clues are deliberately limited to running processes and
    // ordinary user software. Services, tasks, and startup entries often point
    // at generic hosts such as svchost.exe; a name-only link there would create
    // more noise than evidence.
    for (software_index, descriptor) in software.iter().enumerate() {
        if descriptor.classification != SoftwareClassification::UserSoftware {
            continue;
        }
        let Some(software_file_name) = descriptor.file_name.as_deref() else {
            continue;
        };
        if is_generic_executable_name(software_file_name) {
            continue;
        }
        for (target_index, target) in targets.iter().enumerate() {
            if target.kind != AssociationTargetKind::Process
                || strong_pairs.contains(&(software_index, target_index))
            {
                continue;
            }
            let Some(target_file_name) = target.file_name.as_deref() else {
                continue;
            };
            if software_file_name != target_file_name
                || is_generic_executable_name(target_file_name)
                || is_windows_system_path(&target.path.key)
            {
                continue;
            }
            let evidence_value = bound_text(
                &redact_user_profile_path(software_file_name),
                MAX_ASSOCIATION_EVIDENCE_CHARS,
            )
            .value;
            candidates.push(Candidate {
                software_index,
                target_index,
                evidence_type: AssociationEvidenceType::ExecutableFileName,
                match_method: AssociationMatchMethod::SameFileNameDifferentDirectory,
                certainty: AssociationCertainty::Weak,
                evidence_value,
                software_evidence_path: descriptor
                    .clues
                    .iter()
                    .find(|clue| clue.evidence_type == AssociationEvidenceType::DisplayIconPath)
                    .map(|clue| redact_user_profile_path(&clue.path.display)),
                issues: target.issues.clone(),
            });
        }
    }

    candidates
}

fn strong_match(descriptor: &SoftwareDescriptor, target: &TargetAsset) -> Option<PathMatch> {
    for clue in &descriptor.clues {
        if clue.path.key == target.path.key {
            return Some(PathMatch {
                evidence_type: clue.evidence_type,
                match_method: AssociationMatchMethod::ExactPath,
                certainty: AssociationCertainty::Exact,
                evidence_value: redact_user_profile_path(&target.path.display),
                software_evidence_path: Some(redact_user_profile_path(&clue.path.display)),
            });
        }
    }

    for clue in &descriptor.clues {
        if clue.is_file || clue.evidence_type != AssociationEvidenceType::InstallLocationPath {
            continue;
        }
        if is_broad_root(&clue.path.key) || !is_descendant_path(&target.path.key, &clue.path.key) {
            continue;
        }
        return Some(PathMatch {
            evidence_type: AssociationEvidenceType::InstallLocationPath,
            match_method: AssociationMatchMethod::ContainedInInstallLocation,
            certainty: AssociationCertainty::Strong,
            evidence_value: redact_user_profile_path(&target.path.display),
            software_evidence_path: Some(redact_user_profile_path(&clue.path.display)),
        });
    }

    None
}

fn build_relation(
    descriptor: &SoftwareDescriptor,
    target: &TargetAsset,
    candidate_software_count: usize,
    candidate: &Candidate,
) -> AssetRelation {
    let mut issues = candidate.issues.clone();
    if candidate.certainty == AssociationCertainty::Weak {
        push_issue_unique(
            &mut issues,
            association_issue(
                WEAK_FILE_NAME_CODE,
                "仅可执行文件名相同且目录不同；这是弱线索，不能证明软件归属。",
                &target.id,
            ),
        );
    }
    if candidate_software_count > 1 {
        push_issue_unique(
            &mut issues,
            association_issue(
                MULTIPLE_CANDIDATES_CODE,
                format!(
                    "同一目标同时匹配 {candidate_software_count} 个软件候选；未强行选择唯一归属。"
                ),
                &target.id,
            ),
        );
    }
    if target.observation != InventoryObservation::Observed {
        push_issue_unique(
            &mut issues,
            association_issue(
                INCOMPLETE_SOURCE_CODE,
                "关联目标为部分读取；路径证据有效，但目标元数据不完整。",
                &target.id,
            ),
        );
    }
    if descriptor.observation != InventoryObservation::Observed {
        push_issue_unique(
            &mut issues,
            association_issue(
                INCOMPLETE_SOURCE_CODE,
                "软件条目为部分读取；路径证据有效，但软件元数据不完整。",
                &descriptor.id,
            ),
        );
    }

    let observation = merge_relation_observation(descriptor.observation, target.observation);
    let relation_type = relation_type(target.kind);
    let id = format!(
        "association:{}",
        stable_token(&format!(
            "{}\u{0}{}\u{0}{}\u{0}{}",
            descriptor.id,
            target.id,
            match_method_id(candidate.match_method),
            evidence_type_id(candidate.evidence_type)
        ))
    );

    AssetRelation {
        id,
        software_id: descriptor.id.clone(),
        software_name: descriptor.name.clone(),
        target: AssociationTarget {
            kind: target.kind,
            id: target.id.clone(),
            name: target.name.clone(),
            detail: target.detail.clone(),
            path: Some(
                bound_text(
                    &redact_user_profile_path(&target.path.display),
                    MAX_ASSOCIATION_EVIDENCE_CHARS,
                )
                .value,
            ),
            observation: target.observation,
        },
        relation_type,
        evidence_type: candidate.evidence_type,
        match_method: candidate.match_method,
        software_evidence_path: candidate.software_evidence_path.as_deref().map(|path| {
            bound_text(
                &redact_user_profile_path(path),
                MAX_ASSOCIATION_EVIDENCE_CHARS,
            )
            .value
        }),
        evidence_value: bound_text(&candidate.evidence_value, MAX_ASSOCIATION_EVIDENCE_CHARS).value,
        certainty: candidate.certainty,
        observation,
        ambiguous: candidate_software_count > 1,
        candidate_software_count,
        issues,
    }
}

fn count_relations(relations: &[AssetRelation]) -> HashMap<String, (usize, usize)> {
    let mut counts = HashMap::new();
    for relation in relations {
        let entry = counts
            .entry(relation.software_id.clone())
            .or_insert((0usize, 0usize));
        if relation.certainty == AssociationCertainty::Weak {
            entry.1 += 1;
        } else {
            entry.0 += 1;
        }
    }
    counts
}

fn candidate_counts_by_target(candidates: &[Candidate]) -> HashMap<usize, usize> {
    let mut software_by_target: HashMap<usize, HashSet<usize>> = HashMap::new();
    for candidate in candidates {
        software_by_target
            .entry(candidate.target_index)
            .or_default()
            .insert(candidate.software_index);
    }
    software_by_target
        .into_iter()
        .map(|(target, software)| (target, software.len()))
        .collect()
}

fn aggregate_observation(
    sources: &[AssociationSourceStatus],
    any_target_source_readable: bool,
    all_sources_observed: bool,
    truncated: bool,
) -> InventoryObservation {
    if !any_target_source_readable {
        let target_sources: Vec<&AssociationSourceStatus> = sources.iter().skip(1).collect();
        if !target_sources.is_empty()
            && target_sources
                .iter()
                .all(|source| source.observation == InventoryObservation::AccessDenied)
        {
            return InventoryObservation::AccessDenied;
        }
        if !target_sources.is_empty()
            && target_sources
                .iter()
                .all(|source| source.observation == InventoryObservation::Error)
        {
            return InventoryObservation::Error;
        }
        if !target_sources.is_empty()
            && target_sources
                .iter()
                .all(|source| source.observation == InventoryObservation::Unavailable)
        {
            return InventoryObservation::Unavailable;
        }
        return InventoryObservation::Partial;
    }

    if all_sources_observed && !truncated {
        InventoryObservation::Observed
    } else {
        InventoryObservation::Partial
    }
}

fn relation_type(kind: AssociationTargetKind) -> AssociationRelationType {
    match kind {
        AssociationTargetKind::Process => AssociationRelationType::SoftwareProcess,
        AssociationTargetKind::Service => AssociationRelationType::SoftwareService,
        AssociationTargetKind::ScheduledTask => AssociationRelationType::SoftwareScheduledTask,
        AssociationTargetKind::StartupItem => AssociationRelationType::SoftwareStartupItem,
    }
}

fn match_method_id(method: AssociationMatchMethod) -> &'static str {
    match method {
        AssociationMatchMethod::ExactPath => "exactPath",
        AssociationMatchMethod::ContainedInInstallLocation => "containedInInstallLocation",
        AssociationMatchMethod::SameFileNameDifferentDirectory => "sameFileNameDifferentDirectory",
    }
}

fn evidence_type_id(evidence_type: AssociationEvidenceType) -> &'static str {
    match evidence_type {
        AssociationEvidenceType::DisplayIconPath => "displayIconPath",
        AssociationEvidenceType::InstallLocationPath => "installLocationPath",
        AssociationEvidenceType::ExecutableFileName => "executableFileName",
    }
}

fn certainty_rank(certainty: AssociationCertainty) -> u8 {
    match certainty {
        AssociationCertainty::Exact => 0,
        AssociationCertainty::Strong => 1,
        AssociationCertainty::Weak => 2,
    }
}

fn target_kind_rank(kind: AssociationTargetKind) -> u8 {
    match kind {
        AssociationTargetKind::Process => 0,
        AssociationTargetKind::Service => 1,
        AssociationTargetKind::ScheduledTask => 2,
        AssociationTargetKind::StartupItem => 3,
    }
}

fn source_label(source: AssociationSource) -> &'static str {
    match source {
        AssociationSource::SoftwareInventory => "软件盘点",
        AssociationSource::ProcessSnapshot => "进程快照",
        AssociationSource::ServiceInventory => "服务盘点",
        AssociationSource::ScheduledTaskInventory => "计划任务盘点",
        AssociationSource::StartupInventory => "启动项盘点",
    }
}

fn observation_label(observation: InventoryObservation) -> &'static str {
    match observation {
        InventoryObservation::Observed => "已读取",
        InventoryObservation::Partial => "部分读取",
        InventoryObservation::Unavailable => "不可用",
        InventoryObservation::AccessDenied => "无权限",
        InventoryObservation::Error => "查询失败",
    }
}

fn startup_source_label(item: &StartupItem) -> &'static str {
    match item.source {
        crate::model::StartupSource::RegistryRun => "注册表 Run",
        crate::model::StartupSource::RegistryRunOnce => "注册表 RunOnce",
        crate::model::StartupSource::StartupFolder => "启动文件夹",
    }
}

fn merge_relation_observation(
    software: InventoryObservation,
    target: InventoryObservation,
) -> InventoryObservation {
    use InventoryObservation::{AccessDenied, Error, Observed, Partial, Unavailable};
    match (software, target) {
        (AccessDenied, _) | (_, AccessDenied) => AccessDenied,
        (Error, _) | (_, Error) => Error,
        (Observed, Observed) => Observed,
        (Unavailable, _) | (_, Unavailable) => Partial,
        (Partial, _) | (_, Partial) => Partial,
    }
}

fn association_issue(code: &str, message: impl Into<String>, source: &str) -> DataIssue {
    DataIssue {
        code: code.to_string(),
        message: message.into(),
        source: source.to_string(),
    }
}

fn push_issue_unique(issues: &mut Vec<DataIssue>, issue: DataIssue) {
    if !issues.iter().any(|existing| existing.code == issue.code) {
        issues.push(issue);
    }
}

fn association_limitations() -> Vec<String> {
    vec![
        "关联只使用当前进程快照、服务二进制路径、计划任务动作路径和启动项目标路径的静态路径证据；没有实时监控、文件内容、签名或行为证据。".to_string(),
        "路径比较是词法规范化，不解析 8.3 短名、符号链接、junction、环境变量或网络/设备路径。".to_string(),
        "弱线索只表示不同目录下可执行文件名相同，不能证明软件归属；默认不用于服务、任务或启动项。".to_string(),
        "没有关联不等于异常；来源不可用、部分读取或截断时显示证据不足，不显示为“不存在”。".to_string(),
        "不读取、不保留、不展示完整命令行、服务参数、任务参数或启动项参数。".to_string(),
        "关联结果只在本次内存中，不写 SQLite；不修改、不启动、不停止、不删除任何对象。".to_string(),
    ]
}

/// Normalizes a local drive path for case-insensitive comparison without
/// touching the filesystem. Returns `None` for UNC/device/relative/ambiguous
/// input.
fn normalize_local_path(raw: &str) -> Option<NormalizedLocalPath> {
    let trimmed = raw.trim().trim_matches('"').trim();
    if trimmed.is_empty()
        || trimmed.contains('%')
        || trimmed.contains('"')
        || trimmed.contains('\0')
        || trimmed.chars().any(char::is_control)
        || looks_like_command_with_arguments(trimmed)
    {
        return None;
    }

    let replaced = trimmed.replace('/', "\\");
    let without_prefix = replaced
        .strip_prefix(r"\\?\")
        .or_else(|| replaced.strip_prefix(r"\??\"))
        .unwrap_or(&replaced);
    if without_prefix
        .chars()
        .any(|character| matches!(character, '*' | '?' | '<' | '>' | '|'))
    {
        return None;
    }
    let bytes = without_prefix.as_bytes();
    if bytes.len() < 4 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'\\' {
        return None;
    }
    if without_prefix[3..].starts_with('\\') {
        return None;
    }

    let drive = without_prefix[..2].to_string();
    let raw_components = without_prefix[3..].split('\\');
    let mut components = Vec::new();
    for component in raw_components {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            components.pop()?;
            continue;
        }
        let component = component.trim_end_matches([' ', '.']);
        if component.is_empty() {
            continue;
        }
        if component.contains(':') {
            return None;
        }
        components.push(component.to_string());
    }
    if components.is_empty() {
        return None;
    }

    let display = format!("{}\\{}", drive.to_ascii_uppercase(), components.join("\\"));
    let key = display.to_lowercase();
    Some(NormalizedLocalPath { key, display })
}

fn looks_like_command_with_arguments(value: &str) -> bool {
    const EXTENSIONS: [&str; 11] = [
        ".exe", ".com", ".scr", ".sys", ".dll", ".bat", ".cmd", ".ps1", ".vbs", ".js", ".msi",
    ];
    let lower = value.to_ascii_lowercase();
    let Some(end) = EXTENSIONS
        .iter()
        .filter_map(|extension| lower.rfind(extension))
        .max()
    else {
        return false;
    };
    let extension_len = EXTENSIONS
        .iter()
        .find(|extension| lower[end..].starts_with(**extension))
        .map(|extension| extension.len())
        .unwrap_or(0);
    value[end + extension_len..]
        .chars()
        .next()
        .is_some_and(char::is_whitespace)
}

fn is_descendant_path(target: &str, root: &str) -> bool {
    target.len() > root.len()
        && target.starts_with(root)
        && target.as_bytes().get(root.len()) == Some(&b'\\')
}

fn is_broad_root(key: &str) -> bool {
    let Some(rest) = key.get(3..) else {
        return true;
    };
    let parts: Vec<&str> = rest.split('\\').collect();
    matches!(
        parts.as_slice(),
        ["program files"]
            | ["program files", "common files"]
            | ["program files", "windowsapps"]
            | ["program files (x86)"]
            | ["program files (x86)", "common files"]
            | ["programdata"]
            | ["programdata", "package cache"]
            | ["windows"]
            | ["windows", "system32"]
            | ["windows", "syswow64"]
            | ["windows", "winsxs"]
            | ["windows", "temp"]
            | ["windows", "system32", "driverstore"]
            | ["users"]
            | ["users", _]
            | ["users", _, "appdata"]
            | ["users", _, "appdata", "local"]
            | ["users", _, "appdata", "local", "programs"]
            | ["users", _, "appdata", "local", "packages"]
            | ["users", _, "appdata", "local", "temp"]
            | ["users", _, "appdata", "locallow"]
            | ["users", _, "appdata", "roaming"]
            | ["users", _, "desktop"]
            | ["users", _, "documents"]
            | ["users", _, "downloads"]
            | ["users", _, "onedrive"]
    )
}

/// A direct vendor directory under Program Files is often shared by several
/// products. It is not product-specific unless the display name itself confirms
/// the vendor root. Known vendor roots are always treated as broad.
fn is_broad_software_root(key: &str, display_name: &str) -> bool {
    if is_broad_root(key) {
        return true;
    }
    let Some(rest) = key.get(3..) else {
        return true;
    };
    let parts: Vec<&str> = rest.split('\\').collect();
    let vendor = match parts.as_slice() {
        ["program files", vendor] | ["program files (x86)", vendor] => *vendor,
        _ => return false,
    };
    const KNOWN_VENDOR_ROOTS: [&str; 14] = [
        "microsoft",
        "intel",
        "nvidia corporation",
        "google",
        "amd",
        "realtek",
        "dell",
        "hp",
        "lenovo",
        "adobe",
        "oracle",
        "mozilla",
        "common files",
        "windowsapps",
    ];
    KNOWN_VENDOR_ROOTS.contains(&vendor) || !display_name.to_lowercase().contains(vendor)
}

fn is_windows_system_path(key: &str) -> bool {
    key.contains("\\windows\\")
        || key.contains("\\program files\\")
        || key.contains("\\program files (x86)\\")
}

fn path_file_name(key: &str) -> Option<String> {
    key.rsplit('\\')
        .next()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn is_process_file_name(name: &str) -> bool {
    name.ends_with(".exe") || name.ends_with(".com")
}

fn has_executable_extension(key: &str) -> bool {
    const EXTENSIONS: [&str; 11] = [
        ".exe", ".com", ".scr", ".sys", ".dll", ".bat", ".cmd", ".ps1", ".vbs", ".js", ".msi",
    ];
    EXTENSIONS.iter().any(|extension| key.ends_with(extension))
}

fn is_generic_executable_name(name: &str) -> bool {
    const GENERIC: [&str; 35] = [
        "svchost.exe",
        "explorer.exe",
        "runtimebroker.exe",
        "conhost.exe",
        "cmd.exe",
        "powershell.exe",
        "pwsh.exe",
        "dllhost.exe",
        "rundll32.exe",
        "regsvr32.exe",
        "msiexec.exe",
        "wscript.exe",
        "cscript.exe",
        "taskhostw.exe",
        "sihost.exe",
        "backgroundtaskhost.exe",
        "searchhost.exe",
        "startmenuexperiencehost.exe",
        "textinputhost.exe",
        "applicationframehost.exe",
        "systemsettings.exe",
        "wmiprvse.exe",
        "mmc.exe",
        "java.exe",
        "javaw.exe",
        "python.exe",
        "pythonw.exe",
        "node.exe",
        "updater.exe",
        "update.exe",
        "setup.exe",
        "installer.exe",
        "uninstall.exe",
        "launcher.exe",
        "helper.exe",
    ];
    GENERIC.contains(&name.to_ascii_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Confidence, InventoryObservation, PathState, ProcessPathAccess, ProcessSummary,
        RegistryValueState, RegistryView, RiskAssessment, RiskLevel, RiskScope,
        ScheduledTaskAction, ScheduledTaskActionType, ScheduledTaskState, ServiceStartType,
        ServiceState, ServiceType, StartupPathResolution, StartupSource, StartupTargetKind,
        StartupValueType,
    };

    fn risk() -> RiskAssessment {
        RiskAssessment {
            scope: RiskScope::SoftwareMetadata,
            level: RiskLevel::NoClearIssue,
            confidence: Confidence::Low,
            rule_ids: Vec::new(),
            summary: "test".to_string(),
            evidence: Vec::new(),
            blind_spots: Vec::new(),
            recommendation: "test".to_string(),
        }
    }

    fn software(
        id: &str,
        name: &str,
        install: Option<&str>,
        icon: Option<&str>,
    ) -> InstalledSoftware {
        InstalledSoftware {
            id: id.to_string(),
            display_name: name.to_string(),
            version: None,
            version_state: RegistryValueState::Missing,
            publisher: None,
            publisher_state: RegistryValueState::Missing,
            install_location: install.map(str::to_string),
            install_location_state: PathState::Present,
            install_location_value_state: RegistryValueState::Present,
            install_date: None,
            install_date_state: RegistryValueState::Missing,
            display_icon_path: icon.map(str::to_string),
            display_icon_state: if icon.is_some() {
                RegistryValueState::Present
            } else {
                RegistryValueState::Missing
            },
            windows_installer: None,
            windows_installer_state: RegistryValueState::Missing,
            uninstall_command_present: true,
            quiet_uninstall_command_present: false,
            scope: crate::model::InstallScope::Machine,
            registry_view: RegistryView::Registry64,
            registry_key: "HKEY_LOCAL_MACHINE\\test".to_string(),
            system_component: None,
            system_component_state: RegistryValueState::Missing,
            no_remove: None,
            no_remove_state: RegistryValueState::Missing,
            parent_key_name: None,
            parent_key_name_state: RegistryValueState::Missing,
            release_type: None,
            release_type_state: RegistryValueState::Missing,
            classification: SoftwareClassification::UserSoftware,
            observation: InventoryObservation::Observed,
            issues: Vec::new(),
            risk: risk(),
        }
    }

    fn software_result(items: Vec<InstalledSoftware>) -> SoftwareScanResult {
        SoftwareScanResult {
            checked_at: "2026-11-01T00:00:00Z".to_string(),
            observation: InventoryObservation::Observed,
            sources: Vec::new(),
            items,
            issues: Vec::new(),
            limitations: Vec::new(),
            truncated: false,
        }
    }

    fn process(pid: u32, name: &str, path: &str) -> ProcessSummary {
        ProcessSummary {
            pid,
            parent_pid: 1,
            name: name.to_string(),
            executable_path: Some(path.to_string()),
            path_access: ProcessPathAccess::Available,
            parent_name: None,
            identity: Some(format!("pidct1:{pid:016x}")),
            start_time: None,
        }
    }

    fn process_result(items: Vec<ProcessSummary>) -> ProcessListResult {
        ProcessListResult {
            checked_at: "2026-11-01T00:00:00Z".to_string(),
            processes: items,
            issues: Vec::new(),
            limitations: Vec::new(),
            truncated: false,
        }
    }

    fn service(id: &str, name: &str, path: Option<&str>) -> ServiceSummary {
        ServiceSummary {
            id: id.to_string(),
            name: name.to_string(),
            display_name: name.to_string(),
            state: ServiceState::Running,
            start_type: ServiceStartType::Auto,
            service_types: vec![ServiceType::OwnProcess],
            account: None,
            executable_path: path.map(str::to_string),
            observation: InventoryObservation::Observed,
            issues: Vec::new(),
        }
    }

    fn service_result(items: Vec<ServiceSummary>) -> ServiceInventoryResult {
        ServiceInventoryResult {
            checked_at: "2026-11-01T00:00:00Z".to_string(),
            observation: InventoryObservation::Observed,
            items,
            issues: Vec::new(),
            limitations: Vec::new(),
            truncated: false,
        }
    }

    fn task(
        id: &str,
        name: &str,
        actions: Vec<ScheduledTaskAction>,
        observation: InventoryObservation,
    ) -> ScheduledTaskSummary {
        ScheduledTaskSummary {
            id: id.to_string(),
            name: name.to_string(),
            path: format!("\\{name}"),
            state: ScheduledTaskState::Ready,
            enabled: Some(true),
            author: None,
            run_as: None,
            actions,
            actions_truncated: false,
            observation,
            issues: Vec::new(),
        }
    }

    fn task_result(items: Vec<ScheduledTaskSummary>) -> ScheduledTaskInventoryResult {
        ScheduledTaskInventoryResult {
            checked_at: "2026-11-01T00:00:00Z".to_string(),
            observation: InventoryObservation::Observed,
            items,
            issues: Vec::new(),
            limitations: Vec::new(),
            truncated: false,
        }
    }

    fn startup(id: &str, name: &str, path: &str) -> StartupItem {
        StartupItem {
            id: id.to_string(),
            name: name.to_string(),
            value_name: Some(name.to_string()),
            value_type: Some(StartupValueType::RegSz),
            raw_value_readable: true,
            source: StartupSource::RegistryRun,
            scope: crate::model::InstallScope::User,
            registry_view: Some(RegistryView::Shared),
            registry_key: Some("HKEY_CURRENT_USER\\...\\Run".to_string()),
            target_kind: StartupTargetKind::RegistryCommand,
            target_path: Some(path.to_string()),
            target_candidate: None,
            target_state: PathState::Present,
            path_resolution: StartupPathResolution::QuotedExecutable,
            shortcut_arguments_present: Some(true),
            shortcut_working_directory_present: None,
            approval_state: crate::model::StartupApprovalState::NotChecked,
            observation: InventoryObservation::Observed,
            issues: Vec::new(),
            risk: risk(),
        }
    }

    fn startup_result(items: Vec<StartupItem>) -> StartupScanResult {
        StartupScanResult {
            checked_at: "2026-11-01T00:00:00Z".to_string(),
            observation: InventoryObservation::Observed,
            sources: Vec::new(),
            items,
            issues: Vec::new(),
            limitations: Vec::new(),
            truncated: false,
        }
    }

    fn snapshots<'a>(
        software: &'a SoftwareScanResult,
        processes: Option<&'a ProcessListResult>,
        services: &'a ServiceInventoryResult,
        tasks: &'a ScheduledTaskInventoryResult,
        startup: &'a StartupScanResult,
    ) -> AssociationSnapshots<'a> {
        AssociationSnapshots {
            software,
            processes,
            process_error: processes.is_none(),
            services,
            tasks,
            startup,
        }
    }

    fn run(
        software: Vec<InstalledSoftware>,
        processes: Vec<ProcessSummary>,
        services: Vec<ServiceSummary>,
        tasks: Vec<ScheduledTaskSummary>,
        startup: Vec<StartupItem>,
    ) -> AssetAssociationResult {
        let software = software_result(software);
        let processes = process_result(processes);
        let services = service_result(services);
        let tasks = task_result(tasks);
        let startup = startup_result(startup);
        associate_snapshots(
            "2026-11-01T00:00:00Z".to_string(),
            &snapshots(&software, Some(&processes), &services, &tasks, &startup),
        )
    }

    #[test]
    fn windows_path_comparison_is_case_insensitive_and_slash_aware() {
        let left = normalize_local_path(r"C:\Program Files\Vendor\App.EXE").unwrap();
        let right = normalize_local_path(r"c:/program files/vendor/app.exe").unwrap();
        assert_eq!(left.key, right.key);
        assert_eq!(left.display, r"C:\Program Files\Vendor\App.EXE");
    }

    #[test]
    fn path_boundaries_do_not_treat_a_prefix_as_a_child() {
        let root = normalize_local_path(r"C:\Program Files\Vendor").unwrap();
        let child = normalize_local_path(r"C:\Program Files\Vendor\app.exe").unwrap();
        let sibling = normalize_local_path(r"C:\Program Files\VendorExtra\app.exe").unwrap();
        assert!(is_descendant_path(&child.key, &root.key));
        assert!(!is_descendant_path(&sibling.key, &root.key));
    }

    #[test]
    fn path_normalization_removes_dot_segments_and_device_prefix() {
        let path = normalize_local_path(r"\\?\C:\Program Files\Vendor\sub\..\app.exe").unwrap();
        assert_eq!(path.display, r"C:\Program Files\Vendor\app.exe");
        assert_eq!(path.key, r"c:\program files\vendor\app.exe");
    }

    #[test]
    fn invalid_paths_are_not_comparable() {
        for raw in [
            "",
            r"relative\app.exe",
            r"\\server\share\app.exe",
            r"%ProgramFiles%\Vendor\app.exe",
            r"C:\Program Files\Vendor\app.exe --run",
        ] {
            assert!(normalize_local_path(raw).is_none(), "{raw}");
        }
    }

    #[test]
    fn display_icon_exact_path_matches_a_process() {
        let result = run(
            vec![software(
                "app",
                "Example",
                Some(r"C:\Program Files\Example"),
                Some(r"C:\Program Files\Example\example.exe"),
            )],
            vec![process(
                7,
                "example.exe",
                r"C:\Program Files\Example\example.exe",
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(result.relations.len(), 1);
        assert_eq!(result.relations[0].certainty, AssociationCertainty::Exact);
    }

    #[test]
    fn install_location_containment_is_strong_but_same_name_elsewhere_is_only_weak() {
        let result = run(
            vec![software(
                "app",
                "Example",
                Some(r"C:\Program Files\Example"),
                Some(r"C:\Program Files\Example\example.exe"),
            )],
            vec![
                process(7, "example.exe", r"C:\Program Files\Example\bin\worker.exe"),
                process(8, "example.exe", r"D:\Other\example.exe"),
            ],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        assert!(result.relations.iter().any(|relation| {
            relation.target.id.starts_with("process:7:")
                && relation.certainty == AssociationCertainty::Strong
        }));
        let weak = result
            .relations
            .iter()
            .find(|relation| relation.target.id.starts_with("process:8:"))
            .expect("weak relation");
        assert_eq!(weak.certainty, AssociationCertainty::Weak);
        assert_eq!(
            weak.match_method,
            AssociationMatchMethod::SameFileNameDifferentDirectory
        );
    }

    #[test]
    fn same_file_name_in_a_different_directory_never_becomes_exact() {
        let result = run(
            vec![software(
                "app",
                "Example",
                Some(r"C:\Program Files\Example"),
                Some(r"C:\Program Files\Example\example.exe"),
            )],
            vec![process(8, "example.exe", r"D:\Other\example.exe")],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(result.relations.len(), 1);
        assert_eq!(result.relations[0].certainty, AssociationCertainty::Weak);
        assert_ne!(result.relations[0].certainty, AssociationCertainty::Exact);
        assert!(!result.relations[0].ambiguous);
    }

    #[test]
    fn service_and_task_paths_match_without_arguments() {
        let result = run(
            vec![software(
                "app",
                "Example",
                Some(r"C:\Program Files\Example"),
                Some(r"C:\Program Files\Example\example.exe"),
            )],
            Vec::new(),
            vec![service(
                "service:ExampleSvc",
                "Example Service",
                Some(r"C:\Program Files\Example\service.exe"),
            )],
            vec![task(
                "task:\\Example",
                "Example Task",
                vec![ScheduledTaskAction {
                    action_type: ScheduledTaskActionType::Exec,
                    executable_path: Some(r"C:\Program Files\Example\task.exe".to_string()),
                }],
                InventoryObservation::Observed,
            )],
            Vec::new(),
        );
        assert_eq!(result.relations.len(), 2);
        assert!(
            result
                .relations
                .iter()
                .all(|relation| relation.certainty == AssociationCertainty::Strong)
        );
        assert!(result.relations.iter().all(|relation| {
            !relation.evidence_value.contains(" -") && !relation.evidence_value.contains(" /")
        }));
    }

    #[test]
    fn startup_target_path_matches_and_missing_task_action_is_insufficient() {
        let result = run(
            vec![software(
                "app",
                "Example",
                Some(r"C:\Program Files\Example"),
                Some(r"C:\Program Files\Example\example.exe"),
            )],
            Vec::new(),
            Vec::new(),
            vec![task(
                "task:\\EmptyAction",
                "Empty Action",
                Vec::new(),
                InventoryObservation::Partial,
            )],
            vec![startup(
                "startup:test",
                "Example",
                r"C:\Program Files\Example\example.exe",
            )],
        );
        assert!(result.relations.iter().any(|relation| {
            relation.target.kind == AssociationTargetKind::StartupItem
                && relation.certainty == AssociationCertainty::Exact
        }));
        let summary = result
            .software
            .iter()
            .find(|summary| summary.software_id == "app")
            .unwrap();
        // The startup match still counts, but the task source is incomplete and
        // is not silently reported as a clean no-match for other software.
        assert_eq!(summary.state, SoftwareAssociationState::Linked);
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.code == INCOMPLETE_SOURCE_CODE)
        );
    }

    #[test]
    fn incomplete_task_source_prevents_no_match_without_hiding_other_evidence() {
        let software = software_result(vec![
            software(
                "linked",
                "Linked",
                Some(r"C:\Program Files\Linked"),
                Some(r"C:\Program Files\Linked\linked.exe"),
            ),
            software(
                "unmatched",
                "Unmatched",
                Some(r"C:\Program Files\Unmatched"),
                None,
            ),
        ]);
        let processes = process_result(vec![process(
            7,
            "linked.exe",
            r"C:\Program Files\Linked\linked.exe",
        )]);
        let services = service_result(Vec::new());
        let startup = startup_result(Vec::new());
        let mut tasks = task_result(vec![task(
            "task:\\Partial",
            "Partial",
            Vec::new(),
            InventoryObservation::Partial,
        )]);
        tasks.observation = InventoryObservation::Partial;
        tasks.truncated = true;

        let result = associate_snapshots(
            "2026-11-01T00:00:00Z".to_string(),
            &snapshots(&software, Some(&processes), &services, &tasks, &startup),
        );

        let linked = result
            .software
            .iter()
            .find(|summary| summary.software_id == "linked")
            .expect("linked software");
        assert_eq!(linked.state, SoftwareAssociationState::Linked);
        assert!(result.relations.iter().any(|relation| {
            relation.software_id == "linked" && relation.certainty == AssociationCertainty::Exact
        }));

        let unmatched = result
            .software
            .iter()
            .find(|summary| summary.software_id == "unmatched")
            .expect("unmatched software");
        assert_eq!(
            unmatched.state,
            SoftwareAssociationState::InsufficientEvidence
        );
        assert!(result.issues.iter().any(|issue| {
            issue.code == INCOMPLETE_SOURCE_CODE && issue.source == "scheduled-tasks:task-scheduler"
        }));
    }

    #[test]
    fn multiple_software_candidates_are_preserved_as_ambiguous() {
        let result = run(
            vec![
                software(
                    "parent",
                    "Vendor Suite Parent",
                    Some(r"C:\Program Files\Vendor Suite"),
                    None,
                ),
                software(
                    "child",
                    "Vendor Suite Child",
                    Some(r"C:\Program Files\Vendor Suite\Child"),
                    None,
                ),
            ],
            vec![process(
                7,
                "child.exe",
                r"C:\Program Files\Vendor Suite\Child\child.exe",
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(result.relations.len(), 2);
        assert!(result.relations.iter().all(|relation| relation.ambiguous));
        assert!(result.relations.iter().all(|relation| {
            relation.candidate_software_count == 2
                && relation
                    .issues
                    .iter()
                    .any(|issue| issue.code == MULTIPLE_CANDIDATES_CODE)
        }));
    }

    #[test]
    fn no_comparable_path_is_insufficient_evidence_not_no_match() {
        let result = run(
            vec![software(
                "app",
                "Example",
                Some(r"%ProgramFiles%\Example"),
                None,
            )],
            vec![process(
                7,
                "example.exe",
                r"C:\Program Files\Example\example.exe",
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let summary = &result.software[0];
        assert_eq!(
            summary.state,
            SoftwareAssociationState::InsufficientEvidence
        );
        assert_eq!(summary.path_evidence_count, 0);
        assert!(
            summary
                .issues
                .iter()
                .any(|issue| issue.code == "ASSOCIATION-NO-STATIC-PATH")
        );
    }

    #[test]
    fn association_matching_is_case_insensitive_but_respects_path_boundaries() {
        let result = run(
            vec![software(
                "app",
                "Example",
                Some(r"C:\Program Files\Example"),
                None,
            )],
            vec![process(
                7,
                "other.exe",
                r"c:\program files\example extra\other.exe",
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        assert!(result.relations.is_empty());
        assert_eq!(result.software[0].state, SoftwareAssociationState::NoMatch);

        let result = run(
            vec![software(
                "app",
                "Example",
                Some(r"C:\Program Files\Example"),
                None,
            )],
            vec![process(
                7,
                "other.exe",
                r"c:\program files\example\other.exe",
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(result.relations.len(), 1);
        assert_eq!(result.relations[0].certainty, AssociationCertainty::Strong);
    }

    #[test]
    fn missing_task_action_keeps_the_result_insufficient_instead_of_no_match() {
        let result = run(
            vec![software(
                "app",
                "Example",
                Some(r"C:\Program Files\Example"),
                None,
            )],
            Vec::new(),
            Vec::new(),
            vec![task(
                "task:\\MissingAction",
                "Missing Action",
                Vec::new(),
                InventoryObservation::Partial,
            )],
            Vec::new(),
        );
        assert!(result.relations.is_empty());
        assert_eq!(
            result.software[0].state,
            SoftwareAssociationState::InsufficientEvidence
        );
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.code == INCOMPLETE_SOURCE_CODE)
        );
    }

    #[test]
    fn vendor_roots_are_broad_unless_the_display_name_confirms_them() {
        assert!(is_broad_software_root(
            r"c:\program files (x86)\intel",
            "英特尔(R) 无线 Bluetooth(R)"
        ));
        assert!(is_broad_software_root(
            r"c:\program files\microsoft",
            "Microsoft Edge"
        ));
        assert!(!is_broad_software_root(r"c:\program files\git", "Git"));
        assert!(!is_broad_software_root(
            r"c:\program files\denuvo anti-cheat",
            "Denuvo Anti-Cheat"
        ));
    }

    #[test]
    fn broad_install_roots_are_not_used_as_ownership_evidence() {
        let result = run(
            vec![software("app", "Example", Some(r"C:\Program Files"), None)],
            vec![process(
                7,
                "example.exe",
                r"C:\Program Files\Example\example.exe",
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        assert!(result.relations.is_empty());
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.code == "ASSOCIATION-BROAD-INSTALL-LOCATION")
        );
        assert_eq!(
            result.software[0].state,
            SoftwareAssociationState::InsufficientEvidence
        );
    }

    #[test]
    fn user_profile_paths_are_redacted_in_evidence() {
        let result = run(
            vec![software(
                "app",
                "Example",
                Some(r"C:\Users\Alice\AppData\Local\Programs\Example"),
                Some(r"C:\Users\Alice\AppData\Local\Programs\Example\example.exe"),
            )],
            vec![process(
                7,
                "example.exe",
                r"C:\Users\Alice\AppData\Local\Programs\Example\example.exe",
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let relation = &result.relations[0];
        assert!(relation.evidence_value.contains(r"C:\Users\***\"));
        assert!(!relation.evidence_value.contains("Alice"));
        assert!(
            relation
                .target
                .path
                .as_deref()
                .unwrap()
                .contains(r"C:\Users\***\")
        );
    }

    #[test]
    fn relation_ids_and_order_are_stable_for_the_same_snapshot() {
        let software = vec![software(
            "app",
            "Example",
            Some(r"C:\Program Files\Example"),
            Some(r"C:\Program Files\Example\example.exe"),
        )];
        let processes = vec![
            process(7, "example.exe", r"C:\Program Files\Example\example.exe"),
            process(8, "example.exe", r"C:\Program Files\Example\tool.exe"),
        ];
        let first = run(
            software.clone(),
            processes.clone(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let second = run(software, processes, Vec::new(), Vec::new(), Vec::new());
        let first_ids: Vec<_> = first.relations.iter().map(|item| item.id.clone()).collect();
        let second_ids: Vec<_> = second
            .relations
            .iter()
            .map(|item| item.id.clone())
            .collect();
        assert_eq!(first_ids, second_ids);
        assert_eq!(first.relations.len(), second.relations.len());
    }

    #[test]
    fn unavailable_process_source_is_not_rendered_as_a_clean_no_match() {
        let software = software_result(vec![software(
            "app",
            "Example",
            Some(r"C:\Program Files\Example"),
            None,
        )]);
        let services = service_result(Vec::new());
        let tasks = task_result(Vec::new());
        let startup = startup_result(Vec::new());
        let result = associate_snapshots(
            "2026-11-01T00:00:00Z".to_string(),
            &snapshots(&software, None, &services, &tasks, &startup),
        );
        assert_eq!(
            result.software[0].state,
            SoftwareAssociationState::InsufficientEvidence
        );
        assert_eq!(result.sources[1].observation, InventoryObservation::Error);
        assert_ne!(result.observation, InventoryObservation::Observed);
    }

    #[test]
    fn serialized_association_result_has_no_command_line_fields() {
        let result = run(
            vec![software(
                "app",
                "Example",
                Some(r"C:\Program Files\Example"),
                Some(r"C:\Program Files\Example\example.exe"),
            )],
            vec![process(
                7,
                "example.exe",
                r"C:\Program Files\Example\example.exe",
            )],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let value = serde_json::to_value(&result).expect("serialize");
        let text = value.to_string();
        assert!(!text.contains("commandLine"));
        assert!(!text.contains("uninstallCommand"));
        assert!(!text.contains("arguments"));
    }
}
