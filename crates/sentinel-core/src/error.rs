use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("进程 ID 无效")]
    InvalidPid,
    #[error("进程身份令牌无效")]
    InvalidProcessIdentity,
    #[error("进程 {pid} 的身份已变化，可能发生了 PID 复用或进程退出")]
    ProcessIdentityChanged { pid: u32 },
    #[error("无法检查进程 {pid}：{reason}")]
    ProcessUnavailable { pid: u32, reason: String },
    #[error("无法创建进程快照：{0}")]
    ProcessSnapshot(String),
}
