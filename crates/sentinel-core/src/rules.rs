use crate::model::{
    Confidence, Evidence, PathState, RiskAssessment, RiskLevel, RiskScope, SoftwareClassification,
    StartupPathResolution,
};
use crate::util::{is_temp_path, mask_to_hex};

const RULE_SOURCE: &str = "SentinelLite 本地证据规则";

#[derive(Debug, Clone, Copy)]
pub struct SoftwareFacts {
    pub uninstall_command_present: bool,
    pub uninstall_command_known: bool,
    pub install_location_state: PathState,
    pub classification: SoftwareClassification,
}

#[derive(Debug, Clone, Copy)]
pub struct StartupFacts<'a> {
    pub target_path: Option<&'a str>,
    pub target_state: PathState,
    pub path_resolution: StartupPathResolution,
}

#[derive(Debug, Clone, Copy)]
pub struct AffinityFacts {
    pub restricted: Option<bool>,
    pub process_mask: Option<u64>,
    pub system_mask: Option<u64>,
}

fn evidence(code: &str, statement: impl Into<String>) -> Evidence {
    Evidence {
        code: code.to_string(),
        statement: statement.into(),
        source: RULE_SOURCE.to_string(),
    }
}

fn classification_label(classification: SoftwareClassification) -> &'static str {
    match classification {
        SoftwareClassification::UserSoftware => "普通用户软件条目",
        SoftwareClassification::SystemComponent => "系统组件或不可移除组件",
        SoftwareClassification::DriverOrHardwareComponent => "驱动或硬件组件",
        SoftwareClassification::RuntimeOrSharedComponent => "运行时或共享组件",
        SoftwareClassification::SecuritySoftware => "安全软件组件",
        SoftwareClassification::Unknown => "类别未知的组件或更新条目",
    }
}

fn component_awareness_blind_spots() -> Vec<String> {
    vec![
        "分类只依据 Uninstall 注册表字段，未读取驱动存储、服务、共享运行库或软件关联。".to_string(),
        "未检查文件签名、行为、网络访问和卸载残留；分类本身不是安全结论。".to_string(),
    ]
}

pub fn assess_software(facts: SoftwareFacts) -> RiskAssessment {
    match facts.classification {
        SoftwareClassification::SystemComponent
        | SoftwareClassification::DriverOrHardwareComponent
        | SoftwareClassification::RuntimeOrSharedComponent
        | SoftwareClassification::SecuritySoftware => RiskAssessment {
            scope: RiskScope::SoftwareMetadata,
            level: RiskLevel::NoClearIssue,
            confidence: Confidence::Low,
            rule_ids: vec!["INV-COMPONENT-000".to_string()],
            summary: format!(
                "已识别为{}；未按普通用户软件处理，缺少卸载信息不构成配置异常。",
                classification_label(facts.classification)
            ),
            evidence: vec![evidence(
                "INV-CLASSIFICATION",
                format!(
                    "系统组件/组件类分类：{}。安装路径状态：{}。",
                    classification_label(facts.classification),
                    path_state_label(facts.install_location_state)
                ),
            )],
            blind_spots: component_awareness_blind_spots(),
            recommendation: "不要把它当作普通卸载目标；如无独立证据，不进行删除、卸载或驱动处理。"
                .to_string(),
        },
        SoftwareClassification::Unknown => RiskAssessment {
            scope: RiskScope::SoftwareMetadata,
            level: RiskLevel::InsufficientEvidence,
            confidence: Confidence::Unknown,
            rule_ids: vec!["INV-COMPONENT-UNKNOWN".to_string()],
            summary: "存在组件或更新元数据，但当前字段不足以确认是普通软件、运行时还是驱动。"
                .to_string(),
            evidence: vec![evidence(
                "INV-CLASSIFICATION-UNKNOWN",
                format!(
                    "ParentKeyName 或 ReleaseType 等组件/更新字段存在，但没有足够证据确定具体类别。安装路径状态：{}。",
                    path_state_label(facts.install_location_state)
                ),
            )],
            blind_spots: component_awareness_blind_spots(),
            recommendation:
                "保持类别未知，先核实父组件、发布者和安装上下文；不要按品牌或文件夹名猜测。"
                    .to_string(),
        },
        SoftwareClassification::UserSoftware => assess_user_software(facts),
    }
}

