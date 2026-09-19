use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use encoding_rs::{GB18030, UTF_8};
use uuid::Uuid;

use super::files::{self, HostWorkspace, failure};
use super::process::ExportProcess;
use super::*;
use crate::{CoreError, ExportConflictPolicy, LocalIndex, OperationResult, Result};

const RETENTION: Duration = Duration::from_secs(1800);
const LOG_LIMIT: usize = 1024 * 1024;

struct TaskState {
    created: Instant,
    view: CustomExportTask,
    logs: VecDeque<ExportLog>,
    log_bytes: usize,
    decision: Option<ExportConflictPolicy>,
    finished: Option<Instant>,
}

struct Task {
    state: Mutex<TaskState>,
    changed: Condvar,
    caller: Caller,
}

impl Task {
    fn check(&self) -> Result<()> {
        if self.state.lock().unwrap().view.cancel_requested {
            Err(failure("cancelled", "自定义导出已取消"))
        } else {
            Ok(())
        }
    }

    fn status(&self, status: TaskStatus) {
        self.state.lock().unwrap().view.status = status;
    }

    fn log(&self, source: &str, text: String) {
        if text.is_empty() {
            return;
        }
        let mut state = self.state.lock().unwrap();
        state.view.next_cursor += 1;
        let sequence = state.view.next_cursor;
        state.log_bytes += text.len();
        state.logs.push_back(ExportLog {
            sequence,
            source: source.into(),
            text,
        });
        while state.log_bytes > LOG_LIMIT {
            if let Some(log) = state.logs.pop_front() {
                state.log_bytes -= log.text.len();
            }
        }
    }

    fn snapshot(&self, cursor: u64) -> CustomExportTask {
        let state = self.state.lock().unwrap();
        let mut view = state.view.clone();
        view.logs = state
            .logs
            .iter()
            .filter(|log| log.sequence > cursor)
            .cloned()
            .collect();
        view.logs_truncated = state
            .logs
            .front()
            .is_some_and(|log| cursor.saturating_add(1) < log.sequence);
        view
    }

    fn finish(&self, outcome: Result<OperationResult>) {
        let mut state = self.state.lock().unwrap();
        match outcome {
            Ok(result) => {
                state.view.status = TaskStatus::Succeeded;
                state.view.result = Some(result);
            }
            Err(error) => {
                state.view.status = if error.code() == "cancelled" {
                    TaskStatus::Cancelled
                } else {
                    TaskStatus::Failed
                };
                let exit_code = state.view.error.as_ref().and_then(|e| e.exit_code);
                state.view.error = Some(TaskFailure {
                    code: error.code().into(),
                    message: error.to_string(),
                    exit_code,
                });
            }
        }
        state.finished = Some(Instant::now());
        self.changed.notify_all();
    }
}

struct ServiceInner {
    index: LocalIndex,
    workspace: Arc<HostWorkspace>,
    tasks: Mutex<HashMap<String, Arc<Task>>>,
}

impl Drop for ServiceInner {
    fn drop(&mut self) {
        for task in self.tasks.lock().unwrap().values() {
            let mut state = task.state.lock().unwrap();
            if !state.view.status.terminal() {
                state.view.cancel_requested = true;
                task.changed.notify_all();
            }
        }
    }
}

#[derive(Clone)]
pub struct CustomExportService(Arc<ServiceInner>);

impl CustomExportService {
    pub fn new(index: LocalIndex) -> Result<Self> {
        let workspace = Arc::new(HostWorkspace::create(&index)?);
        let inner = Arc::new(ServiceInner {
            index,
            workspace,
            tasks: Mutex::new(HashMap::new()),
        });
        let weak = Arc::downgrade(&inner);
        thread::Builder::new()
            .name("export-retention".into())
            .spawn(move || {
                loop {
                    thread::sleep(Duration::from_secs(15));
                    let Some(inner) = weak.upgrade() else {
                        break;
                    };
                    inner.tasks.lock().unwrap().retain(|_, task| {
                        !task
                            .state
                            .lock()
                            .unwrap()
                            .finished
                            .is_some_and(|time| time.elapsed() >= RETENTION)
                    });
                }
            })
            .map_err(|e| failure("start_failed", e.to_string()))?;
        Ok(Self(inner))
    }

    pub fn profiles(&self, caller: Caller) -> Result<Vec<CustomExportProfile>> {
        Ok(ProfileStore::new(self.0.index.clone())
            .list()?
            .into_iter()
            .filter(|profile| caller == Caller::Desktop || (profile.enabled && profile.allow_mcp))
            .collect())
    }

