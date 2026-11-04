import type {
  AssociationCertainty,
  AssociationEvidenceType,
  AssociationMatchMethod,
  AssociationSource,
  AssociationTargetKind,
  InstallScope,
  InventoryObservation,
  PathState,
  ProcessPathAccess,
  RegistryValueState,
  RegistryView,
  ScheduledTaskActionType,
  ScheduledTaskState,
  ServiceStartType,
  ServiceState,
  ServiceType,
  SignatureState,
  SoftwareAssociationState,
  SoftwareClassification,
  StartupPathResolution,
  StartupTargetKind,
  StartupValueType,
} from "../types";

export function installScopeLabel(scope: InstallScope): string {
  return scope === "machine" ? "本机范围" : "当前用户";
}

export function registryViewLabel(view: RegistryView | null): string {
  if (view === null) {
    return "无注册表视图";
  }
  if (view === "shared") {
    return "共享用户视图";
  }
  return view === "registry64" ? "64 位视图" : "32 位视图";
}

export function pathStateLabel(state: PathState): string {
  switch (state) {
    case "present":
      return "路径存在";
    case "missing":
      return "路径不存在";
    case "accessDenied":
      return "路径访问被拒绝（存在性未知）";
    case "unknown":
      return "路径状态未知";
  }
}

export function startupPathResolutionLabel(resolution: StartupPathResolution): string {
  switch (resolution) {
    case "quotedExecutable":
      return "带引号的可执行路径";
    case "unquotedExecutable":
      return "未加引号的可执行路径";
    case "ambiguousUnquoted":
      return "未加引号的歧义路径（未探测）";
    case "startupFolderFile":
      return "启动文件夹中的文件";
    case "shortcutTargetResolved":
      return "快捷方式目标已解析";
    case "shortcutTargetUnresolved":
      return "快捷方式目标未解析";
    case "unresolved":
      return "无法解析目标";
  }
}

export function startupTargetKindLabel(kind: StartupTargetKind): string {
  switch (kind) {
    case "registryCommand":
      return "注册表命令";
    case "startupFolderFile":
      return "启动文件夹文件";
    case "shortcut":
      return "快捷方式";
  }
}

export function startupValueTypeLabel(valueType: StartupValueType): string {
  switch (valueType) {
    case "regSz":
      return "REG_SZ";
    case "regExpandSz":
      return "REG_EXPAND_SZ";
    case "regMultiSz":
      return "REG_MULTI_SZ";
    case "regDword":
      return "REG_DWORD";
    case "regQword":
      return "REG_QWORD";
    case "regBinary":
      return "REG_BINARY";
    case "regNone":
      return "REG_NONE";
    case "other":
      return "其他类型";
  }
}

export function registryValueStateLabel(state: RegistryValueState): string {
  switch (state) {
    case "present":
      return "已读取";
    case "missing":
      return "未提供";
    case "typeMismatch":
      return "类型不匹配";
    case "invalid":
      return "值无效";
    case "readError":
      return "读取失败";
  }
}

export function softwareClassificationLabel(classification: SoftwareClassification): string {
  switch (classification) {
    case "userSoftware":
      return "普通用户软件";
    case "systemComponent":
      return "系统组件";
    case "driverOrHardwareComponent":
      return "驱动或硬件组件";
    case "runtimeOrSharedComponent":
      return "运行时或共享组件";
    case "securitySoftware":
      return "安全软件组件";
    case "unknown":
      return "类别未知";
  }
}

export function processPathAccessLabel(access: ProcessPathAccess): string {
  switch (access) {
    case "available":
      return "路径可读取";
    case "denied":
      return "路径访问被拒绝";
    case "unavailable":
      return "路径不可用";
  }
}

export function inventoryObservationLabel(state: InventoryObservation): string {
  switch (state) {
    case "observed":
      return "已读取";
    case "partial":
      return "部分读取";
    case "unavailable":
      return "不可用";
    case "accessDenied":
      return "无权限";
    case "error":
      return "查询失败";
  }
}

export function associationSourceLabel(source: AssociationSource): string {
  switch (source) {
    case "softwareInventory":
      return "软件盘点";
    case "processSnapshot":
      return "进程快照";
    case "serviceInventory":
      return "服务盘点";
    case "scheduledTaskInventory":
      return "计划任务盘点";
    case "startupInventory":
      return "启动项盘点";
  }
}

export function associationTargetKindLabel(kind: AssociationTargetKind): string {
  switch (kind) {
    case "process":
      return "运行进程";
    case "service":
      return "Windows 服务";
    case "scheduledTask":
      return "计划任务";
    case "startupItem":
      return "启动项";
  }
}