fn assess_user_software(facts: SoftwareFacts) -> RiskAssessment {
    match facts.install_location_state {
        PathState::AccessDenied => RiskAssessment {
            scope: RiskScope::SoftwareMetadata,
            level: RiskLevel::InsufficientEvidence,
            confidence: Confidence::Unknown,
            rule_ids: vec!["INV-003".to_string()],
            summary: "安装路径访问被拒绝，当前不能确认路径是否存在，也不能据此形成配置异常结论。"
                .to_string(),
            evidence: vec![evidence(
                "INV-PATH-ACCESS-DENIED",
                "读取安装位置时收到访问被拒绝；这不是文件不存在的证据。",
            )],
            blind_spots: vec![
                "未提升权限绕过访问限制。".to_string(),
                "未检查文件签名、行为和卸载残留。".to_string(),
            ],
            recommendation: "保持未知；不要因访问被拒绝执行删除或清洗。".to_string(),
        },
        PathState::Unknown => RiskAssessment {
            scope: RiskScope::SoftwareMetadata,
            level: RiskLevel::InsufficientEvidence,
            confidence: Confidence::Unknown,
            rule_ids: vec!["INV-004".to_string()],
            summary: "安装路径状态未能核验，元数据结论保持证据不足。".to_string(),
            evidence: vec![evidence(
                "INV-PATH-UNVERIFIED",
                "安装路径无法安全探测，可能是网络、设备、相对路径或其他访问限制。",
            )],
            blind_spots: vec![
                "未访问无法安全核验的路径。".to_string(),
                "未检查文件签名、行为和卸载残留。".to_string(),
            ],
            recommendation: "保持未知；不要据此判定为残留或配置异常。".to_string(),
        },
        PathState::Missing => {
            let mut rule_ids = vec!["INV-002".to_string()];
            let mut evidence_items = vec![evidence(
                "INV-PATH-MISSING",
                "安装路径明确返回不存在。路径记录可能过期，也可能在安装后被移动。",
            )];
            if facts.uninstall_command_known && !facts.uninstall_command_present {
                rule_ids.push("INV-001".to_string());
                evidence_items.push(evidence(
                    "INV-NO-UNINSTALL",
                    "卸载注册项没有提供可读取的卸载命令。",
                ));
            }
            RiskAssessment {
                scope: RiskScope::SoftwareMetadata,
                level: RiskLevel::ConfigurationAnomaly,
                confidence: Confidence::Medium,
                rule_ids,
                summary: "普通用户软件条目的安装路径明确缺失或没有卸载信息，原因尚未确认。"
                    .to_string(),
                evidence: evidence_items,
                blind_spots: vec![
                    "未检查文件签名、实际行为、网络访问或安装来源。".to_string(),
                    "路径记录过期或软件被移动都可能造成缺失，不能据此认定为恶意残留。".to_string(),
                ],
                recommendation: "先核实软件来源与当前安装状态，不依据单一元数据规则执行删除。"
                    .to_string(),
            }
        }
        PathState::Present => {
            if !facts.uninstall_command_known {
                return RiskAssessment {
                    scope: RiskScope::SoftwareMetadata,
                    level: RiskLevel::InsufficientEvidence,
                    confidence: Confidence::Unknown,
                    rule_ids: vec!["INV-005".to_string()],
                    summary: "卸载入口读取失败，当前不能确认是否存在；元数据结论保持证据不足。"
                        .to_string(),
                    evidence: vec![evidence(
                        "INV-UNINSTALL-UNKNOWN",
                        "UninstallString 或 QuietUninstallString 的类型异常或读取失败，未按缺失处理。",
                    )],
                    blind_spots: vec![
                        "未提升权限绕过访问限制。".to_string(),
                        "未检查文件签名、行为和卸载残留。".to_string(),
                    ],
                    recommendation: "保持未知；不要因卸载入口不可读执行删除或清洗。".to_string(),
                };
            }

            if !facts.uninstall_command_present {
                return RiskAssessment {
                    scope: RiskScope::SoftwareMetadata,
                    level: RiskLevel::ConfigurationAnomaly,
                    confidence: Confidence::Medium,
                    rule_ids: vec!["INV-001".to_string()],
                    summary: "普通用户软件条目没有提供可读取的卸载命令，原因尚未确认。".to_string(),
                    evidence: vec![evidence(
                        "INV-NO-UNINSTALL",
                        "卸载注册项没有提供可读取的卸载命令，可能来自组件、条目损坏或权限限制。",
                    )],
                    blind_spots: vec![
                        "未检查文件签名、实际行为、网络访问或安装来源。".to_string(),
                        "该异常不等同于恶意软件。".to_string(),
                    ],
                    recommendation: "先核实软件来源与卸载入口，不依据单一元数据规则执行删除。"
                        .to_string(),
                };
            }

            RiskAssessment {
                scope: RiskScope::SoftwareMetadata,
                level: RiskLevel::NoClearIssue,
                confidence: Confidence::Low,
                rule_ids: vec!["INV-000".to_string()],
                summary: "在普通用户软件元数据的检查范围内未发现明确配置异常。".to_string(),
                evidence: vec![evidence(
                    "INV-METADATA-READ",
                    "已读取到软件名称、安装路径状态和卸载入口。",
                )],
                blind_spots: vec![
                    "未检查文件签名、行为、网络连接、安装来源和卸载残留。".to_string(),
                    "没有发现异常不代表已经证明软件安全。".to_string(),
                ],
                recommendation:
                    "结合发布者、安装来源和实际使用情况人工判断；不要仅凭本结果采取清洗动作。"
                        .to_string(),
            }
        }
    }
}