    pub fn start(
        &self,
        caller: Caller,
        request: StartCustomExportRequest,
    ) -> Result<CustomExportTask> {
        let profile = ProfileStore::new(self.0.index.clone())
            .list()?
            .into_iter()
            .find(|profile| profile.id == request.profile_id)
            .ok_or_else(|| failure("profile_not_found", "找不到自定义导出方案"))?;
        if caller == Caller::Mcp && !profile.allow_mcp {
            return Err(failure("not_authorized", "此方案尚未获得桌面端 MCP 授权"));
        }
        if !profile.enabled {
            return Err(failure("profile_disabled", "此方案已停用"));
        }
        profile.validate()?;
        if !profile.executable.is_file() {
            return Err(failure("start_failed", "打包程序不存在"));
        }
        if let Some(path) = &profile.working_directory {
            files::directory(path)?;
        }
        let source = self
            .0
            .index
            .component_path(&request.component_id)?
            .ok_or_else(|| CoreError::InvalidInput("找不到组件 ID".into()))?;
        let source = files::directory(&source)?;
        let destination = files::directory(&request.destination)?;
        files::outside(&source, &destination)?;
        files::outside(&source, &files::directory(self.0.workspace.root.path())?)?;
        let (kind, fallback_name) = crate::operations::inspect_export(&source)?;
        if !profile.component_kinds.contains(&kind) {
            return Err(failure("incompatible_profile", "方案不适用于此组件类型"));
        }
        let name = crate::metadata::read_component_metadata(&source)
            .ok()
            .flatten()
            .map(|metadata| metadata.display_name)
            .unwrap_or(fallback_name);
        let id = Uuid::new_v4().to_string();
        let task = Arc::new(Task {
            caller,
            changed: Condvar::new(),
            state: Mutex::new(TaskState {
                created: Instant::now(),
                view: CustomExportTask {
                    id: id.clone(),
                    component_id: request.component_id.clone(),
                    profile_id: profile.id.clone(),
                    profile_name: profile.name.clone(),
                    destination: destination.clone(),
                    status: TaskStatus::Preparing,
                    cancel_requested: false,
                    conflict_path: None,
                    result: None,
                    error: None,
                    logs: Vec::new(),
                    next_cursor: 0,
                    logs_truncated: false,
                },
                logs: VecDeque::new(),
                log_bytes: 0,
                decision: None,
                finished: None,
            }),
        });
        let mut tasks = self.0.tasks.lock().unwrap();
        if tasks.values().any(|task| {
            let state = task.state.lock().unwrap();
            !state.view.status.terminal() && state.view.component_id == request.component_id
        }) {
            return Err(CoreError::Busy);
        }
        let initial = task.snapshot(0);
        tasks.insert(id.clone(), task.clone());
        let index = self.0.index.clone();
        let workspace = self.0.workspace.clone();
        let spawn = thread::Builder::new()
            .name(format!("export-{id}"))
            .spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    execute(
                        &task,
                        &index,
                        &workspace,
                        &profile,
                        &source,
                        &destination,
                        &name,
                        kind,
                        request.conflict_policy,
                    )
                }))
                .unwrap_or_else(|_| Err(failure("execution_failed", "导出工作线程意外终止")));
                task.finish(outcome);
            });
        if let Err(error) = spawn {
            tasks.remove(&id);
            return Err(failure("start_failed", error.to_string()));
        }
        Ok(initial)
    }

    fn task(&self, caller: Caller, id: &str) -> Result<Arc<Task>> {
        let task = self
            .0
            .tasks
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| failure("task_not_found", "任务不存在、已过期或属于其他宿主进程"))?;
        if task.caller != caller {
            return Err(failure("not_authorized", "不能操作其他调用方的任务"));
        }
        Ok(task)
    }

    pub fn get(&self, caller: Caller, id: &str, cursor: u64) -> Result<CustomExportTask> {
        Ok(self.task(caller, id)?.snapshot(cursor))
    }

    pub fn list_tasks(&self, caller: Caller) -> Vec<CustomExportTask> {
        let tasks: Vec<_> = self
            .0
            .tasks
            .lock()
            .unwrap()
            .values()
            .filter(|task| task.caller == caller)
            .cloned()
            .collect();
        let mut ordered: Vec<_> = tasks
            .iter()
            .map(|task| {
                let created = task.state.lock().unwrap().created;
                (created, task.snapshot(u64::MAX))
            })
            .collect();
        ordered.sort_by_key(|(created, _)| *created);
        ordered.into_iter().map(|(_, view)| view).collect()
    }

    pub fn cancel(&self, caller: Caller, id: &str) -> Result<CustomExportTask> {
        let task = self.task(caller, id)?;
        {
            let mut state = task.state.lock().unwrap();
            if !state.view.status.terminal() {
                state.view.cancel_requested = true;
                task.changed.notify_all();
            }
        }
        Ok(task.snapshot(u64::MAX))
    }

    pub fn resolve_conflict(
        &self,
        caller: Caller,
        id: &str,
        policy: ExportConflictPolicy,
    ) -> Result<CustomExportTask> {
        if policy == ExportConflictPolicy::Error {
            return Err(CoreError::InvalidInput("请选择追加序号或覆盖".into()));
        }
        let task = self.task(caller, id)?;
        {
            let mut state = task.state.lock().unwrap();
            if state.view.status != TaskStatus::AwaitingConflict || state.view.cancel_requested {
                return Err(failure("invalid_task_state", "任务当前未等待重名处理"));
            }
            if state.decision.is_some() {
                return Err(failure("invalid_task_state", "已提交重名处理选择"));
            }
            state.decision = Some(policy);
            task.changed.notify_all();
        }
        Ok(task.snapshot(u64::MAX))
    }

    pub fn shutdown(&self) {
        let tasks: Vec<_> = self.0.tasks.lock().unwrap().values().cloned().collect();
        for task in &tasks {
            let mut state = task.state.lock().unwrap();
            if !state.view.status.terminal() {
                state.view.cancel_requested = true;
                task.changed.notify_all();
            }
        }
        for task in tasks {
            let mut state = task.state.lock().unwrap();
            while !state.view.status.terminal() {
                state = task.changed.wait(state).unwrap();
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute(
    task: &Arc<Task>,
    index: &LocalIndex,
    host: &HostWorkspace,
    profile: &CustomExportProfile,
    source: &Path,
    destination: &Path,
    name: &str,
    kind: crate::ComponentKind,
    policy: ExportConflictPolicy,
) -> Result<OperationResult> {
    let started = Instant::now();
    let check = || {
        task.check()?;
        if started.elapsed().as_secs() >= profile.timeout_seconds {
            return Err(failure("timed_out", "自定义导出超时"));
        }
        Ok(())
    };
    check()?;
    let root = tempfile::Builder::new()
        .prefix("task-")
        .tempdir_in(host.root.path())
        .map_err(|e| failure("prepare_failed", e.to_string()))?;
    let (output, work) = (root.path().join("output"), root.path().join("work"));
    for path in [&output, &work] {
        fs::create_dir(path).map_err(|e| CoreError::io(path, e))?;
    }
    let mut guard = Some(index.try_lock_mutations()?);
    if profile.input_mode == InputMode::Source {
        crate::mcdk_session::assert_component_idle(index, source)?;
    }
    let input = if profile.input_mode == InputMode::Snapshot {
        task.log("system", "正在准备完整临时副本\n".into());
        let input = root.path().join("input");
        files::copy_snapshot(source, &input, &check)?;
        guard.take();
        input
    } else {
        task.log(
            "system",
            "原目录模式：外部程序对源文件的修改无法撤销\n".into(),
        );
        source.to_path_buf()
    };
    check()?;
    let component_id = task.state.lock().unwrap().view.component_id.clone();
    let kind = match kind {
        crate::ComponentKind::Addon => "addon",
        crate::ComponentKind::Map => "map",
        crate::ComponentKind::Material => "material",
    };
    let values = [
        ("input_dir", input.to_string_lossy().into_owned()),
        ("output_dir", output.to_string_lossy().into_owned()),
        ("work_dir", work.to_string_lossy().into_owned()),
        ("component_id", component_id),
        ("component_name", name.into()),
        ("component_kind", kind.into()),
    ];
    let arguments = profile
        .arguments
        .iter()
        .map(|arg| substitute(arg, &values))
        .collect::<Vec<_>>();
    let mut environment: Vec<_> = values
        .iter()
        .map(|(key, value)| (format!("MCDH_{}", key.to_uppercase()), value.clone()))
        .collect();
    environment.push(("MCDH_EXPORT_PROTOCOL_VERSION".into(), "1".into()));
    let cwd = profile.working_directory.as_deref().unwrap_or(&input);
    task.status(TaskStatus::Running);
    let mut process = ExportProcess::start(&profile.executable, &arguments, cwd, &environment)?;
    let readers = [
        read_log(
            process.stdout.take().unwrap(),
            "stdout",
            profile.log_encoding,
            task.clone(),
        ),
        read_log(
            process.stderr.take().unwrap(),
            "stderr",
            profile.log_encoding,
            task.clone(),
        ),
    ];
    let outcome = loop {
        if let Err(error) = check() {
            break Err(error);
        }
        match process.poll() {
            Ok(Some(code)) => break Ok(code),
            Ok(None) => thread::sleep(Duration::from_millis(40)),
            Err(error) => break Err(error),
        }
    };
    let termination = process.terminate();
    drop(process);
    for reader in readers {
        let _ = reader.join();
    }
    guard.take();
    termination?;
    let code = outcome?;
    if code != 0 {
        task.state.lock().unwrap().view.error = Some(TaskFailure {
            code: "execution_failed".into(),
            message: String::new(),
            exit_code: Some(code),
        });
        return Err(failure(
            "execution_failed",
            format!("打包程序退出码：{code}"),
        ));
    }
    task.check()?;
    task.status(TaskStatus::Validating);
    let artifact = files::artifact(&output)?;
    publish(task, &artifact, destination, policy)
}

fn substitute(template: &str, values: &[(&str, String)]) -> String {
    let mut result = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        result.push_str(&rest[..start]);
        let candidate = &rest[start..];
        if let Some(end) = candidate.find('}') {
            let token = &candidate[1..end];
            if let Some((_, value)) = values.iter().find(|(key, _)| *key == token) {
                result.push_str(value);
            } else {
                result.push_str(&candidate[..=end]);
            }
            rest = &candidate[end + 1..];
        } else {
            result.push_str(candidate);
            rest = "";
            break;
        }
    }
    result.push_str(rest);
    result
}

fn read_log(
    mut pipe: File,
    source: &'static str,
    encoding: LogEncoding,
    task: Arc<Task>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut decoder = match encoding {
            LogEncoding::Utf8 => UTF_8,
            LogEncoding::Gb18030 => GB18030,
        }
        .new_decoder();
        let mut bytes = [0; 4096];
        loop {
            let read = match pipe.read(&mut bytes) {
                Ok(read) => read,
                Err(error) => {
                    task.log("system", format!("{source} 读取失败：{error}\n"));
                    break;
                }
            };
            let mut text = String::with_capacity(
                decoder
                    .max_utf8_buffer_length(read)
                    .unwrap_or(read * 4 + 16),
            );
            let _ = decoder.decode_to_string(&bytes[..read], &mut text, read == 0);
            task.log(source, text);
            if read == 0 {
                break;
            }
        }
    })
}