export function associationTargetSourceLabel(kind: AssociationTargetKind): string {
  switch (kind) {
    case "process":
      return "进程快照";
    case "service":
      return "服务盘点";
    case "scheduledTask":
      return "计划任务盘点";
    case "startupItem":
      return "启动项盘点";
  }
}

export function associationEvidenceTypeLabel(
  evidenceType: AssociationEvidenceType,
): string {
  switch (evidenceType) {
    case "displayIconPath":
      return "DisplayIcon 路径";
    case "installLocationPath":
      return "安装位置路径";
    case "executableFileName":
      return "可执行文件名";
  }
}

export function associationCertaintyLabel(certainty: AssociationCertainty): string {
  switch (certainty) {
    case "exact":
      return "精确路径";
    case "strong":
      return "安装目录内路径";
    case "weak":
      return "同名文件弱线索";
  }
}

export function associationMatchMethodLabel(method: AssociationMatchMethod): string {
  switch (method) {
    case "exactPath":
      return "规范化路径完全一致";
    case "containedInInstallLocation":
      return "目标位于软件安装目录之下";
    case "sameFileNameDifferentDirectory":
      return "文件名相同但目录不同";
  }
}

export function softwareAssociationStateLabel(state: SoftwareAssociationState): string {
  switch (state) {
    case "linked":
      return "存在路径关联";
    case "weakOnly":
      return "仅弱线索";
    case "noMatch":
      return "当前快照未匹配";
    case "insufficientEvidence":
      return "证据不足";
    case "sourceUnavailable":
      return "关联来源不可用";
  }
}

export function postureReadStateLabel(
  state: "notRead" | "complete" | "partial" | "unavailable",
): string {
  switch (state) {
    case "notRead":
      return "尚未读取";
    case "complete":
      return "来源已读取";
    case "partial":
      return "部分读取";
    case "unavailable":
      return "来源不可用";
  }
}

export function postureTriageCategoryLabel(
  category: "observed" | "attention" | "insufficient" | "unavailable",
): string {
  switch (category) {
    case "observed":
      return "已观察事实";
    case "attention":
      return "值得核实";
    case "insufficient":
      return "证据不足";
    case "unavailable":
      return "来源不可用";
  }
}

export function serviceStateLabel(state: ServiceState): string {
  switch (state) {
    case "stopped":
      return "已停止";
    case "startPending":
      return "正在启动";
    case "stopPending":
      return "正在停止";
    case "running":
      return "运行中";
    case "continuePending":
      return "正在继续";
    case "pausePending":
      return "正在暂停";
    case "paused":
      return "已暂停";
    case "unknown":
      return "状态未知";
  }
}

export function serviceStartTypeLabel(startType: ServiceStartType): string {
  switch (startType) {
    case "boot":
      return "引导启动";
    case "system":
      return "系统启动";
    case "auto":
      return "自动";
    case "demand":
      return "手动";
    case "disabled":
      return "已禁用";
    case "unknown":
      return "启动类型未知";
  }
}

export function serviceTypeLabel(serviceType: ServiceType): string {
  switch (serviceType) {
    case "kernelDriver":
      return "内核驱动";
    case "fileSystemDriver":
      return "文件系统驱动";
    case "adapter":
      return "适配器";
    case "recognizerDriver":
      return "识别器驱动";
    case "ownProcess":
      return "独立进程";
    case "shareProcess":
      return "共享进程";
    case "interactiveProcess":
      return "交互式进程";
    case "other":
      return "其他类型";
  }
}

export function scheduledTaskStateLabel(state: ScheduledTaskState): string {
  switch (state) {
    case "unknown":
      return "状态未知";
    case "disabled":
      return "已禁用";
    case "queued":
      return "已排队";
    case "ready":
      return "就绪";
    case "running":
      return "运行中";
  }
}

export function scheduledTaskActionTypeLabel(actionType: ScheduledTaskActionType): string {
  switch (actionType) {
    case "exec":
      return "启动程序";
    case "comHandler":
      return "COM 处理程序";
    case "email":
      return "发送邮件";
    case "showMessage":
      return "显示消息";
    case "unknown":
      return "未知动作";
  }
}

export function signatureStateLabel(state: SignatureState): string {
  switch (state) {
    case "valid":
      return "签名有效";
    case "unsigned":
      return "未签名";
    case "invalid":
      return "签名无效";
    case "untrusted":
      return "签名不受信任";
    case "unavailable":
      return "签名状态未知";
  }
}

export function inspectionStateLabel(
  state: "complete" | "partial" | "unavailable",
): string {
  switch (state) {
    case "complete":
      return "检查完整";
    case "partial":
      return "部分数据不可用";
    case "unavailable":
      return "无法完成检查";
  }
}
