use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use sentinel_sensor::delivery::DEFAULT_DELIVERY_CAPACITY;
use sentinel_sensor::etw::{SensorConfig, run};
use sentinel_sensor::pipeline::DEFAULT_QUEUE_CAPACITY;

const EXIT_CONFIG: i32 = 6;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    if arguments.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_usage();
        return;
    }
    if arguments.iter().any(|arg| arg == "--version") {
        println!("SentinelLiteSensor {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let cli = match parse_arguments(&arguments) {
        Ok(cli) => cli,
        Err(error) => {
            eprintln!("参数错误：{error}");
            print_usage();
            std::process::exit(EXIT_CONFIG);
        }
    };

    let config = SensorConfig::new(cli.session_name, cli.duration)
        .with_service(cli.serve)
        .with_queue_capacity(cli.queue_capacity)
        .with_delivery_capacity(cli.delivery_capacity);
    if let Err(error) = config.validate() {
        eprintln!("参数错误：{error}");
        std::process::exit(EXIT_CONFIG);
    }

    if config.serve {
        eprintln!(
            "[sensor] starting service session={} ipc_pipe={}",
            config.session_name,
            sentinel_ipc::DEFAULT_PIPE_NAME
        );
    } else {
        eprintln!(
            "[sensor] starting session={} duration={}s",
            config.session_name,
            config.duration.as_secs()
        );
    }
    let report = run(config);
    eprintln!(
        "[sensor] state={:?} exit={} received={} processed={} dropped={} lost={:?} stopped={} residual={}",
        report.health.state,
        report.exit_code,
        report.health.events_received,
        report.health.events_processed,
        report.health.events_dropped,
        report.health.events_lost,
        report.session_stopped,
        report.residual_session
    );

    let json = match serde_json::to_string_pretty(&report) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("无法序列化 summary：{error}");
            std::process::exit(EXIT_CONFIG);
        }
    };

    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    if let Err(error) = writeln!(stdout, "{json}") {
        eprintln!("无法写入 stdout：{error}");
        std::process::exit(EXIT_CONFIG);
    }
    drop(stdout);

    if let Some(path) = cli.output {
        let output = format!("{json}\n");
        if let Err(error) = std::fs::write(&path, output) {
            eprintln!("无法写入 {}：{error}", path.display());
            std::process::exit(EXIT_CONFIG);
        }
        eprintln!("[sensor] summary written to {}", path.display());
    }

    std::process::exit(report.exit_code);
}

struct Cli {
    session_name: String,
    duration: Duration,
    output: Option<PathBuf>,
    serve: bool,
    queue_capacity: usize,
    delivery_capacity: usize,
}

fn parse_arguments(arguments: &[String]) -> Result<Cli, String> {
    let mut session_name = format!("SentinelLite-Sensor-{}", std::process::id());
    let mut duration = Duration::from_secs(20);
    let mut output = None;
    let mut serve = false;
    let mut queue_capacity = DEFAULT_QUEUE_CAPACITY;
    let mut delivery_capacity = DEFAULT_DELIVERY_CAPACITY;
    let mut index = 0;

    while index < arguments.len() {
        match arguments[index].as_str() {
            "--session-name" => {
                index += 1;
                let value = arguments
                    .get(index)
                    .ok_or_else(|| "--session-name 需要一个值".to_owned())?;
                session_name.clone_from(value);
            }
            "--duration" => {
                index += 1;
                let value = arguments
                    .get(index)
                    .ok_or_else(|| "--duration 需要一个值".to_owned())?;
                let seconds: u64 = value
                    .parse()
                    .map_err(|_| format!("--duration 必须是整数秒：{value}"))?;
                duration = Duration::from_secs(seconds);
            }
            "--output" => {
                index += 1;
                let value = arguments
                    .get(index)
                    .ok_or_else(|| "--output 需要一个路径".to_owned())?;
                if value.is_empty() {
                    return Err("--output 路径不能为空".to_owned());
                }
                output = Some(PathBuf::from(value));
            }
            "--serve" => {
                serve = true;
            }
            "--queue-capacity" => {
                index += 1;
                let value = arguments
                    .get(index)
                    .ok_or_else(|| "--queue-capacity 需要一个整数".to_owned())?;
                queue_capacity = value
                    .parse()
                    .map_err(|_| format!("--queue-capacity 必须是整数：{value}"))?;
            }
            "--event-buffer" => {
                index += 1;
                let value = arguments
                    .get(index)
                    .ok_or_else(|| "--event-buffer 需要一个整数".to_owned())?;
                delivery_capacity = value
                    .parse()
                    .map_err(|_| format!("--event-buffer 必须是整数：{value}"))?;
            }
            other => {
                return Err(format!("未知参数：{other}"));
            }
        }
        index += 1;
    }

    Ok(Cli {
        session_name,
        duration,
        output,
        serve,
        queue_capacity,
        delivery_capacity,
    })
}

fn print_usage() {
    eprintln!(
        "\
SentinelLiteSensor {version}

用法：
  SentinelLiteSensor [--session-name NAME] [--duration SECONDS] [--output PATH]
                     [--queue-capacity EVENTS] [--event-buffer RECORDS] [--serve]

选项：
  --session-name NAME  ETW session 名（ASCII 字母、数字、- _ .，最多 128 字节）
  --duration SECONDS   定时模式运行秒数（1..=3600，默认 20；--serve 时忽略）
  --output PATH        额外把 JSON summary 写入文件
  --queue-capacity N   标准化事件队列容量（64..=65536，默认 4096）
  --event-buffer N     IPC 事件投递窗口容量（64..=65536，默认 4096）
  --serve              服务模式：保持运行，通过只读 IPC 状态管道供主程序查询，Ctrl-C 停止
  -h, --help           显示帮助
  --version            显示版本

退出码：
  0  正常完成
  2  权限不足（fail closed，不创建/不残留 session）
  3  降级：无事件、丢事件、解码失败、配对完整性异常或丢失统计未知
  4  ETW session 启动、provider 启用、OpenTrace 或 ProcessTrace 失败
  5  停止失败或存在残余 session
  6  参数错误或输出写入失败

正常运行时 stdout 输出 JSON summary（--help/--version 除外）；运行日志写入 stderr。",
        version = env!("CARGO_PKG_VERSION")
    );
}