pub fn assess_startup(facts: StartupFacts<'_>) -> RiskAssessment {
    if facts.path_resolution == StartupPathResolution::AmbiguousUnquoted {
        return RiskAssessment {
            scope: RiskScope::StartupMetadata,
            level: RiskLevel::InsufficientEvidence,
            confidence: Confidence::Unknown,
            rule_ids: vec!["STARTUP-005".to_string()],
            summary:
                "启动命令包含未加引号的路径，Windows 可能以不同方式解释可执行文件；未进行存在性判断。"
                    .to_string(),
            evidence: vec![evidence(
                "STARTUP-UNQUOTED-PATH",
                "命令未被完整引号包围，无法安全确定可执行文件边界；这不是文件不存在的证据。",
            )],
            blind_spots: vec![
                "未执行命令解析器或探测歧义候选路径。".to_string(),
                "未核验 Windows 启动审批、签名和实际执行。".to_string(),
            ],
            recommendation: "先核实命令来源和真实可执行路径；不要把歧义路径当作缺失文件。"
                .to_string(),
        };
    }

    if facts.path_resolution == StartupPathResolution::ShortcutTargetUnresolved
        || facts.path_resolution == StartupPathResolution::Unresolved
        || facts.target_path.is_none()
    {
        return RiskAssessment {
            scope: RiskScope::StartupMetadata,
            level: RiskLevel::InsufficientEvidence,
            confidence: Confidence::Unknown,
            rule_ids: vec!["STARTUP-001".to_string()],
            summary: "启动命令存在，但目标文件无法解析，当前不能判断风险或安全。".to_string(),
            evidence: vec![evidence(
                "STARTUP-TARGET-UNRESOLVED",
                "命令无法被安全解析为明确的文件路径，可能是快捷方式、环境变量或非标准命令。",
            )],
            blind_spots: vec![
                "未解析快捷方式目标、脚本内容或命令解释器行为。".to_string(),
                "未核验 Windows 启动审批状态、签名和实际执行结果。".to_string(),
            ],
            recommendation: "先人工核实命令来源与目标，不要把无法解析直接当成恶意。".to_string(),
        };
    }

    let target_path = facts.target_path.unwrap_or_default();

    match facts.target_state {
        PathState::AccessDenied => RiskAssessment {
            scope: RiskScope::StartupMetadata,
            level: RiskLevel::InsufficientEvidence,
            confidence: Confidence::Unknown,
            rule_ids: vec!["STARTUP-006".to_string()],
            summary: "启动项目标路径访问被拒绝，文件是否存在保持未知；当前为证据不足。".to_string(),
            evidence: vec![evidence(
                "STARTUP-TARGET-ACCESS-DENIED",
                format!("目标路径“{target_path}”访问被拒绝，存在性保持未知。"),
            )],
            blind_spots: vec![
                "未通过提权绕过访问限制。".to_string(),
                "未核验启动审批状态、签名和运行行为。".to_string(),
            ],
            recommendation: "保持未知；不要因访问被拒绝删除或禁用启动项。".to_string(),
        },
        PathState::Unknown => RiskAssessment {
            scope: RiskScope::StartupMetadata,
            level: RiskLevel::InsufficientEvidence,
            confidence: Confidence::Unknown,
            rule_ids: vec!["STARTUP-004".to_string()],
            summary: "已解析目标路径，但无法核验目标状态。".to_string(),
            evidence: vec![evidence(
                "STARTUP-EXISTENCE-UNKNOWN",
                format!("无法确认“{target_path}”的状态，可能是权限或文件系统访问限制。"),
            )],
            blind_spots: vec![
                "未核验启动审批状态、数字签名和目标可访问性。".to_string(),
                "该状态不能作为安全或恶意结论。".to_string(),
            ],
            recommendation: "在获得可验证信息前保持未知状态，不执行清洗或禁用。".to_string(),
        },
        PathState::Missing => RiskAssessment {
            scope: RiskScope::StartupMetadata,
            level: RiskLevel::ConfigurationAnomaly,
            confidence: Confidence::Medium,
            rule_ids: vec!["STARTUP-002".to_string()],
            summary: "启动项指向的目标明确不存在。".to_string(),
            evidence: vec![
                evidence(
                    "STARTUP-TARGET-MISSING",
                    format!("目标路径“{target_path}”当前不存在。"),
                ),
                evidence(
                    "STARTUP-CAUSE-UNKNOWN",
                    "残留启动项、延迟安装或失效路径都可能产生这一现象。",
                ),
            ],
            blind_spots: vec![
                "未核验启动审批状态、文件签名、引用来源和运行行为。".to_string(),
                "目标缺失不等同于恶意。".to_string(),
            ],
            recommendation: "核实残留来源和用户意图，再考虑后续可逆处理。".to_string(),
        },
        PathState::Present if is_temp_path(target_path) => RiskAssessment {
            scope: RiskScope::StartupMetadata,
            level: RiskLevel::PotentialUnwanted,
            confidence: Confidence::Low,
            rule_ids: vec!["STARTUP-003".to_string()],
            summary: "启动项目标位于临时目录，来源和意图需要人工核实。".to_string(),
            evidence: vec![
                evidence(
                    "STARTUP-TEMP-PATH",
                    format!("目标路径“{target_path}”位于临时目录或系统临时目录。"),
                ),
                evidence(
                    "STARTUP-LOW-CONFIDENCE",
                    "临时目录可以存放安装器、更新组件或用户程序，因此单一路径特征不足以确认恶意。",
                ),
            ],
            blind_spots: vec![
                "未核验 Windows 启动审批状态、数字签名、父进程和实际运行行为。".to_string(),
                "无法从现有静态数据判断用户是否知情或主动启用。".to_string(),
            ],
            recommendation: "先核实文件来源与用户意图；不要仅凭临时路径直接删除启动项或文件。"
                .to_string(),
        },
        PathState::Present => RiskAssessment {
            scope: RiskScope::StartupMetadata,
            level: RiskLevel::NoClearIssue,
            confidence: Confidence::Low,
            rule_ids: vec!["STARTUP-000".to_string()],
            summary: "在启动项目标元数据的检查范围内未发现明确异常。".to_string(),
            evidence: vec![evidence(
                "STARTUP-TARGET-PRESENT",
                format!("目标路径“{target_path}”当前存在。"),
            )],
            blind_spots: vec![
                "未核验启动审批状态、数字签名、父进程、脚本内容和运行行为。".to_string(),
                "启动项存在并不自动代表它会运行，也不自动代表恶意。".to_string(),
            ],
            recommendation: "结合发布者、签名和用户意图核实；本阶段不修改或禁用启动项。"
                .to_string(),
        },
    }
}

