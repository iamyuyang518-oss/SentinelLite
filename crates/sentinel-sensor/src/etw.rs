//! Real-time ETW session lifecycle for process start/stop telemetry.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use sentinel_ipc::event_server::{EventProvider, EventServer, EventServerConfig};
use sentinel_ipc::protocol::{
    PermissionState as IpcPermissionState, SensorState as IpcSensorState, SensorStatus,
};
use sentinel_ipc::server::{Server, ServerConfig, ServerError, StatusProvider};
use serde::Serialize;
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_PRIVILEGE_NOT_HELD, ERROR_WMI_INSTANCE_NOT_FOUND, GetLastError,
};
use windows_sys::Win32::System::Console::{
    CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, CTRL_SHUTDOWN_EVENT, SetConsoleCtrlHandler,
};
use windows_sys::Win32::System::Diagnostics::Etw::{
    CONTROLTRACE_HANDLE, CloseTrace, ControlTraceW, EVENT_CONTROL_CODE_ENABLE_PROVIDER,
    EVENT_RECORD, EVENT_TRACE_CONTROL_QUERY, EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_LOGFILEW,
    EVENT_TRACE_LOGFILEW_0, EVENT_TRACE_LOGFILEW_1, EVENT_TRACE_PROPERTIES,
    EVENT_TRACE_REAL_TIME_MODE, EnableTraceEx2, OpenTraceW, PROCESS_TRACE_MODE_EVENT_RECORD,
    PROCESS_TRACE_MODE_REAL_TIME, PROCESSTRACE_HANDLE, ProcessTrace, StartTraceW,
    TRACE_LEVEL_INFORMATION, WNODE_FLAG_TRACED_GUID,
};
use windows_sys::core::GUID;

use crate::decode::{DecodeError, decode_process_event};
use crate::delivery::{
    BrokerSink, DEFAULT_DELIVERY_CAPACITY, DeliveryConfig, DeliverySnapshot, EventBroker,
    MAX_DELIVERY_CAPACITY, MIN_DELIVERY_CAPACITY, SESSION_END_DELIVERY_GRACE,
    SESSION_END_DRAIN_LINGER,
};
use crate::event::SessionEndReason;
use crate::health::{HealthSnapshot, PermissionState, SensorHealth, SensorState};
use crate::pair::{PairingSnapshot, PairingState};
use crate::pipeline::{
    ConsumerState, DEFAULT_QUEUE_CAPACITY, EventPipeline, MAX_QUEUE_CAPACITY, MIN_QUEUE_CAPACITY,
    PipelineConfig, PipelineHealth, PipelineSnapshot,
};

const KERNEL_PROCESS_GUID: GUID = GUID::from_u128(0x22fb2cd6_0e7b_422b_a0c7_2fad1fd0e716);
const KERNEL_PROCESS_GUID_DATA4: [u8; 8] = [0xa0, 0xc7, 0x2f, 0xad, 0x1f, 0xd0, 0xe7, 0x16];
const PROCESS_KEYWORD: u64 = 0x10;
const MAX_SESSION_NAME_BYTES: usize = 128;
const MAX_DURATION_SECONDS: u64 = 3600;
const RESIDUAL_QUERY_ATTEMPTS: usize = 10;
const RESIDUAL_QUERY_DELAY: Duration = Duration::from_millis(50);
const SESSION_STOPPED_ABSENT: u32 = ERROR_WMI_INSTANCE_NOT_FOUND;
const PROCESS_TRACE_GRACE: Duration = Duration::from_secs(10);
const PROCESS_TRACE_RETRY_GRACE: Duration = Duration::from_secs(2);
const SERVICE_WAIT_SLICE: Duration = Duration::from_secs(10);
const WINDOW_COMPLETION_TOLERANCE: Duration = Duration::from_millis(100);

static RUNTIME: OnceLock<Arc<Runtime>> = OnceLock::new();
static ACTIVE_SESSION: OnceLock<Arc<SessionControl>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct SensorConfig {
    pub session_name: String,
    pub duration: Duration,
    pub serve: bool,
    pub queue_capacity: usize,
    pub delivery_capacity: usize,
}

impl SensorConfig {
    pub fn new(session_name: impl Into<String>, duration: Duration) -> Self {
        Self {
            session_name: session_name.into(),
            duration,
            serve: false,
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
            delivery_capacity: DEFAULT_DELIVERY_CAPACITY,
        }
    }

    pub fn with_service(mut self, serve: bool) -> Self {
        self.serve = serve;
        self
    }

    pub fn with_queue_capacity(mut self, queue_capacity: usize) -> Self {
        self.queue_capacity = queue_capacity;
        self
    }

    pub fn with_delivery_capacity(mut self, delivery_capacity: usize) -> Self {
        self.delivery_capacity = delivery_capacity;
        self
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.session_name.is_empty() {
            return Err("session name must not be empty".to_owned());
        }
        if self.session_name.len() > MAX_SESSION_NAME_BYTES {
            return Err(format!(
                "session name must be at most {MAX_SESSION_NAME_BYTES} bytes"
            ));
        }
        if !self.session_name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        }) {
            return Err(
                "session name may contain only ASCII letters, digits, '-', '_' and '.'".to_owned(),
            );
        }
        if self.duration.is_zero() {
            return Err("duration must be greater than zero".to_owned());
        }
        if self.duration > Duration::from_secs(MAX_DURATION_SECONDS) {
            return Err(format!(
                "duration must be at most {MAX_DURATION_SECONDS} seconds"
            ));
        }
        PipelineConfig::new(self.queue_capacity).validate()?;
        DeliveryConfig::new(self.delivery_capacity).validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    pub sensor: &'static str,
    pub version: &'static str,
    pub mode: &'static str,
    pub etl_output: bool,
    pub ipc_requested: bool,
    pub ipc_started: bool,
    pub ipc_pipe: Option<&'static str>,
    pub ipc_error: Option<String>,
    pub event_ipc_requested: bool,
    pub event_ipc_started: bool,
    pub event_ipc_pipe: Option<&'static str>,
    pub event_ipc_error: Option<String>,
    pub delivery: DeliverySnapshot,
    pub session_name: String,
    pub session_created: bool,
    pub session_stopped: bool,
    pub trace_opened: bool,
    pub trace_closed: bool,
    pub interrupted: bool,
    pub elapsed_ms: u64,
    pub expected_ms: u64,
    pub window_complete: bool,
    pub residual_session: bool,
    pub session_stop_error: Option<String>,
    pub session_close_error: Option<String>,
    pub process_trace_error: Option<String>,
    pub coverage: Vec<&'static str>,
    pub protection_claim: &'static str,
    #[serde(flatten)]
    pub health: HealthSnapshot,
    pub pairing: PairingSnapshot,
    pub session_id: String,
    pub pipeline: PipelineSnapshot,
    pub error: Option<String>,
    pub exit_code: i32,
}

pub fn run(config: SensorConfig) -> RunReport {
    let runtime = Arc::new(Runtime::with_capacities(
        config.queue_capacity,
        config.delivery_capacity,
    ));
    let installed = RUNTIME.set(Arc::clone(&runtime)).is_ok();
    let mut context = RunContext::new(config, runtime);

    if installed {
        match context.execute() {
            Ok(()) => {}
            Err(failure) => context.failure = Some(failure),
        }
    } else {
        let message = "sensor runtime is already initialized in this process".to_owned();
        context.runtime.health.mark_failed(&message);
        context.failure = Some(Failure::config(message));
    }

    context.cleanup();
    context.finish()
}

struct Runtime {
    health: Arc<SensorHealth>,
    pairing: Arc<Mutex<PairingState>>,
    delivery: Arc<EventBroker>,
    pipeline: EventPipeline,
}

impl Runtime {
    fn with_capacities(queue_capacity: usize, delivery_capacity: usize) -> Self {
        // Validate before this call rejects out-of-range values; clamp here so
        // a bad CLI value cannot trigger a huge allocation first.
        let queue_capacity = queue_capacity.clamp(MIN_QUEUE_CAPACITY, MAX_QUEUE_CAPACITY);
        let delivery_capacity =
            delivery_capacity.clamp(MIN_DELIVERY_CAPACITY, MAX_DELIVERY_CAPACITY);
        let health = Arc::new(SensorHealth::new());
        let pairing = Arc::new(Mutex::new(PairingState::new()));
        let session_id = sentinel_events::generate_session_id();
        let delivery = EventBroker::new(session_id.clone(), delivery_capacity);
        let pipeline = EventPipeline::start_with_sink_and_session(
            PipelineConfig::new(queue_capacity),
            Arc::clone(&health),
            Arc::clone(&pairing),
            session_id,
            Box::new(BrokerSink::new(Arc::clone(&delivery))),
        );
        Self {
            health,
            pairing,
            delivery,
            pipeline,
        }
    }

