use crate::model::{
    DeepDefenseState, DeepDefenseStatus, ProtectionOverview, ProtectionSensor, ProtectionState,
    ReadOnlyCheck, SensorState,
};
use crate::util::now_rfc3339;

const MAXIMUM_MONITORING_LIMITATION: &str = "尚未实现持续事件采集，不能产生实时告警或拦截。";

pub fn protection_overview() -> ProtectionOverview {
    ProtectionOverview {
        generated_at: now_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        overall_state: ProtectionState::Unknown,
        headline: "无法确认整体保护状态".to_string(),
        explanation: "进程传感器只覆盖进程 start/stop；原生防护状态是用户主动触发的只读快照；没有告警队列、自动处置或完整覆盖，不能视为持续保护。"
            .to_string(),
        sensors: vec![
            ProtectionSensor {
                id: "realtime-process".to_string(),
                name: "实时进程监控".to_string(),
                state: SensorState::NotImplemented,
                description: "记录新进程、父子关系与异常执行链。".to_string(),
                coverage: Vec::new(),
                limitations: vec![
                    "尚未接入 ETW 进程事件。".to_string(),
                    MAXIMUM_MONITORING_LIMITATION.to_string(),
                ],
            },
            ProtectionSensor {
                id: "filesystem".to_string(),
                name: "文件变化监控".to_string(),
                state: SensorState::NotImplemented,
                description: "记录文件创建、修改、批量重命名和删除。".to_string(),
                coverage: Vec::new(),
                limitations: vec![
                    "尚未接入文件系统事件与目录监控。".to_string(),
                    MAXIMUM_MONITORING_LIMITATION.to_string(),
                ],
            },
            ProtectionSensor {
                id: "persistence".to_string(),
                name: "持久化变化监控".to_string(),
                state: SensorState::NotImplemented,
                description: "观察启动项、计划任务、服务与注册表持久化变化。".to_string(),
                coverage: Vec::new(),
                limitations: vec![
                    "当前只有用户主动触发的部分启动项只读读取。".to_string(),
                    MAXIMUM_MONITORING_LIMITATION.to_string(),
                ],
            },
            ProtectionSensor {
                id: "network".to_string(),
                name: "网络与 DNS 关联".to_string(),
                state: SensorState::NotImplemented,
                description: "关联可疑进程与网络、DNS 活动。".to_string(),
                coverage: Vec::new(),
                limitations: vec![
                    "尚未接入网络事件采集或连接表关联。".to_string(),
                    MAXIMUM_MONITORING_LIMITATION.to_string(),
                ],
            },
            ProtectionSensor {
                id: "native-protection".to_string(),
                name: "原生防护状态".to_string(),
                state: SensorState::NotChecked,
                description: "按需只读读取 Windows Defender、防火墙的开关和签名状态。".to_string(),
                coverage: Vec::new(),
                limitations: vec![
                    "状态由用户主动发起的只读查询提供；未查询、查询失败或字段缺失时保持未知。"
                        .to_string(),
                    "SentinelLite 不关闭、不替代原生安全产品。".to_string(),
                ],
            },
        ],
        read_only_checks: vec![
            ReadOnlyCheck {
                id: "softwareInventory".to_string(),
                name: "软件与组件盘点".to_string(),
                description: "读取 32/64 位注册表视图中的传统卸载信息，不执行卸载。".to_string(),
            },
            ReadOnlyCheck {
                id: "startupItems".to_string(),
                name: "启动项检查".to_string(),
                description: "读取 Run、RunOnce 与启动文件夹，不修改、不禁用、不删除启动项。"
                    .to_string(),
            },
            ReadOnlyCheck {
                id: "affinityInspection".to_string(),
                name: "进程与 CPU 亲和性".to_string(),
                description: "只读观察进程、处理器范围、磁盘文件签名和短时 CPU 时间。".to_string(),
            },
        ],
        deep_defense: DeepDefenseStatus {
            state: DeepDefenseState::NotImplemented,
            explanation:
                "深度防御模式尚未实现。不会显示虚假的启用状态，也不会伪造资源开销或拦截数据。"
                    .to_string(),
        },
        boundaries: vec![
            "实时监控仍不完整：进程 start/stop 已接入，文件/网络/DLL/服务未实现；没有拦截或自动处置。"
                .to_string(),
            "不提供自动卸载、文件删除、进程终止或注册表清洗。".to_string(),
            "无法读取的数据会显示为未知或证据不足，不会被当作安全。".to_string(),
            "与 Windows Defender 等原生安全产品协作，不在第一版取代成熟杀毒引擎。".to_string(),
        ],
    }
}
