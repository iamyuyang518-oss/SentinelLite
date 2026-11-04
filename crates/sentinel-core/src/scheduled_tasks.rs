//! Read-only Windows Task Scheduler inventory.
//!
//! The scanner enumerates folders and registered tasks, then reads the definition metadata and
//! executable action paths. It never runs, creates, updates, enables, disables, or deletes a
//! task. Arguments and working directories are deliberately not retained.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::TaskScheduler::{
    IActionCollection, IExecAction, IRegisteredTask, ITaskService, TASK_ACTION_COM_HANDLER,
    TASK_ACTION_EXEC, TASK_ACTION_SEND_EMAIL, TASK_ACTION_SHOW_MESSAGE, TASK_ENUM_HIDDEN,
    TASK_STATE_DISABLED, TASK_STATE_QUEUED, TASK_STATE_READY, TASK_STATE_RUNNING, TaskScheduler,
};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::core::{BSTR, Interface};

use crate::com::ComApartment;
use crate::limits::{
    MAX_TASK_ACTION_PATH_CHARS, MAX_TASK_ACTIONS, MAX_TASK_FOLDERS, MAX_TASK_IDENTITY_CHARS,
    MAX_TASK_ITEMS, MAX_TASK_NAME_CHARS, MAX_TASK_PATH_CHARS, bound_text,
};
use crate::model::{
    DataIssue, InventoryObservation, ScheduledTaskAction, ScheduledTaskActionType,
    ScheduledTaskInventoryResult, ScheduledTaskState, ScheduledTaskSummary,
};
use crate::util::now_rfc3339;

const TIME_LIMIT: Duration = Duration::from_secs(10);
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_SERVICE_DISABLED: u32 = 1_058;
const ERROR_SERVICE_NOT_ACTIVE: u32 = 1_062;
const ERROR_SERVICE_DOES_NOT_EXIST: u32 = 1_060;