fn wait_conflict(task: &Task, path: &Path) -> Result<ExportConflictPolicy> {
    let mut state = task.state.lock().unwrap();
    state.view.status = TaskStatus::AwaitingConflict;
    state.view.conflict_path = Some(path.to_path_buf());
    let deadline = Instant::now() + RETENTION;
    loop {
        if state.view.cancel_requested {
            return Err(failure("cancelled", "自定义导出已取消"));
        }
        if let Some(policy) = state.decision.take() {
            state.view.conflict_path = None;
            return Ok(policy);
        }
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Err(failure(
                "conflict_expired",
                "重名处理已超过 30 分钟，临时产物已清理",
            ));
        };
        state = task.changed.wait_timeout(state, remaining).unwrap().0;
    }
}

fn publish(
    task: &Task,
    artifact: &Path,
    destination: &Path,
    mut policy: ExportConflictPolicy,
) -> Result<OperationResult> {
    let requested = destination.join(artifact.file_name().unwrap());
    if policy == ExportConflictPolicy::Error && fs::symlink_metadata(&requested).is_ok() {
        policy = wait_conflict(task, &requested)?;
    }
    task.check()?;
    task.status(TaskStatus::Publishing);
    let mut temporary = files::prepare_publish(artifact, destination, &|| task.check())?;
    let mut target = requested.clone();
    let mut suffix = 2;
    loop {
        // Cancellation and final publication have one linearization point under this lock.
        let mut state = task.state.lock().unwrap();
        if state.view.cancel_requested {
            return Err(failure("cancelled", "自定义导出已取消"));
        }
        if policy == ExportConflictPolicy::Overwrite
            && fs::symlink_metadata(&target).is_ok_and(|m| !m.is_file() || files::is_link(&m))
        {
            return Err(failure("publish_failed", "拒绝覆盖目录或链接"));
        }
        let persisted = if policy == ExportConflictPolicy::Overwrite {
            temporary.persist(&target)
        } else {
            temporary.persist_noclobber(&target)
        };
        match persisted {
            Ok(_) => {
                let result = OperationResult {
                    component: None,
                    actual_path: target.clone(),
                    modified_files: vec![target],
                    warnings: Vec::new(),
                };
                state.view.status = TaskStatus::Succeeded;
                state.view.result = Some(result.clone());
                return Ok(result);
            }
            Err(error) => {
                let exists = error.error.kind() == std::io::ErrorKind::AlreadyExists
                    || fs::symlink_metadata(&target).is_ok();
                temporary = error.file;
                if !exists || policy == ExportConflictPolicy::Overwrite {
                    return Err(failure("publish_failed", error.error.to_string()));
                }
                drop(state);
                if policy == ExportConflictPolicy::Rename {
                    target = files::numbered(&requested, suffix);
                    suffix += 1;
                } else {
                    policy = wait_conflict(task, &requested)?;
                    task.status(TaskStatus::Publishing);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