    #[cfg(test)]
    fn new() -> Self {
        Self::with_capacities(DEFAULT_QUEUE_CAPACITY, DEFAULT_DELIVERY_CAPACITY)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveStats {
    pub(crate) events_lost: Option<u64>,
    pub(crate) realtime_buffers_lost: Option<u64>,
}

fn live_health_ok(
    health: &HealthSnapshot,
    pairing: &PairingSnapshot,
    stats: Option<LiveStats>,
    pipeline: &PipelineHealth,
) -> bool {
    health.state == SensorState::Active
        && health.events_received > 0
        && health.events_processed > 0
        && health.events_dropped == 0
        && health.decode_errors == 0
        && health.partial_events == 0
        && stats.is_some_and(|stats| {
            stats.events_lost == Some(0) && stats.realtime_buffers_lost == Some(0)
        })
        && pairing.coverage_issue().is_none()
        && !pairing.has_integrity_issue()
        && pipeline.consumer_state == ConsumerState::Running
        && pipeline.consumer_error.is_none()
        && pipeline.dropped_total == 0
        && pipeline.sink_errors == 0
        && !pipeline.shutdown_timed_out
}

fn map_sensor_state(state: SensorState) -> IpcSensorState {
    match state {
        SensorState::NotStarted => IpcSensorState::NotStarted,
        SensorState::Starting => IpcSensorState::Starting,
        SensorState::Active => IpcSensorState::Active,
        SensorState::Degraded => IpcSensorState::Degraded,
        SensorState::Failed => IpcSensorState::Failed,
        SensorState::Stopped => IpcSensorState::Stopped,
        SensorState::Unavailable => IpcSensorState::Unavailable,
    }
}

fn map_permission_state(state: PermissionState) -> IpcPermissionState {
    match state {
        PermissionState::Unknown => IpcPermissionState::Unknown,
        PermissionState::Granted => IpcPermissionState::Granted,
        PermissionState::Limited => IpcPermissionState::Limited,
        PermissionState::Denied => IpcPermissionState::Denied,
    }
}

struct LiveStatusProvider {
    runtime: Arc<Runtime>,
    session: Arc<SessionControl>,
    started: Instant,
}

impl StatusProvider for LiveStatusProvider {
    fn status(&self) -> Result<SensorStatus, String> {
        let health = self.runtime.health.snapshot();
        let pairing = self
            .runtime
            .pairing
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .snapshot();
        let stats = self.session.live_stats();
        let pipeline_health = self.runtime.pipeline.health_snapshot();
        let health_ok = live_health_ok(&health, &pairing, stats, &pipeline_health);
        let last_error = health
            .last_error
            .clone()
            .or_else(|| self.runtime.pipeline.last_error())
            .or_else(|| {
                if stats.is_none() {
                    Some("ETW live loss count unavailable".to_owned())
                } else if health.state == SensorState::Active && !health_ok {
                    Some("sensor health not proven".to_owned())
                } else {
                    None
                }
            });
        Ok(SensorStatus {
            sensor_state: map_sensor_state(health.state),
            permission_state: map_permission_state(health.permission_state),
            events_received: health.events_received,
            events_processed: health.events_processed,
            events_dropped: health.events_dropped,
            events_lost: stats.and_then(|stats| stats.events_lost),
            realtime_buffers_lost: stats.and_then(|stats| stats.realtime_buffers_lost),
            decode_errors: health.decode_errors,
            last_event_at: health.last_event_at,
            uptime_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
            health_ok,
            last_error,
        })
    }
}

unsafe extern "system" fn event_record_callback(record: *mut EVENT_RECORD) {
    let Some(runtime) = RUNTIME.get() else {
        return;
    };
    runtime.health.record_received();

    if record.is_null() {
        runtime
            .pipeline
            .push_invalid(0, 0, 0, "null ETW event record", None);
        return;
    }
    // SAFETY: the callback is only invoked with a valid EVENT_RECORD for the
    // duration of the call.
    let record = unsafe { &*record };

    if !is_kernel_process(&record.EventHeader.ProviderId) {
        runtime.pipeline.record_filtered_ignored();
        return;
    }

    let event_id = record.EventHeader.EventDescriptor.Id;
    let version = record.EventHeader.EventDescriptor.Version;
    let timestamp =
        (record.EventHeader.TimeStamp > 0).then_some(record.EventHeader.TimeStamp as u64);
    let payload = if record.UserData.is_null() || record.UserDataLength == 0 {
        &[][..]
    } else {
        // SAFETY: UserData points to UserDataLength bytes for the duration of
        // the callback.
        unsafe {
            std::slice::from_raw_parts(record.UserData.cast::<u8>(), record.UserDataLength as usize)
        }
    };

    match decode_process_event(event_id, version, payload) {
        Ok(event) => runtime.pipeline.push_process(event, timestamp),
        Err(DecodeError::UnsupportedEvent { .. }) => {
            // Event IDs outside this slice are intentionally ignored; they are
            // not a decode failure of a process start/stop event.
            runtime.pipeline.record_filtered_ignored();
        }
        Err(error) => runtime.pipeline.push_invalid(
            event_id,
            version,
            payload.len(),
            &error.to_string(),
            timestamp,
        ),
    }
}

fn is_kernel_process(guid: &GUID) -> bool {
    guid.data1 == 0x22fb_2cd6
        && guid.data2 == 0x0e7b
        && guid.data3 == 0x422b
        && guid.data4 == KERNEL_PROCESS_GUID_DATA4
}

#[derive(Debug, Clone, Default)]
struct StopProbe {
    stop_rc: Option<u32>,
    events_lost: Option<u64>,
    realtime_buffers_lost: Option<u64>,
    stopped: bool,
}

struct SessionControl {
    handle: CONTROLTRACE_HANDLE,
    name: Vec<u16>,
    stop_state: Mutex<Option<StopProbe>>,
    interrupted: AtomicBool,
}

impl SessionControl {
    fn start(name: &str) -> Result<Self, StartTraceFailure> {
        let mut wide: Vec<u16> = name.encode_utf16().collect();
        wide.push(0);
        let mut properties = PropsBuffer::new(&wide);
        let mut handle = CONTROLTRACE_HANDLE::default();
        // SAFETY: pointers refer to live, correctly sized buffers.
        let rc = unsafe { StartTraceW(&mut handle, wide.as_ptr(), properties.as_mut_ptr()) };
        if rc != 0 {
            return Err(StartTraceFailure { code: rc });
        }
        Ok(Self {
            handle,
            name: wide,
            stop_state: Mutex::new(None),
            interrupted: AtomicBool::new(false),
        })
    }

    #[cfg(test)]
    fn for_test(name: &str) -> Self {
        let mut wide: Vec<u16> = name.encode_utf16().collect();
        wide.push(0);
        Self {
            handle: CONTROLTRACE_HANDLE::default(),
            name: wide,
            stop_state: Mutex::new(None),
            interrupted: AtomicBool::new(false),
        }
    }

    fn enable_provider(&self) -> u32 {
        // SAFETY: the session handle is owned by this process and the provider
        // GUID is a valid static value.
        unsafe {
            EnableTraceEx2(
                self.handle,
                &KERNEL_PROCESS_GUID,
                EVENT_CONTROL_CODE_ENABLE_PROVIDER,
                TRACE_LEVEL_INFORMATION as u8,
                PROCESS_KEYWORD,
                0,
                0,
                std::ptr::null(),
            )
        }
    }

    fn open_trace(&self, logger_name: &mut [u16]) -> Result<PROCESSTRACE_HANDLE, u32> {
        let mut logfile = EVENT_TRACE_LOGFILEW {
            LoggerName: logger_name.as_mut_ptr(),
            Anonymous1: EVENT_TRACE_LOGFILEW_0 {
                ProcessTraceMode: PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD,
            },
            Anonymous2: EVENT_TRACE_LOGFILEW_1 {
                EventRecordCallback: Some(event_record_callback),
            },
            Context: std::ptr::null_mut(),
            ..Default::default()
        };
        // SAFETY: logfile and the logger name buffer are live for this call.
        let handle = unsafe { OpenTraceW(&mut logfile) };
        if handle.Value == u64::MAX {
            // SAFETY: reads the thread-local last error set by OpenTraceW.
            Err(unsafe { GetLastError() })
        } else {
            Ok(handle)
        }
    }

    fn stop(&self) -> StopProbe {
        let mut guard = self
            .stop_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(probe) = guard.as_ref() {
            if probe.stopped {
                return probe.clone();
            }
        }
        // Retry a previously failed stop instead of returning a cached failure.
        let probe = self.query_and_stop();
        *guard = Some(probe.clone());
        probe
    }

    fn interrupt(&self) {
        self.interrupted.store(true, Ordering::SeqCst);
        let _ = self.stop();
    }

    fn was_interrupted(&self) -> bool {
        self.interrupted.load(Ordering::SeqCst)
    }

    fn query_and_stop(&self) -> StopProbe {
        let mut query_properties = PropsBuffer::new(&self.name);
        // SAFETY: the session is owned by this process and the properties
        // buffer is valid.
        let query_rc = unsafe {
            ControlTraceW(
                self.handle,
                self.name.as_ptr(),
                query_properties.as_mut_ptr(),
                EVENT_TRACE_CONTROL_QUERY,
            )
        };
        let query_stats = (query_rc == 0).then(|| query_properties.values());

        let mut stop_properties = PropsBuffer::new(&self.name);
        // SAFETY: same live session handle and a valid properties buffer.
        let stop_rc = unsafe {
            ControlTraceW(
                self.handle,
                self.name.as_ptr(),
                stop_properties.as_mut_ptr(),
                EVENT_TRACE_CONTROL_STOP,
            )
        };
        let stop_stats = (stop_rc == 0).then(|| stop_properties.values());

        let already_gone = query_rc == SESSION_STOPPED_ABSENT || stop_rc == SESSION_STOPPED_ABSENT;

        StopProbe {
            stop_rc: Some(stop_rc),
            events_lost: merge_max(
                query_stats.map(|stats| u64::from(stats.events_lost)),
                stop_stats.map(|stats| u64::from(stats.events_lost)),
            ),
            realtime_buffers_lost: merge_max(
                query_stats.map(|stats| u64::from(stats.realtime_buffers_lost)),
                stop_stats.map(|stats| u64::from(stats.realtime_buffers_lost)),
            ),
            stopped: stop_rc == 0 || already_gone,
        }
    }

    fn live_stats(&self) -> Option<LiveStats> {
        let mut properties = PropsBuffer::new(&self.name);
        // SAFETY: the session handle is owned by this process and the
        // properties buffer is live for the call. QUERY does not stop it.
        let rc = unsafe {
            ControlTraceW(
                self.handle,
                self.name.as_ptr(),
                properties.as_mut_ptr(),
                EVENT_TRACE_CONTROL_QUERY,
            )
        };
        (rc == 0).then(|| {
            let stats = properties.values();
            LiveStats {
                events_lost: Some(u64::from(stats.events_lost)),
                realtime_buffers_lost: Some(u64::from(stats.realtime_buffers_lost)),
            }
        })
    }

    fn query_exists(&self) -> Result<bool, u32> {
        let mut properties = PropsBuffer::new(&self.name);
        // SAFETY: query by instance name, so a zero handle is valid.
        let rc = unsafe {
            ControlTraceW(
                CONTROLTRACE_HANDLE::default(),
                self.name.as_ptr(),
                properties.as_mut_ptr(),
                EVENT_TRACE_CONTROL_QUERY,
            )
        };
        if rc == 0 {
            Ok(true)
        } else if rc == SESSION_STOPPED_ABSENT {
            Ok(false)
        } else {
            Err(rc)
        }
    }
}

unsafe extern "system" fn console_ctrl_handler(ctrl_type: u32) -> i32 {
    match ctrl_type {
        CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT | CTRL_SHUTDOWN_EVENT => {
            if let Some(session) = ACTIVE_SESSION.get() {
                session.interrupt();
            }
            // Return TRUE so Ctrl-C does not terminate the process before the
            // normal cleanup/report path runs. Close/shutdown may still be
            // terminated by Windows after the handler returns.
            1
        }
        _ => 0,
    }
}

fn install_console_handler(session: &Arc<SessionControl>) -> Result<(), String> {
    // SAFETY: the handler is a valid static function pointer; adding it does
    // not transfer ownership.
    let installed = unsafe { SetConsoleCtrlHandler(Some(console_ctrl_handler), 1) };
    if installed == 0 {
        // SAFETY: reads the thread-local last error set by the failed call.
        let code = unsafe { GetLastError() };
        return Err(format!(
            "SetConsoleCtrlHandler failed: {code}: {}",
            os_error_text(code)
        ));
    }
    ACTIVE_SESSION
        .set(Arc::clone(session))
        .map_err(|_| "active sensor session is already registered".to_owned())
}

fn is_permission_error(code: u32) -> bool {
    code == ERROR_ACCESS_DENIED || code == ERROR_PRIVILEGE_NOT_HELD
}

struct PropsBuffer {
    words: Vec<u64>,
}

impl PropsBuffer {
    fn new(name: &[u16]) -> Self {
        let base_size = std::mem::size_of::<EVENT_TRACE_PROPERTIES>();
        let total_bytes = base_size + (name.len() + 1) * std::mem::size_of::<u16>();
        let word_count = total_bytes.div_ceil(std::mem::size_of::<u64>());
        let mut words = vec![0u64; word_count];

        // SAFETY: the allocation is at least EVENT_TRACE_PROPERTIES bytes and
        // is aligned for u64 (and therefore for the struct).
        unsafe {
            let properties = words.as_mut_ptr().cast::<EVENT_TRACE_PROPERTIES>();
            (*properties).Wnode.BufferSize = (word_count * std::mem::size_of::<u64>()) as u32;
            (*properties).Wnode.Flags = WNODE_FLAG_TRACED_GUID;
            (*properties).Wnode.ClientContext = 1;
            (*properties).LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
            (*properties).FlushTimer = 1;
            (*properties).BufferSize = 64;
            (*properties).MinimumBuffers = 4;
            (*properties).MaximumBuffers = 16;
            (*properties).LoggerNameOffset = base_size as u32;

            let destination = words.as_mut_ptr().cast::<u8>().add(base_size).cast::<u16>();
            std::ptr::copy_nonoverlapping(name.as_ptr(), destination, name.len());
        }

        Self { words }
    }

    fn as_mut_ptr(&mut self) -> *mut EVENT_TRACE_PROPERTIES {
        self.words.as_mut_ptr().cast()
    }

    fn values(&self) -> TraceStats {
        // SAFETY: the buffer was initialized as EVENT_TRACE_PROPERTIES.
        unsafe {
            let properties = self.words.as_ptr().cast::<EVENT_TRACE_PROPERTIES>();
            TraceStats {
                events_lost: (*properties).EventsLost,
                realtime_buffers_lost: (*properties).RealTimeBuffersLost,
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TraceStats {
    events_lost: u32,
    realtime_buffers_lost: u32,
}

#[derive(Debug)]
struct StartTraceFailure {
    code: u32,
}

impl StartTraceFailure {
    fn is_permission_denied(&self) -> bool {
        self.code == ERROR_ACCESS_DENIED || self.code == ERROR_PRIVILEGE_NOT_HELD
    }
}

impl fmt::Display for StartTraceFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "StartTraceW failed: {}: {}",
            self.code,
            os_error_text(self.code)
        )
    }
}

#[derive(Debug, Clone)]
struct Failure {
    exit_code: i32,
    message: String,
}

impl Failure {
    fn config(message: impl Into<String>) -> Self {
        Self {
            exit_code: 6,
            message: message.into(),
        }
    }

    fn permission(message: impl Into<String>) -> Self {
        Self {
            exit_code: 2,
            message: message.into(),
        }
    }

    fn etw(message: impl Into<String>) -> Self {
        Self {
            exit_code: 4,
            message: message.into(),
        }
    }

    fn cleanup(message: impl Into<String>) -> Self {
        Self {
            exit_code: 5,
            message: message.into(),
        }
    }
}

struct RunContext {
    config: SensorConfig,
    runtime: Arc<Runtime>,
    session: Option<Arc<SessionControl>>,
    session_created: bool,
    provider_enabled: bool,
    trace_opened: bool,
    trace_closed: bool,
    trace_handle: Option<PROCESSTRACE_HANDLE>,
    trace_worker: Option<JoinHandle<()>>,
    trace_worker_started: bool,
    collection_started: Option<Instant>,
    collection_ended: Option<Instant>,
    stopper: Option<JoinHandle<()>>,
    stop_signal: Option<mpsc::Sender<()>>,
    ipc_server: Option<Server>,
    ipc_started: bool,
    ipc_error: Option<String>,
    event_server: Option<EventServer>,
    event_ipc_started: bool,
    event_ipc_error: Option<String>,
    stop_probe: Option<StopProbe>,
    process_trace_rc: Option<u32>,
    close_trace_rc: Option<u32>,
    residual_session: bool,
    residual_error: Option<String>,
    failure: Option<Failure>,
}

impl RunContext {
    fn new(config: SensorConfig, runtime: Arc<Runtime>) -> Self {
        Self {
            config,
            runtime,
            session: None,
            session_created: false,
            provider_enabled: false,
            trace_opened: false,
            trace_closed: false,
            trace_handle: None,
            trace_worker: None,
            trace_worker_started: false,
            collection_started: None,
            collection_ended: None,
            stopper: None,
            stop_signal: None,
            ipc_server: None,
            ipc_started: false,
            ipc_error: None,
            event_server: None,
            event_ipc_started: false,
            event_ipc_error: None,
            stop_probe: None,
            process_trace_rc: None,
            close_trace_rc: None,
            residual_session: false,
            residual_error: None,
            failure: None,
        }
    }

    fn execute(&mut self) -> Result<(), Failure> {
        self.config.validate().map_err(Failure::config)?;

        if let Some(error) = self.runtime.pipeline.startup_error() {
            let message = error.to_owned();
            self.runtime.health.mark_failed(&message);
            return Err(Failure::etw(message));
        }

        self.runtime.health.set_permission(PermissionState::Unknown);
        self.runtime.health.mark_starting();

        let session = match SessionControl::start(&self.config.session_name) {
            Ok(session) => session,
            Err(error) if error.is_permission_denied() => {
                self.runtime.health.set_permission(PermissionState::Denied);
                self.runtime.health.mark_unavailable(&error.to_string());
                return Err(Failure::permission(error.to_string()));
            }
            Err(error) => {
                self.runtime.health.mark_failed(&error.to_string());
                return Err(Failure::etw(error.to_string()));
            }
        };

        let session = Arc::new(session);
        self.session = Some(Arc::clone(&session));
        self.session_created = true;

        if let Err(message) = install_console_handler(&session) {
            self.runtime.health.mark_failed(&message);
            return Err(Failure::etw(message));
        }

        let enable_rc = session.enable_provider();
        if enable_rc != 0 {
            let message = format!(
                "EnableTraceEx2 failed: {enable_rc}: {}",
                os_error_text(enable_rc)
            );
            if is_permission_error(enable_rc) {
                self.runtime.health.set_permission(PermissionState::Denied);
            }
            self.runtime.health.mark_failed(&message);
            return Err(Failure::etw(message));
        }
        self.provider_enabled = true;

        let mut logger_name = session.name.clone();
        let trace_handle = match session.open_trace(&mut logger_name) {
            Ok(handle) => handle,
            Err(code) => {
                let message = format!("OpenTraceW failed: {code}: {}", os_error_text(code));
                if is_permission_error(code) {
                    self.runtime.health.set_permission(PermissionState::Denied);
                }
                self.runtime.health.mark_failed(&message);
                return Err(Failure::etw(message));
            }
        };
        self.trace_opened = true;
        self.trace_handle = Some(trace_handle);
        self.runtime.health.set_permission(PermissionState::Granted);
        self.runtime.health.mark_active();
        self.collection_started = Some(Instant::now());

        if self.config.serve {
            let provider = Arc::new(LiveStatusProvider {
                runtime: Arc::clone(&self.runtime),
                session: Arc::clone(&session),
                started: Instant::now(),
            });
            self.handle_ipc_start(Server::start(
                ServerConfig::new(sentinel_ipc::DEFAULT_PIPE_NAME),
                provider,
            ))?;
            let event_provider: Arc<dyn EventProvider> = self.runtime.delivery.clone();
            self.handle_event_ipc_start(EventServer::start(
                EventServerConfig::new(sentinel_ipc::DEFAULT_EVENT_PIPE_NAME),
                event_provider,
            ))?;
        }

        let duration = self.config.duration;
        if !self.config.serve {
            let (stop_signal, stop_receiver) = mpsc::channel::<()>();
            self.stop_signal = Some(stop_signal);
            let stopper_session = Arc::clone(&session);
            let stopper = thread::Builder::new()
                .name("sentinel-sensor-stopper".to_owned())
                .spawn(move || {
                    if let Err(RecvTimeoutError::Timeout) = stop_receiver.recv_timeout(duration) {
                        stopper_session.stop();
                    }
                });
            match stopper {
                Ok(handle) => self.stopper = Some(handle),
                Err(error) => {
                    let message = format!("failed to spawn ETW stopper thread: {error}");
                    self.runtime.health.mark_failed(&message);
                    return Err(Failure::etw(message));
                }
            }
        }

        let (trace_sender, trace_receiver) = mpsc::channel::<(u32, u32)>();
        let worker_handle = trace_handle;
        let worker = thread::Builder::new()
            .name("sentinel-sensor-trace".to_owned())
            .spawn(move || {
                // Catch Rust panics after ProcessTrace returns so CloseTrace is
                // still attempted. A panic inside the FFI callback aborts the
                // process before this point.
                let process_rc = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    // SAFETY: worker_handle was returned by OpenTraceW and is
                    // owned by this worker for the lifetime of the call.
                    unsafe { ProcessTrace(&worker_handle, 1, std::ptr::null(), std::ptr::null()) }
                }))
                .unwrap_or(u32::MAX);
                // SAFETY: the trace handle is closed exactly once by this
                // worker, after ProcessTrace returns or unwinds.
                let close_rc = unsafe { CloseTrace(worker_handle) };
                let _ = trace_sender.send((process_rc, close_rc));
            });
        match worker {
            Ok(handle) => {
                self.trace_worker = Some(handle);
                self.trace_worker_started = true;
            }
            Err(error) => {
                let message = format!("failed to spawn ETW trace worker thread: {error}");
                self.runtime.health.mark_failed(&message);
                return Err(Failure::etw(message));
            }
        }

        let wait_timeout = if self.config.serve {
            SERVICE_WAIT_SLICE
        } else {
            duration.saturating_add(PROCESS_TRACE_GRACE)
        };
        loop {
            match trace_receiver.recv_timeout(wait_timeout) {
                Ok((process_rc, close_rc)) => {
                    self.collection_ended = Some(Instant::now());
                    self.process_trace_rc = Some(process_rc);
                    self.close_trace_rc = Some(close_rc);
                    self.trace_closed = true;
                    self.signal_stopper();
                    let probe = session.stop();
                    self.record_stop_probe(probe);
                    self.join_stopper();
                    self.join_trace_worker();

                    if let Some(failure) = trace_result_failure(process_rc) {
                        self.runtime.health.mark_failed(&failure.message);
                        return Err(failure);
                    }
                    if self.config.serve && !session.was_interrupted() {
                        let message = "service ended without an interrupt".to_owned();
                        self.runtime.health.mark_failed(&message);
                        return Err(Failure::cleanup(message));
                    }
                    return Ok(());
                }
                Err(RecvTimeoutError::Timeout) => {
                    if self.config.serve {
                        if !session.was_interrupted() {
                            continue;
                        }
                        self.collection_ended = Some(Instant::now());
                        let probe = session.stop();
                        self.record_stop_probe(probe);
                        self.join_stopper();

                        // A retry may have succeeded even though the first stop
                        // failed. Give the worker a short window to report it.
                        match trace_receiver.recv_timeout(PROCESS_TRACE_RETRY_GRACE) {
                            Ok((process_rc, close_rc)) => {
                                self.process_trace_rc = Some(process_rc);
                                self.close_trace_rc = Some(close_rc);
                                self.trace_closed = true;
                                self.join_trace_worker();
                                if let Some(failure) = trace_result_failure(process_rc) {
                                    self.runtime.health.mark_failed(&failure.message);
                                    return Err(failure);
                                }
                                // The interrupt arrived just after the wait
                                // slice expired; this is still a normal
                                // interrupted-service exit.
                                return Ok(());
                            }
                            Err(RecvTimeoutError::Disconnected) => {
                                self.join_trace_worker();
                                let message =
                                    "ProcessTrace worker exited without reporting a result"
                                        .to_owned();
                                self.runtime.health.mark_failed(&message);
                                return Err(Failure::etw(message));
                            }
                            Err(RecvTimeoutError::Timeout) => {
                                self.join_trace_worker();
                                let message = "ProcessTrace did not return after interrupted \
                                               service stop"
                                    .to_owned();
                                self.runtime.health.mark_failed(&message);
                                return Err(Failure::cleanup(message));
                            }
                        }
                    }

                    self.collection_ended = Some(Instant::now());
                    self.signal_stopper();
                    let probe = session.stop();
                    self.record_stop_probe(probe);
                    self.join_stopper();

                    // A retry may have succeeded even though the first stop failed.
                    // Give the worker a short window to report its close result.
                    if let Ok((process_rc, close_rc)) =
                        trace_receiver.recv_timeout(PROCESS_TRACE_RETRY_GRACE)
                    {
                        self.process_trace_rc = Some(process_rc);
                        self.close_trace_rc = Some(close_rc);
                        self.trace_closed = true;
                    }
                    self.join_trace_worker();

                    let message = format!(
                        "ProcessTrace did not return within {}s after stop",
                        wait_timeout.as_secs()
                    );
                    self.runtime.health.mark_failed(&message);
                    return Err(Failure::cleanup(message));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.collection_ended = Some(Instant::now());
                    self.signal_stopper();
                    let probe = session.stop();
                    self.record_stop_probe(probe);
                    self.join_stopper();
                    self.join_trace_worker();
                    let message =
                        "ProcessTrace worker exited without reporting a result".to_owned();
                    self.runtime.health.mark_failed(&message);
                    return Err(Failure::etw(message));
                }
            }
        }
    }