pub fn assess_affinity(facts: AffinityFacts) -> RiskAssessment {
    let mask_evidence = match (facts.process_mask, facts.system_mask) {
        (Some(process), Some(system)) => format!(
            "进程允许的处理器掩码为 {}，当前处理器组的系统掩码为 {}。",
            mask_to_hex(process),
            mask_to_hex(system)
        ),
        (Some(process), None) => format!("进程允许的处理器掩码为 {}.", mask_to_hex(process)),
        _ => "未获得完整的处理器掩码。".to_string(),
    };

    match facts.restricted {
        Some(true) => RiskAssessment {
            scope: RiskScope::AffinityConfiguration,
            level: RiskLevel::ConfigurationAnomaly,
            confidence: Confidence::Low,
            rule_ids: vec!["AFF-001".to_string()],
            summary:
                "检测到受限亲和性配置：进程可用的逻辑处理器范围小于当前处理器组；原因无法从现有接口确认。"
                    .to_string(),
            evidence: vec![
                evidence("AFF-RESTRICTED", mask_evidence),
                evidence(
                    "AFF-NOT-PROOF",
                    "处理器亲和性限制不等同于恶意行为。高 CPU 占用也不能证明亲和性被修改。",
                ),
            ],
            blind_spots: vec![
                "Windows 公开接口不提供 SetProcessAffinityMask 调用者与修改时间的可靠审计链。"
                    .to_string(),
                "无法区分软件自身配置、用户设置、兼容性策略或其他管理行为。".to_string(),
                "没有独立的行为、网络或内存证据。".to_string(),
            ],
            recommendation: "先确认软件配置和用户意图，再决定是否处理；本阶段只读，不修改亲和性。"
                .to_string(),
        },
        Some(false) => RiskAssessment {
            scope: RiskScope::AffinityConfiguration,
            level: RiskLevel::NoClearIssue,
            confidence: Confidence::Low,
            rule_ids: vec!["AFF-000".to_string()],
            summary: "亲和性范围内未发现异常；该结论只涉及当前处理器组亲和性，不代表进程整体安全。"
                .to_string(),
            evidence: vec![evidence("AFF-EQUAL", mask_evidence)],
            blind_spots: vec![
                "亲和性范围一致不代表进程行为安全；未检查行为、网络或内存证据。".to_string(),
                "跨处理器组和超过 64 个逻辑处理器的细节可能未被当前接口完整覆盖。".to_string(),
            ],
            recommendation: "将亲和性视为环境事实，而不是安全结论；继续核查实际行为。".to_string(),
        },
        None => RiskAssessment {
            scope: RiskScope::AffinityConfiguration,
            level: RiskLevel::InsufficientEvidence,
            confidence: Confidence::Unknown,
            rule_ids: vec!["AFF-002".to_string()],
            summary: "亲和性证据不足：无法读取处理器亲和性，当前不能判断范围是否受到限制。"
                .to_string(),
            evidence: vec![evidence("AFF-UNREADABLE", mask_evidence)],
            blind_spots: vec![
                "权限不足、进程退出或受保护进程都可能阻止读取。".to_string(),
                "无法判断设置来源，也不能据此推断恶意。".to_string(),
            ],
            recommendation: "保持未知状态；在用户明确授权后再考虑更深入的只读验证。".to_string(),
        },
    }
}

pub fn assess_process_overall() -> RiskAssessment {
    RiskAssessment {
        scope: RiskScope::ProcessOverall,
        level: RiskLevel::InsufficientEvidence,
        confidence: Confidence::Unknown,
        rule_ids: vec!["PROC-001".to_string()],
        summary: "没有独立行为证据，不能形成整体进程安全结论；当前整体判断为证据不足。".to_string(),
        evidence: vec![evidence(
            "PROC-NO-BEHAVIOR-EVIDENCE",
            "处理器亲和性、磁盘签名和短时 CPU 占用是环境/文件属性，不足以证明进程整体安全或恶意。",
        )],
        blind_spots: vec![
            "未检查完整命令行、模块加载、网络连接、父子进程行为链或内存映像。".to_string(),
            "磁盘文件签名不能证明当前运行内存未被修改。".to_string(),
        ],
        recommendation: "将本次结果限定在亲和性配置范围内；等待独立行为证据后再形成整体结论。"
            .to_string(),
    }
}

fn path_state_label(state: PathState) -> &'static str {
    match state {
        PathState::Present => "存在",
        PathState::Missing => "明确不存在",
        PathState::AccessDenied => "访问被拒绝",
        PathState::Unknown => "未知",
    }
}