#[derive(Debug)]
struct TaskReadOutcome {
    summary: Option<ScheduledTaskSummary>,
    issue: bool,
    denied: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CountFailure {
    HResult(u32),
    InvalidNegative(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BoundedReadPlan {
    limit: usize,
    truncated: bool,
    failure: Option<CountFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActionReadPlan {
    count: usize,
    truncated: bool,
    failure: Option<CountFailure>,
}

impl CountFailure {
    fn detail(self) -> String {
        match self {
            Self::HResult(code) => format!("HRESULT 0x{code:08X}"),
            Self::InvalidNegative(value) => format!("无效计数 {value}"),
        }
    }
}

fn plan_task_read(result: Result<i32, u32>, available: usize) -> BoundedReadPlan {
    plan_bounded_read(result, available)
}

fn plan_folder_read(result: Result<i32, u32>, available: usize) -> BoundedReadPlan {
    plan_bounded_read(result, available)
}

fn plan_bounded_read(result: Result<i32, u32>, available: usize) -> BoundedReadPlan {
    match result {
        Ok(count) if count >= 0 => {
            let count = count as usize;
            BoundedReadPlan {
                limit: count.min(available),
                truncated: count > available,
                failure: None,
            }
        }
        Ok(count) => BoundedReadPlan {
            limit: 0,
            truncated: true,
            failure: Some(CountFailure::InvalidNegative(count)),
        },
        Err(code) => BoundedReadPlan {
            limit: 0,
            truncated: true,
            failure: Some(CountFailure::HResult(code)),
        },
    }
}

fn plan_action_read(result: Result<i32, u32>) -> ActionReadPlan {
    match result {
        Ok(count) if count >= 0 => {
            let count = count as usize;
            ActionReadPlan {
                count: count.min(MAX_TASK_ACTIONS),
                truncated: count > MAX_TASK_ACTIONS,
                failure: None,
            }
        }
        Ok(count) => ActionReadPlan {
            count: 0,
            truncated: false,
            failure: Some(CountFailure::InvalidNegative(count)),
        },
        Err(code) => ActionReadPlan {
            count: 0,
            truncated: false,
            failure: Some(CountFailure::HResult(code)),
        },
    }
}

fn apply_bounded_plan(
    plan: &BoundedReadPlan,
    observation: &mut InventoryObservation,
    truncated: &mut bool,
) -> Option<CountFailure> {
    if plan.failure.is_some() || plan.truncated {
        *observation = InventoryObservation::Partial;
        *truncated = true;
    }
    plan.failure
}

fn apply_action_plan(
    plan: &ActionReadPlan,
    observation: &mut InventoryObservation,
) -> Option<CountFailure> {
    if plan.failure.is_some() || plan.truncated {
        *observation = InventoryObservation::Partial;
    }
    plan.failure
}

pub fn list_scheduled_tasks() -> ScheduledTaskInventoryResult {
    let started = Instant::now();
    let checked_at = now_rfc3339();
    let _apartment = match ComApartment::initialize() {
        Ok(apartment) => apartment,
        Err(code) => {
            return failed_result(
                checked_at,
                observation_for_code(code),
                vec![issue(
                    "TASK-INVENTORY-COM-UNAVAILABLE",
                    format!("无法初始化 COM（HRESULT 0x{code:08X}）；计划任务列表未知。"),
                    "Task Scheduler",
                )],
            );
        }
    };

    let service: ITaskService = match unsafe {
        CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
    } {
        Ok(service) => service,
        Err(error) => {
            let code = hresult_code(&error);
            return failed_result(
                checked_at,
                observation_for_code(code),
                vec![issue(
                    "TASK-INVENTORY-UNAVAILABLE",
                    format!("无法连接任务计划程序服务（HRESULT 0x{code:08X}）；计划任务列表未知。"),
                    "Task Scheduler",
                )],
            );
        }
    };

    let empty = VARIANT::default();
    if let Err(error) = unsafe { service.Connect(&empty, &empty, &empty, &empty) } {
        let code = hresult_code(&error);
        return failed_result(
            checked_at,
            observation_for_code(code),
            vec![issue(
                "TASK-INVENTORY-CONNECT-FAILED",
                format!("无法连接本机任务计划程序（HRESULT 0x{code:08X}）；计划任务列表未知。"),
                "Task Scheduler",
            )],
        );
    }

    let root = match unsafe { service.GetFolder(&BSTR::from("\\")) } {
        Ok(folder) => folder,
        Err(error) => {
            let code = hresult_code(&error);
            return failed_result(
                checked_at,
                observation_for_code(code),
                vec![issue(
                    "TASK-INVENTORY-ROOT-FAILED",
                    format!(
                        "无法打开任务计划程序根文件夹（HRESULT 0x{code:08X}）；计划任务列表未知。"
                    ),
                    "Task Scheduler",
                )],
            );
        }
    };

    let mut folders = VecDeque::new();
    folders.push_back(root);
    let mut folders_seen = 0usize;
    let mut items = Vec::new();
    let mut issues = Vec::new();
    let mut truncated = false;
    let mut observation = InventoryObservation::Observed;
    let mut partial_items = 0usize;
    let mut denied_items = 0usize;
    let mut unreadable_tasks = 0usize;
    let mut failed_folders = 0usize;

    while let Some(folder) = folders.pop_front() {
        if started.elapsed() >= TIME_LIMIT {
            truncated = true;
            observation = InventoryObservation::Partial;
            issues.push(issue(
                "TASK-INVENTORY-TIME-LIMIT",
                "计划任务枚举超过 10 秒安全上限，已停止继续读取；结果可能不完整。",
                "Task Scheduler",
            ));
            break;
        }
        folders_seen += 1;
        if folders_seen > MAX_TASK_FOLDERS {
            truncated = true;
            observation = InventoryObservation::Partial;
            issues.push(issue(
                "TASK-INVENTORY-FOLDER-LIMIT",
                format!("计划任务文件夹超过 {MAX_TASK_FOLDERS} 个安全上限；结果不完整。"),
                "Task Scheduler",
            ));
            break;
        }

        match unsafe { folder.GetTasks(TASK_ENUM_HIDDEN.0) } {
            Ok(collection) => {
                let available = MAX_TASK_ITEMS.saturating_sub(items.len());
                let plan = plan_task_read(
                    unsafe { collection.Count() }.map_err(|error| hresult_code(&error)),
                    available,
                );
                if let Some(failure) = apply_bounded_plan(&plan, &mut observation, &mut truncated) {
                    issues.push(issue(
                        "TASK-INVENTORY-TASKS-COUNT-UNREADABLE",
                        format!(
                            "一个任务文件夹的任务集合计数读取失败（{}）；该文件夹中的任务可能不完整。",
                            failure.detail()
                        ),
                        "Task Scheduler",
                    ));
                }
                for index in 1..=plan.limit {
                    if items.len() >= MAX_TASK_ITEMS {
                        truncated = true;
                        observation = InventoryObservation::Partial;
                        break;
                    }
                    if started.elapsed() >= TIME_LIMIT {
                        truncated = true;
                        observation = InventoryObservation::Partial;
                        break;
                    }
                    let variant = variant_index(index as i32);
                    match unsafe { collection.get_Item(&variant) } {
                        Ok(task) => {
                            let outcome = read_task(&task);
                            if outcome.issue {
                                partial_items += 1;
                            }
                            if outcome.denied {
                                denied_items += 1;
                            }
                            if let Some(summary) = outcome.summary {
                                items.push(summary);
                            } else {
                                unreadable_tasks += 1;
                            }
                        }
                        Err(error) => {
                            unreadable_tasks += 1;
                            if hresult_code(&error) == ERROR_ACCESS_DENIED {
                                denied_items += 1;
                            }
                        }
                    }
                }
            }
            Err(error) => {
                failed_folders += 1;
                if hresult_code(&error) == ERROR_ACCESS_DENIED {
                    denied_items += 1;
                }
                issues.push(issue(
                    "TASK-INVENTORY-FOLDER-TASKS-FAILED",
                    format!(
                        "一个任务文件夹的任务列表读取失败（HRESULT 0x{:08X}）；该文件夹可能不完整。",
                        hresult_code(&error)
                    ),
                    "Task Scheduler",
                ));
                observation = InventoryObservation::Partial;
            }
        }

        match unsafe { folder.GetFolders(0) } {
            Ok(collection) => {
                let available = MAX_TASK_FOLDERS.saturating_sub(folders_seen + folders.len());
                let plan = plan_folder_read(
                    unsafe { collection.Count() }.map_err(|error| hresult_code(&error)),
                    available,
                );
                if let Some(failure) = apply_bounded_plan(&plan, &mut observation, &mut truncated) {
                    issues.push(issue(
                        "TASK-INVENTORY-SUBFOLDERS-COUNT-UNREADABLE",
                        format!(
                            "一个任务文件夹的子文件夹集合计数读取失败（{}）；结果可能不完整。",
                            failure.detail()
                        ),
                        "Task Scheduler",
                    ));
                }
                for index in 1..=plan.limit {
                    let variant = variant_index(index as i32);
                    match unsafe { collection.get_Item(&variant) } {
                        Ok(child) => folders.push_back(child),
                        Err(error) => {
                            failed_folders += 1;
                            if hresult_code(&error) == ERROR_ACCESS_DENIED {
                                denied_items += 1;
                            }
                        }
                    }
                }
            }
            Err(error) => {
                failed_folders += 1;
                if hresult_code(&error) == ERROR_ACCESS_DENIED {
                    denied_items += 1;
                }
                issues.push(issue(
                    "TASK-INVENTORY-SUBFOLDERS-FAILED",
                    format!(
                        "一个任务文件夹的子文件夹列表读取失败（HRESULT 0x{:08X}）；结果可能不完整。",
                        hresult_code(&error)
                    ),
                    "Task Scheduler",
                ));
                observation = InventoryObservation::Partial;
            }
        }
    }

    if partial_items > 0 || unreadable_tasks > 0 || failed_folders > 0 {
        if observation == InventoryObservation::Observed {
            observation = InventoryObservation::Partial;
        }
        issues.push(issue(
            "TASK-INVENTORY-PARTIAL",
            format!(
                "{} 个任务/文件夹未能完整读取；这些条目保持部分或未知，不会当作正常。",
                partial_items + unreadable_tasks + failed_folders
            ),
            "Task Scheduler",
        ));
    }
    if denied_items > 0 {
        if observation == InventoryObservation::Observed {
            observation = InventoryObservation::Partial;
        }
        issues.push(issue(
            "TASK-INVENTORY-ACCESS-DENIED",
            format!("{denied_items} 个任务或文件夹因权限不足无法读取；未把它们当作正常。"),
            "Task Scheduler",
        ));
    }
    if truncated {
        issues.push(issue(
            "TASK-INVENTORY-TRUNCATED",
            format!(
                "计划任务结果未完整读取（达到 {MAX_TASK_ITEMS} 项 / {MAX_TASK_FOLDERS} 个文件夹的安全上限、集合计数失败或超过 10 秒），不是完整列表。"
            ),
            "Task Scheduler",
        ));
    }
    if items.is_empty() && observation == InventoryObservation::Observed {
        issues.push(issue(
            "TASK-INVENTORY-EMPTY",
            "计划任务枚举没有返回可显示条目；这不代表系统中没有计划任务。",
            "Task Scheduler",
        ));
    }
    if started.elapsed() >= TIME_LIMIT && !truncated {
        truncated = true;
        observation = InventoryObservation::Partial;
        issues.push(issue(
            "TASK-INVENTORY-TIME-LIMIT",
            "计划任务读取超过 10 秒安全上限；结果可能不完整。",
            "Task Scheduler",
        ));
    }

    ScheduledTaskInventoryResult {
        checked_at,
        observation,
        items,
        issues,
        limitations: vec![
            "这是某一时刻的任务计划程序只读快照，不是实时监控。".to_string(),
            format!(
                "最多返回 {MAX_TASK_ITEMS} 个任务、访问 {MAX_TASK_FOLDERS} 个文件夹；达到上限、集合计数失败或超过 10 秒时间上限会标记结果不完整。"
            ),
            format!(
                "每个任务最多保留 {MAX_TASK_ACTIONS} 个动作；只读取动作类型和可执行文件路径，不读取 Arguments、WorkingDirectory 或触发器。"
            ),
            "普通用户可能无法读取部分任务/文件夹的定义；这些条目显示为部分/无权限，不会当作正常。"
                .to_string(),
            "不运行、创建、修改、删除、启用或禁用任何计划任务；不执行任务中的程序。".to_string(),
            "不判断任务是否恶意；任务名称、作者或路径本身不能作为恶意结论。".to_string(),
        ],
        truncated,
    }
}

fn read_task(task: &IRegisteredTask) -> TaskReadOutcome {
    let name = match unsafe { task.Name() } {
        Ok(value) => bounded_bstr(&value, MAX_TASK_NAME_CHARS),
        Err(_) => None,
    };
    let path = match unsafe { task.Path() } {
        Ok(value) => bounded_bstr(&value, MAX_TASK_PATH_CHARS),
        Err(_) => None,
    };
    let (Some(name), Some(path)) = (name, path) else {
        return TaskReadOutcome {
            summary: None,
            issue: true,
            denied: false,
        };
    };

    let mut issues = Vec::new();
    let mut observation = InventoryObservation::Observed;
    let mut denied = false;
    let mut definition_available = false;

    let state = match unsafe { task.State() } {
        Ok(value) => map_task_state(value),
        Err(error) => {
            denied |= hresult_code(&error) == ERROR_ACCESS_DENIED;
            observation = InventoryObservation::Partial;
            push_issue(
                &mut issues,
                issue(
                    "TASK-STATE-UNREADABLE",
                    "任务状态读取失败；状态保持未知。",
                    "Task Scheduler",
                ),
            );
            ScheduledTaskState::Unknown
        }
    };
    let enabled = match unsafe { task.Enabled() } {
        Ok(value) => Some(value.0 != 0),
        Err(error) => {
            denied |= hresult_code(&error) == ERROR_ACCESS_DENIED;
            observation = InventoryObservation::Partial;
            push_issue(
                &mut issues,
                issue(
                    "TASK-ENABLED-UNREADABLE",
                    "任务启用状态读取失败；启用状态保持未知。",
                    "Task Scheduler",
                ),
            );
            None
        }
    };

    let mut author = None;
    let mut run_as = None;
    let mut actions = Vec::new();
    let mut actions_truncated = false;

    match unsafe { task.Definition() } {
        Ok(definition) => {
            definition_available = true;
            match unsafe { definition.RegistrationInfo() } {
                Ok(info) => {
                    let mut value = BSTR::new();
                    if unsafe { info.Author(&mut value) }.is_ok() {
                        author = bounded_bstr(&value, MAX_TASK_IDENTITY_CHARS);
                    } else {
                        observation = InventoryObservation::Partial;
                        push_issue(
                            &mut issues,
                            issue(
                                "TASK-AUTHOR-UNREADABLE",
                                "任务作者读取失败；作者保持未知。",
                                "Task Scheduler",
                            ),
                        );
                    }
                }
                Err(_) => {
                    observation = InventoryObservation::Partial;
                    push_issue(
                        &mut issues,
                        issue(
                            "TASK-REGISTRATION-UNREADABLE",
                            "任务注册信息读取失败；作者保持未知。",
                            "Task Scheduler",
                        ),
                    );
                }
            }

            match unsafe { definition.Principal() } {
                Ok(principal) => {
                    let mut user = BSTR::new();
                    let mut group = BSTR::new();
                    let user_read = unsafe { principal.UserId(&mut user) }.is_ok();
                    let group_read = unsafe { principal.GroupId(&mut group) }.is_ok();
                    if user_read {
                        run_as = bounded_bstr(&user, MAX_TASK_IDENTITY_CHARS);
                    }
                    if run_as.is_none() && group_read {
                        run_as = bounded_bstr(&group, MAX_TASK_IDENTITY_CHARS);
                    }
                    if !user_read && !group_read {
                        observation = InventoryObservation::Partial;
                        push_issue(
                            &mut issues,
                            issue(
                                "TASK-PRINCIPAL-UNREADABLE",
                                "任务运行身份读取失败；运行账户保持未知。",
                                "Task Scheduler",
                            ),
                        );
                    }
                }
                Err(_) => {
                    observation = InventoryObservation::Partial;
                    push_issue(
                        &mut issues,
                        issue(
                            "TASK-PRINCIPAL-UNREADABLE",
                            "任务主体读取失败；运行账户保持未知。",
                            "Task Scheduler",
                        ),
                    );
                }
            }

            match unsafe { definition.Actions() } {
                Ok(collection) => {
                    let mut count_value = 0i32;
                    let plan = plan_action_read(
                        unsafe { collection.Count(&mut count_value) }
                            .map(|_| count_value)
                            .map_err(|error| hresult_code(&error)),
                    );
                    if let Some(failure) = apply_action_plan(&plan, &mut observation) {
                        // Keep the count failure even when earlier field issues filled the
                        // per-task issue cap; otherwise an unknown action list could look empty.
                        issues.push(issue(
                            "TASK-ACTIONS-COUNT-UNREADABLE",
                            format!(
                                "任务动作集合计数读取失败（{}）；动作列表保持未知，不当作空列表。",
                                failure.detail()
                            ),
                            "Task Scheduler",
                        ));
                    }
                    if plan.truncated {
                        actions_truncated = true;
                        push_issue(
                            &mut issues,
                            issue(
                                "TASK-ACTIONS-TRUNCATED",
                                format!(
                                    "任务动作超过 {MAX_TASK_ACTIONS} 个安全上限；只保留前 {MAX_TASK_ACTIONS} 个。"
                                ),
                                "Task Scheduler",
                            ),
                        );
                    }
                    for index in 1..=plan.count {
                        match read_action(&collection, index) {
                            Ok(action) => actions.push(action),
                            Err(error) => {
                                observation = InventoryObservation::Partial;
                                let code = hresult_code(&error);
                                denied |= code == ERROR_ACCESS_DENIED;
                                push_issue(
                                    &mut issues,
                                    issue(
                                        "TASK-ACTION-UNREADABLE",
                                        format!(
                                            "一个任务动作读取失败（HRESULT 0x{code:08X}）；该动作保持未知。"
                                        ),
                                        "Task Scheduler",
                                    ),
                                );
                            }
                        }
                    }
                }
                Err(error) => {
                    observation = InventoryObservation::Partial;
                    denied |= hresult_code(&error) == ERROR_ACCESS_DENIED;
                    push_issue(
                        &mut issues,
                        issue(
                            "TASK-ACTIONS-UNREADABLE",
                            "任务动作列表读取失败；动作保持未知。",
                            "Task Scheduler",
                        ),
                    );
                }
            }
        }
        Err(error) => {
            let code = hresult_code(&error);
            denied |= code == ERROR_ACCESS_DENIED;
            observation = if code == ERROR_ACCESS_DENIED {
                InventoryObservation::AccessDenied
            } else {
                InventoryObservation::Partial
            };
            push_issue(
                &mut issues,
                issue(
                    "TASK-DEFINITION-UNREADABLE",
                    if code == ERROR_ACCESS_DENIED {
                        "没有权限读取任务定义；作者、运行身份和动作保持未知。"
                    } else {
                        "任务定义读取失败；作者、运行身份和动作保持未知。"
                    },
                    "Task Scheduler",
                ),
            );
        }
    }

    if definition_available
        && author.is_none()
        && run_as.is_none()
        && actions.is_empty()
        && !actions_truncated
        && observation == InventoryObservation::Observed
    {
        observation = InventoryObservation::Partial;
        push_issue(
            &mut issues,
            issue(
                "TASK-DEFINITION-EMPTY",
                "任务定义没有提供可显示的作者、运行身份或动作；字段保持未知。",
                "Task Scheduler",
            ),
        );
    }

    TaskReadOutcome {
        summary: Some(ScheduledTaskSummary {
            id: format!("task:{path}"),
            name,
            path,
            state,
            enabled,
            author,
            run_as,
            actions,
            actions_truncated,
            observation,
            issues,
        }),
        issue: !matches!(
            observation,
            InventoryObservation::Observed | InventoryObservation::AccessDenied
        ) || denied,
        denied,
    }
}

fn read_action(
    collection: &IActionCollection,
    index: usize,
) -> Result<ScheduledTaskAction, windows::core::Error> {
    let action = unsafe { collection.get_Item(index as i32) }?;
    let mut action_type = windows::Win32::System::TaskScheduler::TASK_ACTION_TYPE(0);
    unsafe { action.Type(&mut action_type) }?;
    let mapped_type = map_action_type(action_type);
    let executable_path = if mapped_type == ScheduledTaskActionType::Exec {
        let exec: IExecAction = action.cast()?;
        let mut value = BSTR::new();
        unsafe { exec.Path(&mut value) }?;
        bounded_path(&value.to_string(), MAX_TASK_ACTION_PATH_CHARS)
    } else {
        None
    };
    Ok(ScheduledTaskAction {
        action_type: mapped_type,
        executable_path,
    })
}

fn map_task_state(value: windows::Win32::System::TaskScheduler::TASK_STATE) -> ScheduledTaskState {
    match value {
        TASK_STATE_DISABLED => ScheduledTaskState::Disabled,
        TASK_STATE_QUEUED => ScheduledTaskState::Queued,
        TASK_STATE_READY => ScheduledTaskState::Ready,
        TASK_STATE_RUNNING => ScheduledTaskState::Running,
        _ => ScheduledTaskState::Unknown,
    }
}

fn map_action_type(
    value: windows::Win32::System::TaskScheduler::TASK_ACTION_TYPE,
) -> ScheduledTaskActionType {
    match value {
        TASK_ACTION_EXEC => ScheduledTaskActionType::Exec,
        TASK_ACTION_COM_HANDLER => ScheduledTaskActionType::ComHandler,
        TASK_ACTION_SEND_EMAIL => ScheduledTaskActionType::Email,
        TASK_ACTION_SHOW_MESSAGE => ScheduledTaskActionType::ShowMessage,
        _ => ScheduledTaskActionType::Unknown,
    }
}

fn variant_index(index: i32) -> VARIANT {
    let mut variant = VARIANT::default();
    unsafe {
        let inner = &mut *variant.Anonymous.Anonymous;
        inner.vt = VT_I4;
        inner.Anonymous.lVal = index;
    }
    variant
}

fn bounded_bstr(value: &BSTR, max_chars: usize) -> Option<String> {
    bounded_clean(&value.to_string(), max_chars)
}

fn bounded_path(value: &str, max_chars: usize) -> Option<String> {
    let trimmed = value.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(trimmed);
    bounded_clean(unquoted, max_chars)
}

fn bounded_clean(value: &str, max_chars: usize) -> Option<String> {
    let cleaned: String = value
        .chars()
        .filter(|character| !character.is_control())
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(bound_text(trimmed, max_chars).value)
    }
}

fn push_issue(issues: &mut Vec<DataIssue>, issue: DataIssue) {
    if issues.len() < 4 {
        issues.push(issue);
    }
}

fn issue(code: &str, message: impl Into<String>, source: &str) -> DataIssue {
    DataIssue {
        code: code.to_string(),
        message: message.into(),
        source: source.to_string(),
    }
}

fn hresult_code(error: &windows::core::Error) -> u32 {
    error.code().0 as u32
}

fn observation_for_code(code: u32) -> InventoryObservation {
    match code & 0xFFFF {
        ERROR_ACCESS_DENIED => InventoryObservation::AccessDenied,
        ERROR_SERVICE_DISABLED | ERROR_SERVICE_NOT_ACTIVE | ERROR_SERVICE_DOES_NOT_EXIST => {
            InventoryObservation::Unavailable
        }
        _ => InventoryObservation::Error,
    }
}

fn failed_result(
    checked_at: String,
    observation: InventoryObservation,
    issues: Vec<DataIssue>,
) -> ScheduledTaskInventoryResult {
    ScheduledTaskInventoryResult {
        checked_at,
        observation,
        items: Vec::new(),
        issues,
        limitations: vec![
            "计划任务列表不可读；空结果不代表没有计划任务。".to_string(),
            "不运行、创建、修改、删除、启用或禁用任何计划任务。".to_string(),
        ],
        truncated: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::TaskScheduler::{TASK_ACTION_TYPE, TASK_STATE, TASK_STATE_UNKNOWN};

    #[test]
    fn task_state_mapping_keeps_unknown_distinct() {
        assert_eq!(map_task_state(TASK_STATE_READY), ScheduledTaskState::Ready);
        assert_eq!(
            map_task_state(TASK_STATE_DISABLED),
            ScheduledTaskState::Disabled
        );
        assert_eq!(
            map_task_state(TASK_STATE_UNKNOWN),
            ScheduledTaskState::Unknown
        );
        assert_eq!(map_task_state(TASK_STATE(99)), ScheduledTaskState::Unknown);
    }

    #[test]
    fn action_type_mapping_covers_known_and_unknown_values() {
        assert_eq!(
            map_action_type(TASK_ACTION_EXEC),
            ScheduledTaskActionType::Exec
        );
        assert_eq!(
            map_action_type(TASK_ACTION_COM_HANDLER),
            ScheduledTaskActionType::ComHandler
        );
        assert_eq!(
            map_action_type(TASK_ACTION_SEND_EMAIL),
            ScheduledTaskActionType::Email
        );
        assert_eq!(
            map_action_type(TASK_ACTION_SHOW_MESSAGE),
            ScheduledTaskActionType::ShowMessage
        );
        assert_eq!(
            map_action_type(TASK_ACTION_TYPE(99)),
            ScheduledTaskActionType::Unknown
        );
    }

    #[test]
    fn task_error_mapping_distinguishes_access_denied_and_unavailable() {
        assert_eq!(
            observation_for_code(0x8007_0005),
            InventoryObservation::AccessDenied
        );
        assert_eq!(
            observation_for_code(0x8007_0424),
            InventoryObservation::Unavailable
        );
        assert_eq!(
            observation_for_code(0x8007_0426),
            InventoryObservation::Unavailable
        );
        assert_eq!(
            observation_for_code(0x8000_4000),
            InventoryObservation::Error
        );
    }

    #[test]
    fn bounded_clean_drops_controls_and_empty_values() {
        assert_eq!(
            bounded_clean("  Microsoft\\Update  ", MAX_TASK_NAME_CHARS).as_deref(),
            Some("Microsoft\\Update")
        );
        assert_eq!(bounded_clean("\r\n\t", MAX_TASK_NAME_CHARS), None);
        assert_eq!(bounded_clean("", MAX_TASK_NAME_CHARS), None);
    }

    #[test]
    fn bounded_path_removes_only_wrapping_quotes() {
        assert_eq!(
            bounded_path(
                r#""C:\Program Files\Vendor\app.exe""#,
                MAX_TASK_ACTION_PATH_CHARS
            )
            .as_deref(),
            Some(r"C:\Program Files\Vendor\app.exe")
        );
        assert_eq!(
            bounded_path(
                r"C:\Program Files\Vendor\app.exe",
                MAX_TASK_ACTION_PATH_CHARS
            )
            .as_deref(),
            Some(r"C:\Program Files\Vendor\app.exe")
        );
        assert_eq!(bounded_path("\"\"", MAX_TASK_ACTION_PATH_CHARS), None);
    }

    #[test]
    fn task_collection_count_failure_is_kept_visible() {
        let plan = plan_task_read(Err(0x8007_0005), MAX_TASK_ITEMS);
        let mut observation = InventoryObservation::Observed;
        let mut truncated = false;
        let failure = apply_bounded_plan(&plan, &mut observation, &mut truncated);

        assert_eq!(plan.limit, 0);
        assert_eq!(observation, InventoryObservation::Partial);
        assert!(truncated);
        assert_eq!(failure, Some(CountFailure::HResult(0x8007_0005)));
    }

    #[test]
    fn folder_collection_count_failure_is_kept_visible() {
        let plan = plan_folder_read(Err(0x8007_0005), MAX_TASK_FOLDERS - 1);
        let mut observation = InventoryObservation::Observed;
        let mut truncated = false;
        let failure = apply_bounded_plan(&plan, &mut observation, &mut truncated);

        assert_eq!(plan.limit, 0);
        assert_eq!(observation, InventoryObservation::Partial);
        assert!(truncated);
        assert_eq!(failure, Some(CountFailure::HResult(0x8007_0005)));
    }

    #[test]
    fn action_collection_count_failure_keeps_task_partial_without_faking_truncation() {
        let plan = plan_action_read(Err(0x8007_0005));
        let mut observation = InventoryObservation::Observed;
        let failure = apply_action_plan(&plan, &mut observation);

        assert_eq!(plan.count, 0);
        assert!(!plan.truncated);
        assert_eq!(observation, InventoryObservation::Partial);
        assert_eq!(failure, Some(CountFailure::HResult(0x8007_0005)));
    }

    #[test]
    fn task_count_below_exact_and_above_limit_is_distinguished() {
        let below = plan_task_read(Ok((MAX_TASK_ITEMS - 1) as i32), MAX_TASK_ITEMS);
        let mut below_observation = InventoryObservation::Observed;
        let mut below_truncated = false;
        apply_bounded_plan(&below, &mut below_observation, &mut below_truncated);
        assert_eq!(below.limit, MAX_TASK_ITEMS - 1);
        assert_eq!(below_observation, InventoryObservation::Observed);
        assert!(!below_truncated);
        assert_eq!(below.failure, None);

        let exact = plan_task_read(Ok(MAX_TASK_ITEMS as i32), MAX_TASK_ITEMS);
        let mut exact_observation = InventoryObservation::Observed;
        let mut exact_truncated = false;
        apply_bounded_plan(&exact, &mut exact_observation, &mut exact_truncated);
        assert_eq!(exact.limit, MAX_TASK_ITEMS);
        assert_eq!(exact_observation, InventoryObservation::Observed);
        assert!(!exact_truncated);
        assert_eq!(exact.failure, None);

        let above = plan_task_read(Ok((MAX_TASK_ITEMS + 1) as i32), MAX_TASK_ITEMS);
        let mut above_observation = InventoryObservation::Observed;
        let mut above_truncated = false;
        apply_bounded_plan(&above, &mut above_observation, &mut above_truncated);
        assert_eq!(above.limit, MAX_TASK_ITEMS);
        assert_eq!(above_observation, InventoryObservation::Partial);
        assert!(above_truncated);
        assert_eq!(above.failure, None);
    }

    #[test]
    fn task_count_after_capacity_is_confirmed_as_truncated() {
        let plan = plan_task_read(Ok(1), 0);
        let mut observation = InventoryObservation::Observed;
        let mut truncated = false;
        apply_bounded_plan(&plan, &mut observation, &mut truncated);

        assert_eq!(plan.limit, 0);
        assert_eq!(observation, InventoryObservation::Partial);
        assert!(truncated);
        assert_eq!(plan.failure, None);
    }

    #[test]
    fn folder_count_at_and_above_remaining_capacity_is_distinguished() {
        let exact = plan_folder_read(Ok((MAX_TASK_FOLDERS - 1) as i32), MAX_TASK_FOLDERS - 1);
        let mut exact_observation = InventoryObservation::Observed;
        let mut exact_truncated = false;
        apply_bounded_plan(&exact, &mut exact_observation, &mut exact_truncated);
        assert_eq!(exact.limit, MAX_TASK_FOLDERS - 1);
        assert_eq!(exact_observation, InventoryObservation::Observed);
        assert!(!exact_truncated);

        let above = plan_folder_read(Ok(MAX_TASK_FOLDERS as i32), MAX_TASK_FOLDERS - 1);
        let mut above_observation = InventoryObservation::Observed;
        let mut above_truncated = false;
        apply_bounded_plan(&above, &mut above_observation, &mut above_truncated);
        assert_eq!(above.limit, MAX_TASK_FOLDERS - 1);
        assert_eq!(above_observation, InventoryObservation::Partial);
        assert!(above_truncated);
    }

    #[test]
    fn action_count_over_limit_and_normal_action_list_are_complete() {
        let normal = plan_action_read(Ok(2));
        let mut normal_observation = InventoryObservation::Observed;
        apply_action_plan(&normal, &mut normal_observation);
        assert_eq!(normal.count, 2);
        assert!(!normal.truncated);
        assert_eq!(normal_observation, InventoryObservation::Observed);
        assert_eq!(normal.failure, None);

        let above = plan_action_read(Ok((MAX_TASK_ACTIONS + 1) as i32));
        let mut above_observation = InventoryObservation::Observed;
        apply_action_plan(&above, &mut above_observation);
        assert_eq!(above.count, MAX_TASK_ACTIONS);
        assert!(above.truncated);
        assert_eq!(above_observation, InventoryObservation::Partial);
        assert_eq!(above.failure, None);
    }

    #[test]
    fn empty_task_collection_is_not_marked_partial_or_truncated() {
        let plan = plan_task_read(Ok(0), MAX_TASK_ITEMS);
        let mut observation = InventoryObservation::Observed;
        let mut truncated = false;
        let failure = apply_bounded_plan(&plan, &mut observation, &mut truncated);

        assert_eq!(plan.limit, 0);
        assert_eq!(observation, InventoryObservation::Observed);
        assert!(!truncated);
        assert_eq!(failure, None);
    }
}