    fn handle_ipc_start(&mut self, result: Result<Server, ServerError>) -> Result<(), Failure> {
        match result {
            Ok(server) => {
                self.ipc_server = Some(server);
                self.ipc_started = true;
                Ok(())
            }
            Err(error) if error.is_fatal() => {
                let message = format!("IPC privilege state is unrecoverable: {error}");
                self.runtime.health.mark_failed(&message);
                self.ipc_error = Some(message.clone());
                Err(Failure::etw(message))
            }
            Err(error) => {
                let message = format!("IPC status pipe unavailable: {error}");
                self.runtime.health.mark_degraded(&message);
                self.ipc_error = Some(message);
                Ok(())
            }
        }
    }

    fn handle_event_ipc_start(
        &mut self,
        result: Result<EventServer, ServerError>,
    ) -> Result<(), Failure> {
        match result {
            Ok(server) => {
                self.event_server = Some(server);
                self.event_ipc_started = true;
                Ok(())
            }
            Err(error) if error.is_fatal() => {
                let message = format!("IPC privilege state is unrecoverable: {error}");
                self.runtime.health.mark_failed(&message);
                self.event_ipc_error = Some(message.clone());
                Err(Failure::etw(message))
            }
            Err(error) => {
                let message = format!("IPC event pipe unavailable: {error}");
                self.runtime.health.mark_degraded(&message);
                self.event_ipc_error = Some(message);
                Ok(())
            }
        }
    }

    /// Keep a successful stop probe even if an earlier attempt failed. A
    /// later failure must not overwrite a probe that already stopped cleanly.
    fn record_stop_probe(&mut self, probe: StopProbe) {
        let keep_existing = self
            .stop_probe
            .as_ref()
            .is_some_and(|existing| existing.stopped);
        if !keep_existing {
            self.stop_probe = Some(probe);
        }
    }

    /// The ETW stop probe only supplies final loss counters when the session
    /// actually stopped and no residual session remains. A failed stop must
    /// leave both values unknown, not zero.
    fn final_etw_loss_stats(&self) -> (Option<u64>, Option<u64>) {
        if self.residual_session {
            return (None, None);
        }
        let Some(probe) = &self.stop_probe else {
            return (None, None);
        };
        // Require our own successful stop. A session that was already absent
        // (for example stopped by logman) may have query stats, but they are
        // not proven to be the final values from this run.
        if !probe.stopped || probe.stop_rc != Some(0) {
            return (None, None);
        }
        (probe.events_lost, probe.realtime_buffers_lost)
    }

    fn cleanup(&mut self) {
        if self.collection_started.is_some() && self.collection_ended.is_none() {
            self.collection_ended = Some(Instant::now());
        }
        self.stop_and_close();
        if self.session_created {
            self.check_residual_session();
        }
        let (events_lost, realtime_buffers_lost) = self.final_etw_loss_stats();
        self.runtime
            .pipeline
            .set_final_etw_stats(events_lost, realtime_buffers_lost);
        let (reason, error) = self.session_end_info();
        let _ = match error {
            Some(message) => self.runtime.pipeline.close_with_error(message),
            None => self.runtime.pipeline.close(reason),
        };
        self.finish_event_delivery();
        self.apply_health();
    }

    /// Keep the event endpoint alive until a client that has been polling
    /// reads the final `SessionEnd`, bounded by a short grace period. A run
    /// with no event reader exits immediately.
    fn finish_event_delivery(&mut self) {
        let Some(mut server) = self.event_server.take() else {
            return;
        };
        let had_reader = self.runtime.delivery.snapshot().batches_served > 0;
        if had_reader {
            if !self
                .runtime
                .delivery
                .wait_for_session_end_delivery(SESSION_END_DELIVERY_GRACE)
            {
                self.runtime
                    .health
                    .mark_degraded("event delivery did not confirm session end before shutdown");
            }
            // Multiple consumers can share the endpoint (the live UI and the
            // persistence worker). One ack must not let the sensor close the
            // pipe before the other has read the tail.
            thread::sleep(SESSION_END_DRAIN_LINGER);
        }
        if let Err(error) = server.stop() {
            let message = format!("IPC event server stop failed: {error}");
            self.runtime.health.mark_degraded(&message);
            if self.event_ipc_error.is_none() {
                self.event_ipc_error = Some(message);
            }
        }
    }

    fn session_end_info(&self) -> (SessionEndReason, Option<String>) {
        if let Some(failure) = &self.failure {
            return (SessionEndReason::Error, Some(failure.message.clone()));
        }
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.was_interrupted())
        {
            return (SessionEndReason::Interrupted, None);
        }
        if self.config.serve {
            (SessionEndReason::Shutdown, None)
        } else {
            (SessionEndReason::Completed, None)
        }
    }

    fn stop_and_close(&mut self) {
        if let Some(mut server) = self.ipc_server.take() {
            if let Err(error) = server.stop() {
                let message = format!("IPC server stop failed: {error}");
                self.runtime.health.mark_degraded(&message);
                if self.ipc_error.is_none() {
                    self.ipc_error = Some(message);
                }
            }
        }
        self.signal_stopper();
        if let Some(session) = &self.session {
            let probe = session.stop();
            self.record_stop_probe(probe);
        }
        self.join_stopper();

        // The trace worker owns CloseTrace after it has been spawned. Only the
        // main thread closes when the worker could not be started at all.
        if self.trace_opened && !self.trace_closed && !self.trace_worker_started {
            if let Some(handle) = self.trace_handle {
                // SAFETY: no worker owns the handle and it is closed once.
                let rc = unsafe { CloseTrace(handle) };
                self.close_trace_rc = Some(rc);
                self.trace_closed = true;
            }
        }
    }

    fn collection_elapsed(&self) -> Duration {
        match (self.collection_started, self.collection_ended) {
            (Some(start), Some(end)) => end.saturating_duration_since(start),
            _ => Duration::ZERO,
        }
    }

    fn signal_stopper(&mut self) {
        if let Some(signal) = self.stop_signal.take() {
            let _ = signal.send(());
        }
    }

    fn join_stopper(&mut self) {
        if let Some(handle) = self.stopper.take() {
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
    }

    fn join_trace_worker(&mut self) {
        if let Some(handle) = self.trace_worker.take() {
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
    }

    fn check_residual_session(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        let mut residual = true;
        let mut residual_error = None;
        for attempt in 0..RESIDUAL_QUERY_ATTEMPTS {
            match session.query_exists() {
                Ok(false) => {
                    residual = false;
                    break;
                }
                Ok(true) => {
                    if attempt + 1 < RESIDUAL_QUERY_ATTEMPTS {
                        thread::sleep(RESIDUAL_QUERY_DELAY);
                    }
                }
                Err(code) => {
                    residual = true;
                    residual_error = Some(format!(
                        "ETW residual query failed: {code}: {}",
                        os_error_text(code)
                    ));
                    break;
                }
            }
        }
        if residual && residual_error.is_none() {
            residual_error = Some("ETW session still present after stop".to_owned());
        }
        self.residual_session = residual;
        self.residual_error = residual_error;
    }

    fn apply_health(&mut self) {
        let health = &self.runtime.health;
        let pairing = self
            .runtime
            .pairing
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .snapshot();
        if let Some(error) = &self.ipc_error {
            health.mark_degraded(error);
        }
        if let Some(error) = &self.event_ipc_error {
            health.mark_degraded(error);
        }

        let pipeline = self.runtime.pipeline.snapshot();
        if pipeline.consumer_state == ConsumerState::Failed {
            let message = pipeline
                .consumer_error
                .clone()
                .unwrap_or_else(|| "event pipeline consumer failed".to_owned());
            health.mark_failed(&message);
        } else if let Some(error) = &pipeline.consumer_error {
            health.mark_degraded(error);
        }
        if pipeline.dropped_total() > 0 {
            health.mark_degraded(&format!(
                "event pipeline dropped {} events (bounded queue)",
                pipeline.dropped_total()
            ));
        }
        if pipeline.shutdown_timed_out {
            let backlog = pipeline.backlog_at_shutdown_timeout.unwrap_or(0);
            health.mark_degraded(&format!(
                "event pipeline shutdown timed out with {backlog} events still queued"
            ));
        }
        if pipeline.sink_errors > 0 {
            health.mark_degraded(&format!(
                "event pipeline sink reported {} errors",
                pipeline.sink_errors
            ));
        }
        if pipeline.consumer_state != ConsumerState::Running {
            let snapshot = health.snapshot();
            if snapshot.events_processed != pipeline.normalized {
                health.mark_degraded(&format!(
                    "event accounting mismatch: processed {} but normalized {}",
                    snapshot.events_processed, pipeline.normalized
                ));
            }
            if snapshot.events_dropped != pipeline.invalid + pipeline.dropped_total() {
                health.mark_degraded(&format!(
                    "event accounting mismatch: dropped {} but invalid+queue {}",
                    snapshot.events_dropped,
                    pipeline.invalid + pipeline.dropped_total()
                ));
            }
            let accounted = snapshot
                .events_processed
                .saturating_add(snapshot.events_dropped)
                .saturating_add(snapshot.ignored_events);
            if snapshot.events_received != accounted {
                health.mark_degraded(&format!(
                    "event accounting mismatch: received {} but accounted {}",
                    snapshot.events_received, accounted
                ));
            }
            if pipeline.backlog != 0 {
                health.mark_degraded(&format!(
                    "event pipeline backlog remained at shutdown: {}",
                    pipeline.backlog
                ));
            }
            if snapshot.ignored_events != pipeline.filtered_ignored {
                health.mark_degraded(&format!(
                    "event accounting mismatch: ignored {} but filtered {}",
                    snapshot.ignored_events, pipeline.filtered_ignored
                ));
            }
            let expected_pipeline_received = snapshot
                .events_processed
                .saturating_add(snapshot.events_dropped);
            if pipeline.received != expected_pipeline_received {
                health.mark_degraded(&format!(
                    "event accounting mismatch: pipeline received {} but processed+dropped {}",
                    pipeline.received, expected_pipeline_received
                ));
            }
        }

        if self.session_created && self.trace_opened {
            if let Some(reason) = pairing.coverage_issue() {
                health.mark_degraded(reason);
            }
            if pairing.has_integrity_issue() {
                health.mark_degraded("process pairing integrity issue");
            }
        }

        if let Some(probe) = &self.stop_probe {
            health.set_events_lost(probe.events_lost);
            health.set_realtime_buffers_lost(probe.realtime_buffers_lost);
            if !probe.stopped {
                health.mark_failed("ETW session did not stop cleanly");
            }
            if let Some(code) = probe.stop_rc {
                if code != 0 && code != SESSION_STOPPED_ABSENT {
                    health.mark_degraded(&format!(
                        "ControlTraceW stop returned error: {code}: {}",
                        os_error_text(code)
                    ));
                }
            }
            if probe.events_lost.is_none() {
                health.mark_degraded("ETW event loss count unavailable");
            }
            if probe.realtime_buffers_lost.is_none() {
                health.mark_degraded("ETW realtime buffer loss count unavailable");
            }
            if probe.events_lost.is_some_and(|value| value > 0)
                || probe.realtime_buffers_lost.is_some_and(|value| value > 0)
            {
                health.mark_degraded("ETW event loss observed");
            }
        } else if self.session_created {
            health.mark_degraded("ETW session stop information unavailable");
        }

        if self.residual_session {
            health.mark_failed(
                self.residual_error
                    .as_deref()
                    .unwrap_or("ETW session residual after stop"),
            );
        }
        if let Some(rc) = self.process_trace_rc {
            if rc == u32::MAX {
                health.mark_failed("ProcessTrace worker panicked");
            } else if rc != 0 {
                health.mark_failed(&format!("ProcessTrace failed: {rc}: {}", os_error_text(rc)));
            }
        }
        if let Some(rc) = self.close_trace_rc {
            if rc != 0 {
                health.mark_degraded(&format!("CloseTrace failed: {rc}: {}", os_error_text(rc)));
            }
        }

        let interrupted = self
            .session
            .as_ref()
            .is_some_and(|session| session.was_interrupted());
        if interrupted {
            health.mark_degraded("run interrupted before requested duration");
        } else if self.trace_opened {
            let elapsed = self.collection_elapsed();
            if elapsed.saturating_add(WINDOW_COMPLETION_TOLERANCE) < self.config.duration {
                health.mark_degraded("run ended before requested duration");
            }
        }

        if self.session_created && health.snapshot().events_received == 0 {
            health.mark_degraded("no process events observed");
        }
        if self.session_created {
            health.mark_stopped();
        }
    }

    fn finish(self) -> RunReport {
        let health = self.runtime.health.snapshot();
        let pairing = self
            .runtime
            .pairing
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .snapshot();
        let session_stopped =
            self.session_created && self.stop_probe.as_ref().is_some_and(|probe| probe.stopped);
        let session_stop_error = self.stop_probe.as_ref().and_then(|probe| {
            probe.stop_rc.and_then(|code| {
                (code != 0 && code != SESSION_STOPPED_ABSENT)
                    .then(|| format!("ControlTraceW stop failed: {code}: {}", os_error_text(code)))
            })
        });
        let session_close_error = self.close_trace_rc.and_then(|code| {
            (code != 0).then(|| format!("CloseTrace failed: {code}: {}", os_error_text(code)))
        });
        let process_trace_error = self.process_trace_rc.and_then(|code| {
            if code == u32::MAX {
                Some("ProcessTrace worker panicked".to_owned())
            } else {
                (code != 0).then(|| format!("ProcessTrace failed: {code}: {}", os_error_text(code)))
            }
        });
        let interrupted = self
            .session
            .as_ref()
            .is_some_and(|session| session.was_interrupted());
        let elapsed = self.collection_elapsed();
        let elapsed_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
        let expected_ms = u64::try_from(self.config.duration.as_millis()).unwrap_or(u64::MAX);
        let window_complete = window_is_complete(
            self.failure.is_some(),
            interrupted,
            self.trace_opened,
            self.process_trace_rc,
            self.residual_session,
            elapsed,
            self.config.duration,
        );
        let coverage_active = self.provider_enabled
            && window_complete
            && health.ok
            && pairing.starts_seen > 0
            && pairing.stops_seen > 0;
        let error = self
            .failure
            .as_ref()
            .map(|failure| failure.message.clone())
            .or_else(|| self.residual_error.clone());
        let exit_code = exit_code_for(
            &self.failure,
            self.session_created,
            session_stopped,
            self.residual_session,
            &health,
        );
        let pipeline = self.runtime.pipeline.snapshot();
        let session_id = self.runtime.pipeline.session_id().to_owned();

        RunReport {
            sensor: "SentinelLiteSensor",
            version: env!("CARGO_PKG_VERSION"),
            mode: "realtime",
            etl_output: false,
            ipc_requested: self.config.serve,
            ipc_started: self.ipc_started,
            ipc_pipe: self.config.serve.then_some(sentinel_ipc::DEFAULT_PIPE_NAME),
            ipc_error: self.ipc_error.clone(),
            event_ipc_requested: self.config.serve,
            event_ipc_started: self.event_ipc_started,
            event_ipc_pipe: self
                .config
                .serve
                .then_some(sentinel_ipc::DEFAULT_EVENT_PIPE_NAME),
            event_ipc_error: self.event_ipc_error.clone(),
            delivery: self.runtime.delivery.snapshot(),
            session_name: self.config.session_name.clone(),
            session_created: self.session_created,
            session_stopped,
            trace_opened: self.trace_opened,
            trace_closed: self.trace_closed,
            interrupted,
            elapsed_ms,
            expected_ms,
            window_complete,
            residual_session: self.residual_session,
            session_stop_error,
            session_close_error,
            process_trace_error,
            coverage: if coverage_active {
                vec!["process_start_stop"]
            } else {
                Vec::new()
            },
            protection_claim: "none",
            health,
            pairing,
            session_id,
            pipeline,
            error,
            exit_code,
        }
    }
}

impl Drop for RunContext {
    fn drop(&mut self) {
        self.stop_and_close();
    }
}

fn exit_code_for(
    failure: &Option<Failure>,
    session_created: bool,
    session_stopped: bool,
    residual_session: bool,
    health: &HealthSnapshot,
) -> i32 {
    if residual_session {
        return 5;
    }
    if let Some(failure) = failure {
        return failure.exit_code;
    }
    if session_created && !session_stopped {
        return 5;
    }
    if session_created && !health.ok {
        return 3;
    }
    if session_created { 0 } else { 4 }
}

fn trace_result_failure(process_rc: u32) -> Option<Failure> {
    if process_rc == u32::MAX {
        Some(Failure::etw("ProcessTrace worker panicked"))
    } else if process_rc != 0 {
        Some(Failure::etw(format!(
            "ProcessTrace failed: {process_rc}: {}",
            os_error_text(process_rc)
        )))
    } else {
        None
    }
}

fn window_is_complete(
    failure_present: bool,
    interrupted: bool,
    trace_opened: bool,
    process_trace_rc: Option<u32>,
    residual_session: bool,
    elapsed: Duration,
    expected: Duration,
) -> bool {
    !failure_present
        && !interrupted
        && trace_opened
        && process_trace_rc == Some(0)
        && !residual_session
        && elapsed.saturating_add(WINDOW_COMPLETION_TOLERANCE) >= expected
}

fn merge_max(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn os_error_text(code: u32) -> String {
    std::io::Error::from_raw_os_error(code as i32).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedProcessEvent, EVENT_PROCESS_START, EVENT_PROCESS_STOP, FieldState};
    use crate::health::SensorState;

    fn health_snapshot(ok: bool, state: SensorState) -> HealthSnapshot {
        HealthSnapshot {
            state,
            permission_state: PermissionState::Granted,
            events_received: 1,
            events_processed: 1,
            events_dropped: 0,
            events_lost: Some(0),
            realtime_buffers_lost: Some(0),
            decode_errors: 0,
            partial_events: 0,
            ignored_events: 0,
            last_event_at: None,
            last_error: None,
            degraded: state == SensorState::Degraded,
            ok,
        }
    }

    fn unique_test_session_name() -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        format!("SentinelLite-Sensor-Test-{}-{nanos}", std::process::id())
    }

    #[test]
    fn final_etw_loss_stats_require_a_clean_stop() {
        let config = SensorConfig::new(unique_test_session_name(), Duration::from_secs(1));
        let mut context = RunContext::new(config, Arc::new(Runtime::new()));
        assert_eq!(context.final_etw_loss_stats(), (None, None));

        context.stop_probe = Some(StopProbe {
            stopped: false,
            events_lost: Some(4),
            realtime_buffers_lost: Some(1),
            ..StopProbe::default()
        });
        assert_eq!(context.final_etw_loss_stats(), (None, None));

        context.stop_probe = Some(StopProbe {
            stopped: true,
            stop_rc: Some(0),
            events_lost: Some(4),
            realtime_buffers_lost: None,
        });
        assert_eq!(context.final_etw_loss_stats(), (Some(4), None));

        context.residual_session = true;
        assert_eq!(context.final_etw_loss_stats(), (None, None));
    }

    #[test]
    fn a_successful_stop_retry_replaces_a_failed_probe() {
        let config = SensorConfig::new(unique_test_session_name(), Duration::from_secs(1));
        let mut context = RunContext::new(config, Arc::new(Runtime::new()));
        context.record_stop_probe(StopProbe {
            stopped: false,
            events_lost: Some(9),
            realtime_buffers_lost: None,
            ..StopProbe::default()
        });
        assert_eq!(context.final_etw_loss_stats(), (None, None));

        context.record_stop_probe(StopProbe {
            stopped: true,
            stop_rc: Some(0),
            events_lost: Some(2),
            realtime_buffers_lost: Some(0),
        });
        assert_eq!(context.final_etw_loss_stats(), (Some(2), Some(0)));

        // A later failed retry must not hide the successful stop.
        context.record_stop_probe(StopProbe {
            stopped: false,
            events_lost: None,
            realtime_buffers_lost: None,
            ..StopProbe::default()
        });
        assert_eq!(context.final_etw_loss_stats(), (Some(2), Some(0)));
    }

    #[test]
    fn accepts_default_style_session_name() {
        let config = SensorConfig::new(
            "SentinelLite-Sensor-1234".to_owned(),
            Duration::from_secs(20),
        );
        assert!(config.validate().is_ok());
    }

    #[test]
    fn rejects_empty_or_unsafe_session_name() {
        for name in ["", "with space", "with/slash", "with\nnewline"] {
            let config = SensorConfig::new(name.to_owned(), Duration::from_secs(20));
            assert!(config.validate().is_err(), "name {name:?} must be rejected");
        }
    }

    #[test]
    fn rejects_too_long_session_name() {
        let config = SensorConfig::new(
            "a".repeat(MAX_SESSION_NAME_BYTES + 1),
            Duration::from_secs(1),
        );
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_zero_and_oversized_duration() {
        let zero = SensorConfig::new("SentinelLite-Sensor-1".to_owned(), Duration::ZERO);
        assert!(zero.validate().is_err());
        let oversized = SensorConfig::new(
            "SentinelLite-Sensor-1".to_owned(),
            Duration::from_secs(MAX_DURATION_SECONDS + 1),
        );
        assert!(oversized.validate().is_err());
    }

    #[test]
    fn queue_capacity_is_configurable_and_bounded() {
        use crate::pipeline::{MAX_QUEUE_CAPACITY, MIN_QUEUE_CAPACITY};

        for capacity in [0, MIN_QUEUE_CAPACITY - 1, MAX_QUEUE_CAPACITY + 1] {
            let config =
                SensorConfig::new("SentinelLite-Sensor-1".to_owned(), Duration::from_secs(1))
                    .with_queue_capacity(capacity);
            assert!(
                config.validate().is_err(),
                "capacity {capacity} must be rejected"
            );
        }
        for capacity in [
            MIN_QUEUE_CAPACITY,
            DEFAULT_QUEUE_CAPACITY,
            MAX_QUEUE_CAPACITY,
        ] {
            let config =
                SensorConfig::new("SentinelLite-Sensor-1".to_owned(), Duration::from_secs(1))
                    .with_queue_capacity(capacity);
            assert!(
                config.validate().is_ok(),
                "capacity {capacity} must be accepted"
            );
        }
    }

    #[test]
    fn exit_codes_cover_failure_cleanup_and_degraded_paths() {
        let ok = health_snapshot(true, SensorState::Stopped);
        assert_eq!(exit_code_for(&None, true, true, false, &ok), 0);
        assert_eq!(
            exit_code_for(
                &None,
                true,
                true,
                false,
                &health_snapshot(false, SensorState::Degraded)
            ),
            3
        );
        assert_eq!(exit_code_for(&None, true, true, true, &ok), 5);
        assert_eq!(exit_code_for(&None, true, false, false, &ok), 5);
        assert_eq!(exit_code_for(&None, false, false, false, &ok), 4);

        let permission = Some(Failure::permission("denied"));
        assert_eq!(exit_code_for(&permission, false, false, false, &ok), 2);
        let etw = Some(Failure::etw("start failed"));
        assert_eq!(exit_code_for(&etw, false, false, false, &ok), 4);
        let cleanup = Some(Failure::cleanup("timeout"));
        assert_eq!(exit_code_for(&cleanup, true, true, false, &ok), 5);
    }

    #[test]
    fn trace_result_failure_classifies_worker_results() {
        assert!(trace_result_failure(0).is_none());
        let panicked = trace_result_failure(u32::MAX).expect("panic is a failure");
        assert_eq!(panicked.exit_code, 4);
        let failed = trace_result_failure(5).expect("non-zero is a failure");
        assert_eq!(failed.exit_code, 4);
    }

    #[test]
    fn merge_max_prefers_a_known_larger_value() {
        assert_eq!(merge_max(None, None), None);
        assert_eq!(merge_max(Some(0), None), Some(0));
        assert_eq!(merge_max(None, Some(2)), Some(2));
        assert_eq!(merge_max(Some(2), Some(3)), Some(3));
        assert_eq!(merge_max(Some(3), Some(2)), Some(3));
    }

    #[test]
    fn window_completion_requires_a_full_clean_run() {
        let expected = Duration::from_secs(20);
        assert!(window_is_complete(
            false,
            false,
            true,
            Some(0),
            false,
            expected,
            expected
        ));
        assert!(!window_is_complete(
            true,
            false,
            true,
            Some(0),
            false,
            expected,
            expected
        ));
        assert!(!window_is_complete(
            false,
            false,
            true,
            Some(0),
            false,
            Duration::from_secs(19),
            expected
        ));
        assert!(!window_is_complete(
            false,
            true,
            true,
            Some(0),
            false,
            expected,
            expected
        ));
        assert!(!window_is_complete(
            false,
            false,
            true,
            Some(0),
            true,
            expected,
            expected
        ));
        assert!(!window_is_complete(
            false,
            false,
            true,
            Some(1),
            false,
            expected,
            expected
        ));
    }

    #[test]
    fn missing_session_is_reported_absent() {
        let session = SessionControl::for_test(&unique_test_session_name());
        assert_eq!(session.query_exists(), Ok(false));
    }

    #[test]
    fn stopping_a_missing_session_is_benign() {
        let session = SessionControl::for_test(&unique_test_session_name());
        let probe = session.stop();
        assert!(probe.stopped);
        assert!(probe.events_lost.is_none());
        assert!(probe.realtime_buffers_lost.is_none());
    }

    #[test]
    fn interrupt_sets_the_flag_and_attempts_stop() {
        let session = SessionControl::for_test(&unique_test_session_name());
        assert!(!session.was_interrupted());
        session.interrupt();
        assert!(session.was_interrupted());
        assert!(session.stop().stopped);
    }

    #[test]
    fn pipeline_drops_degrade_the_session() {
        let runtime = Arc::new(Runtime::new());
        runtime.health.mark_starting();
        runtime.health.mark_active();
        runtime.health.set_events_lost(Some(0));
        runtime.health.set_realtime_buffers_lost(Some(0));

        runtime.health.record_received();
        runtime
            .pipeline
            .push_process(process_event(EVENT_PROCESS_START, 42, 7), Some(1_000_000));
        runtime.health.record_received();
        runtime
            .pipeline
            .push_process(process_event(EVENT_PROCESS_STOP, 42, 7), Some(2_000_000));
        let _ = runtime.pipeline.close(SessionEndReason::Completed);

        // The pipeline is closed; this event is counted as a queue drop rather
        // than silently disappearing.
        runtime.health.record_received();
        runtime
            .pipeline
            .push_process(process_event(EVENT_PROCESS_START, 43, 8), Some(3_000_000));

        let config = SensorConfig::new(unique_test_session_name(), Duration::from_secs(1));
        let mut context = RunContext::new(config, Arc::clone(&runtime));
        context.apply_health();

        let snapshot = runtime.health.snapshot();
        assert_eq!(snapshot.state, SensorState::Degraded);
        assert!(
            snapshot
                .last_error
                .as_deref()
                .is_some_and(|error| { error.contains("event pipeline dropped 1 events") })
        );
        assert_eq!(
            snapshot.events_dropped,
            snapshot.events_received - snapshot.events_processed - snapshot.ignored_events
        );
        // ETW loss and pipeline loss remain separate numbers.
        assert_eq!(snapshot.events_lost, Some(0));
        assert_eq!(runtime.pipeline.snapshot().dropped_consumer_gone, 1);
    }

    #[test]
    fn session_end_reason_tracks_failure_and_interrupt() {
        let runtime = Arc::new(Runtime::new());
        let config = SensorConfig::new(unique_test_session_name(), Duration::from_secs(1));
        let mut context = RunContext::new(config, runtime);
        let session = Arc::new(SessionControl::for_test(&unique_test_session_name()));
        session.interrupt();
        context.session = Some(session);

        let (reason, error) = context.session_end_info();
        assert_eq!(reason, SessionEndReason::Interrupted);
        assert!(error.is_none());

        context.failure = Some(Failure::etw("boom"));
        let (reason, error) = context.session_end_info();
        assert_eq!(reason, SessionEndReason::Error);
        assert_eq!(error.as_deref(), Some("boom"));
    }

    #[test]
    fn stop_only_window_is_degraded_even_with_clean_stop() {
        let runtime = Arc::new(Runtime::new());
        {
            let mut pairing = runtime
                .pairing
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            pairing.observe(
                &DecodedProcessEvent {
                    event_id: EVENT_PROCESS_STOP,
                    version: 2,
                    pid: 1,
                    create_time: 2,
                    parent_pid: None,
                    sequence: None,
                    image_name: Some("cmd.exe".to_owned()),
                    image_name_state: FieldState::Present,
                    partial: false,
                },
                1,
            );
        }
        runtime.health.record_received();
        runtime.health.record_processed();

        let start = Instant::now();
        let config = SensorConfig::new(unique_test_session_name(), Duration::from_secs(1));
        let mut context = RunContext::new(config, Arc::clone(&runtime));
        context.session_created = true;
        context.trace_opened = true;
        context.collection_started = Some(start);
        context.collection_ended = Some(start + Duration::from_secs(1));
        context.stop_probe = Some(StopProbe {
            stop_rc: Some(0),
            events_lost: Some(0),
            realtime_buffers_lost: Some(0),
            stopped: true,
        });

        context.apply_health();

        let snapshot = runtime.health.snapshot();
        assert_eq!(snapshot.state, SensorState::Degraded);
        assert!(!snapshot.ok);
        assert_eq!(
            snapshot.last_error.as_deref(),
            Some("no process start events observed")
        );
    }

    fn process_event(event_id: u16, pid: u32, create_time: u64) -> DecodedProcessEvent {
        DecodedProcessEvent {
            event_id,
            version: 2,
            pid,
            create_time,
            parent_pid: None,
            sequence: None,
            image_name: Some("cmd.exe".to_owned()),
            image_name_state: FieldState::Present,
            partial: false,
        }
    }

    fn healthy_context() -> (RunContext, Arc<Runtime>) {
        let runtime = Arc::new(Runtime::new());
        runtime.health.mark_starting();
        runtime.health.mark_active();
        for _ in 0..2 {
            runtime.health.record_received();
            runtime.health.record_processed();
        }
        runtime.health.set_events_lost(Some(0));
        runtime.health.set_realtime_buffers_lost(Some(0));
        {
            let mut pairing = runtime
                .pairing
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            pairing.observe(&process_event(EVENT_PROCESS_START, 42, 7), 1);
            pairing.observe(&process_event(EVENT_PROCESS_STOP, 42, 7), 2);
        }

        let start = Instant::now();
        let config = SensorConfig::new(unique_test_session_name(), Duration::from_secs(1));
        let mut context = RunContext::new(config, Arc::clone(&runtime));
        context.session_created = true;
        context.provider_enabled = true;
        context.trace_opened = true;
        context.trace_closed = true;
        context.process_trace_rc = Some(0);
        context.close_trace_rc = Some(0);
        context.collection_started = Some(start);
        context.collection_ended = Some(start + Duration::from_secs(1));
        context.stop_probe = Some(StopProbe {
            stop_rc: Some(0),
            events_lost: Some(0),
            realtime_buffers_lost: Some(0),
            stopped: true,
        });
        (context, runtime)
    }

    #[test]
    fn healthy_full_window_claims_coverage() {
        let (mut context, _runtime) = healthy_context();
        context.apply_health();
        let report = context.finish();

        assert!(report.window_complete);
        assert!(report.health.ok);
        assert_eq!(report.coverage, vec!["process_start_stop"]);
        assert_eq!(report.exit_code, 0);
    }

    #[test]
    fn timeout_late_result_is_not_a_complete_window() {
        let (mut context, _runtime) = healthy_context();
        context.failure = Some(Failure::cleanup(
            "ProcessTrace did not return within 30s after stop",
        ));
        context
            .runtime
            .health
            .mark_failed("ProcessTrace did not return within 30s after stop");
        let report = context.finish();

        assert!(!report.window_complete);
        assert!(report.coverage.is_empty());
        assert!(!report.health.ok);
        assert_eq!(report.exit_code, 5);
        assert!(report.trace_closed);
        assert!(report.process_trace_error.is_none());
    }

    #[test]
    fn permission_failure_keeps_the_permission_error() {
        let runtime = Arc::new(Runtime::new());
        runtime.health.mark_starting();
        runtime
            .health
            .set_permission(crate::health::PermissionState::Denied);
        runtime
            .health
            .mark_unavailable("StartTraceW failed: 5: access denied");
        let config = SensorConfig::new(unique_test_session_name(), Duration::from_secs(1));
        let mut context = RunContext::new(config, Arc::clone(&runtime));

        context.apply_health();

        let snapshot = runtime.health.snapshot();
        assert_eq!(snapshot.state, SensorState::Unavailable);
        assert_eq!(
            snapshot.last_error.as_deref(),
            Some("StartTraceW failed: 5: access denied")
        );
        assert!(!snapshot.ok);
    }

    fn pairing_snapshot(starts_seen: u64, stops_seen: u64) -> PairingSnapshot {
        PairingSnapshot {
            paired: stops_seen.min(starts_seen),
            unpaired_starts: starts_seen.saturating_sub(stops_seen),
            orphan_stops: stops_seen.saturating_sub(starts_seen),
            duplicate_starts: 0,
            duplicate_stops: 0,
            pid_reuse_detected: 0,
            sequence_mismatches: 0,
            start_overflow: 0,
            orphan_stop_overflow: 0,
            identity_mismatched_stops: 0,
            out_of_order_pairs: 0,
            indeterminate_identities: 0,
            closed_identity_evictions: 0,
            pending_orphan_stops: 0,
            starts_seen,
            stops_seen,
            observed_start_image_names: Vec::new(),
            observed_start_image_names_truncated: false,
        }
    }

    fn pipeline_health_ok() -> PipelineHealth {
        PipelineHealth {
            consumer_state: ConsumerState::Running,
            consumer_error: None,
            dropped_total: 0,
            sink_errors: 0,
            shutdown_timed_out: false,
        }
    }

    #[test]
    fn live_health_requires_active_zero_loss_and_start_coverage() {
        let health = health_snapshot(true, SensorState::Active);
        let pairing = pairing_snapshot(1, 1);
        let pipeline = pipeline_health_ok();
        let zero = Some(LiveStats {
            events_lost: Some(0),
            realtime_buffers_lost: Some(0),
        });
        assert!(live_health_ok(&health, &pairing, zero, &pipeline));

        assert!(!live_health_ok(&health, &pairing, None, &pipeline));
        assert!(!live_health_ok(
            &health,
            &pairing,
            Some(LiveStats {
                events_lost: Some(1),
                realtime_buffers_lost: Some(0),
            }),
            &pipeline
        ));
        assert!(!live_health_ok(
            &health,
            &pairing_snapshot(0, 1),
            zero,
            &pipeline
        ));
        assert!(!live_health_ok(
            &health_snapshot(false, SensorState::Degraded),
            &pairing,
            zero,
            &pipeline
        ));

        let mut dropped = pipeline_health_ok();
        dropped.dropped_total = 1;
        assert!(!live_health_ok(&health, &pairing, zero, &dropped));

        let mut failed = pipeline_health_ok();
        failed.consumer_state = ConsumerState::Failed;
        assert!(!live_health_ok(&health, &pairing, zero, &failed));

        let mut errored = pipeline_health_ok();
        errored.consumer_error = Some("sink failed".to_owned());
        assert!(!live_health_ok(&health, &pairing, zero, &errored));
    }

    struct NoopStatusProvider;

    impl StatusProvider for NoopStatusProvider {
        fn status(&self) -> Result<SensorStatus, String> {
            Err("unused test provider".to_owned())
        }
    }

    #[test]
    fn fatal_ipc_error_from_server_start_stops_the_sensor_lifecycle() {
        let runtime = Arc::new(Runtime::new());
        let config = SensorConfig::new(unique_test_session_name(), Duration::from_secs(1));
        let mut context = RunContext::new(config, Arc::clone(&runtime));
        let session = Arc::new(SessionControl::for_test(&unique_test_session_name()));
        context.session = Some(Arc::clone(&session));
        context.session_created = true;

        let pipe_name = format!(r"\\.\pipe\{}", unique_test_session_name());
        let server_config = ServerConfig::for_testing(pipe_name).with_startup_error_for_testing(
            ServerError::PrivilegeRestore("persistent restore failure".to_owned()),
        );
        let result =
            context.handle_ipc_start(Server::start(server_config, Arc::new(NoopStatusProvider)));
        assert!(result.is_err());
        assert_eq!(runtime.health.snapshot().state, SensorState::Failed);
        assert!(!runtime.health.snapshot().ok);

        context.cleanup();

        assert!(
            session
                .stop_state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_some(),
            "fatal IPC setup must still stop the ETW session"
        );
        assert_eq!(runtime.health.snapshot().state, SensorState::Failed);
    }

    #[test]
    fn drop_attempts_session_cleanup() {
        let runtime = Arc::new(Runtime::new());
        let config = SensorConfig::new(unique_test_session_name(), Duration::from_secs(1));
        let mut context = RunContext::new(config, runtime);
        let session = Arc::new(SessionControl::for_test(&unique_test_session_name()));
        context.session = Some(Arc::clone(&session));
        context.session_created = true;

        drop(context);

        assert!(
            session
                .stop_state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_some()
        );
    }
}
