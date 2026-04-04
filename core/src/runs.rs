// Run orchestration system for yowcode
//
// This module provides run management, task execution, and artifact tracking
// inspired by auto-dev's orchestration system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::types::Project;

/// Configuration for creating a run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    pub project_id: Uuid,
    pub description: String,
    pub branch: Option<String>,
    pub commit_hash: Option<String>,
    pub priority: i32,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            project_id: Uuid::new_v4(),
            description: String::new(),
            branch: None,
            commit_hash: None,
            priority: 0,
            tags: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Status of a run
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    Pending,
    Queued,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// Status of a task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

/// A run represents a single execution of tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub id: Uuid,
    pub project_id: Uuid,
    pub description: String,
    pub status: RunStatus,
    pub branch: Option<String>,
    pub commit_hash: Option<String>,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
}

impl Run {
    pub fn new(config: RunConfig) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id: config.project_id,
            description: config.description,
            status: RunStatus::Pending,
            branch: config.branch,
            commit_hash: config.commit_hash,
            priority: config.priority,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            tags: config.tags,
            metadata: config.metadata,
        }
    }

    pub fn start(&mut self) {
        self.status = RunStatus::Running;
        self.started_at = Some(Utc::now());
    }

    pub fn complete(&mut self) {
        self.status = RunStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    pub fn fail(&mut self) {
        self.status = RunStatus::Failed;
        self.completed_at = Some(Utc::now());
    }

    pub fn cancel(&mut self) {
        self.status = RunStatus::Cancelled;
        self.completed_at = Some(Utc::now());
    }
}

/// A task is a unit of work within a run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub run_id: Uuid,
    pub name: String,
    pub description: String,
    pub status: TaskStatus,
    pub command: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub output: Option<String>,
    pub dependencies: Vec<Uuid>,
}

impl Task {
    pub fn new(run_id: Uuid, name: String, description: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            run_id,
            name,
            description,
            status: TaskStatus::Pending,
            command: None,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
            output: None,
            dependencies: Vec::new(),
        }
    }
}

/// An artifact produced by a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: Uuid,
    pub task_id: Uuid,
    pub run_id: Uuid,
    pub name: String,
    pub path: String,
    pub artifact_type: String,
    pub size_bytes: Option<u64>,
    pub created_at: DateTime<Utc>,
}

/// An audit event for tracking run activities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: Uuid,
    pub run_id: Uuid,
    pub event_type: String,
    pub description: String,
    pub user_id: Option<String>,
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
}

/// Statistics for runs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStats {
    pub total_runs: usize,
    pub pending_runs: usize,
    pub running_runs: usize,
    pub completed_runs: usize,
    pub failed_runs: usize,
    pub cancelled_runs: usize,
}

/// Events that can be emitted during run execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RunEvent {
    RunCreated { run_id: Uuid },
    RunStarted { run_id: Uuid },
    RunCompleted { run_id: Uuid, success: bool },
    TaskCreated { run_id: Uuid, task_id: Uuid },
    TaskStarted { run_id: Uuid, task_id: Uuid },
    TaskCompleted { run_id: Uuid, task_id: Uuid, success: bool },
    ArtifactCreated { run_id: Uuid, task_id: Uuid, artifact_id: Uuid },
    RunCancelled { run_id: Uuid },
}

/// A handle to control an active run
#[derive(Clone)]
pub struct RunHandle {
    pub run_id: Uuid,
    cancel_tx: mpsc::Sender<()>,
}

impl RunHandle {
    pub fn new(run_id: Uuid, cancel_tx: mpsc::Sender<()>) -> Self {
        Self { run_id, cancel_tx }
    }

    pub async fn cancel(&self) -> Result<()> {
        self.cancel_tx
            .send(())
            .await
            .map_err(|_| Error::RunError("Failed to send cancel signal".to_string()))?;
        Ok(())
    }
}

/// Queue for managing pending runs
pub struct RunQueue {
    queue: Arc<RwLock<Vec<Run>>>,
}

impl RunQueue {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn enqueue(&self, run: Run) -> Result<()> {
        let mut queue = self.queue.write().await;
        queue.push(run);
        queue.sort_by(|a, b| b.priority.cmp(&a.priority)); // Higher priority first
        Ok(())
    }

    pub async fn dequeue(&self) -> Option<Run> {
        let mut queue = self.queue.write().await;
        queue.pop()
    }

    pub async fn peek(&self) -> Option<Run> {
        let queue = self.queue.read().await;
        queue.last().cloned()
    }

    pub async fn len(&self) -> usize {
        self.queue.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.queue.read().await.is_empty()
    }
}

impl Default for RunQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Manager for run orchestration
pub struct RunManager {
    runs: Arc<RwLock<HashMap<Uuid, Run>>>,
    tasks: Arc<RwLock<HashMap<Uuid, Task>>>,
    artifacts: Arc<RwLock<HashMap<Uuid, Artifact>>>,
    audit_events: Arc<RwLock<Vec<AuditEvent>>>,
    projects: Arc<RwLock<HashMap<Uuid, Project>>>,
    queue: RunQueue,
    event_tx: broadcast::Sender<RunEvent>,
    active_runs: Arc<RwLock<HashMap<Uuid, RunHandle>>>,
}

impl RunManager {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        Self {
            runs: Arc::new(RwLock::new(HashMap::new())),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            artifacts: Arc::new(RwLock::new(HashMap::new())),
            audit_events: Arc::new(RwLock::new(Vec::new())),
            projects: Arc::new(RwLock::new(HashMap::new())),
            queue: RunQueue::new(),
            event_tx,
            active_runs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_project(&self, project: Project) -> Result<()> {
        let mut projects = self.projects.write().await;
        projects.insert(project.id, project);
        Ok(())
    }

    pub async fn create_run(&self, config: RunConfig) -> Result<Uuid> {
        let run = Run::new(config);
        let run_id = run.id;

        // Verify project exists
        let projects = self.projects.read().await;
        if !projects.contains_key(&run.project_id) {
            return Err(Error::RunError(format!(
                "Project {} not found",
                run.project_id
            )));
        }
        drop(projects);

        // Store the run
        let mut runs = self.runs.write().await;
        runs.insert(run_id, run.clone());

        // Emit event
        let _ = self.event_tx.send(RunEvent::RunCreated { run_id });

        // Add to queue
        self.queue.enqueue(run).await?;

        Ok(run_id)
    }

    pub async fn get_run(&self, run_id: Uuid) -> Result<Run> {
        let runs = self.runs.read().await;
        runs.get(&run_id)
            .cloned()
            .ok_or_else(|| Error::RunError(format!("Run {} not found", run_id)))
    }

    pub async fn list_runs(&self, project_id: Option<Uuid>) -> Result<Vec<Run>> {
        let runs = self.runs.read().await;
        if let Some(pid) = project_id {
            Ok(runs
                .values()
                .filter(|r| r.project_id == pid)
                .cloned()
                .collect())
        } else {
            Ok(runs.values().cloned().collect())
        }
    }

    pub async fn cancel_run(&self, run_id: Uuid) -> Result<()> {
        let mut runs = self.runs.write().await;
        if let Some(run) = runs.get_mut(&run_id) {
            match run.status {
                RunStatus::Running | RunStatus::Queued => {
                    run.cancel();
                    let _ = self.event_tx.send(RunEvent::RunCancelled { run_id });

                    // Cancel active run if exists
                    let handle = {
                        let active = self.active_runs.read().await;
                        active.get(&run_id).cloned()
                    };
                    if let Some(handle) = handle {
                        handle.cancel().await?;
                    }

                    Ok(())
                }
                _ => Err(Error::RunError(format!(
                    "Cannot cancel run in {:?} status",
                    run.status
                ))),
            }
        } else {
            Err(Error::RunError(format!("Run {} not found", run_id)))
        }
    }

    pub async fn create_task(&self, run_id: Uuid, task: Task) -> Result<Uuid> {
        let task_id = task.id;

        // Verify run exists
        let runs = self.runs.read().await;
        if !runs.contains_key(&run_id) {
            return Err(Error::RunError(format!("Run {} not found", run_id)));
        }
        drop(runs);

        let mut tasks = self.tasks.write().await;
        tasks.insert(task_id, task.clone());

        let _ = self.event_tx.send(RunEvent::TaskCreated { run_id, task_id });

        Ok(task_id)
    }

    pub async fn get_task(&self, task_id: Uuid) -> Result<Task> {
        let tasks = self.tasks.read().await;
        tasks
            .get(&task_id)
            .cloned()
            .ok_or_else(|| Error::RunError(format!("Task {} not found", task_id)))
    }

    pub async fn list_tasks(&self, run_id: Uuid) -> Result<Vec<Task>> {
        let tasks = self.tasks.read().await;
        Ok(tasks
            .values()
            .filter(|t| t.run_id == run_id)
            .cloned()
            .collect())
    }

    pub async fn create_artifact(&self, artifact: Artifact) -> Result<Uuid> {
        let artifact_id = artifact.id;
        let run_id = artifact.run_id;
        let task_id = artifact.task_id;

        let mut artifacts = self.artifacts.write().await;
        artifacts.insert(artifact_id, artifact);

        let _ = self
            .event_tx
            .send(RunEvent::ArtifactCreated { run_id, task_id, artifact_id });

        Ok(artifact_id)
    }

    pub async fn list_artifacts(&self, run_id: Uuid) -> Result<Vec<Artifact>> {
        let artifacts = self.artifacts.read().await;
        Ok(artifacts
            .values()
            .filter(|a| a.run_id == run_id)
            .cloned()
            .collect())
    }

    pub async fn get_audit_events(&self, run_id: Option<Uuid>) -> Result<Vec<AuditEvent>> {
        let events = self.audit_events.read().await;
        if let Some(rid) = run_id {
            Ok(events
                .iter()
                .filter(|e| e.run_id == rid)
                .cloned()
                .collect())
        } else {
            Ok(events.clone())
        }
    }

    pub async fn get_stats(&self) -> RunStats {
        let runs = self.runs.read().await;
        let total_runs = runs.len();
        let pending_runs = runs.values().filter(|r| r.status == RunStatus::Pending).count();
        let running_runs = runs.values().filter(|r| r.status == RunStatus::Running).count();
        let completed_runs = runs
            .values()
            .filter(|r| r.status == RunStatus::Completed)
            .count();
        let failed_runs = runs.values().filter(|r| r.status == RunStatus::Failed).count();
        let cancelled_runs = runs
            .values()
            .filter(|r| r.status == RunStatus::Cancelled)
            .count();

        RunStats {
            total_runs,
            pending_runs,
            running_runs,
            completed_runs,
            failed_runs,
            cancelled_runs,
        }
    }

    pub async fn process_run(&self, run_id: Uuid) -> Result<()> {
        // Get the run
        let mut runs = self.runs.write().await;
        let run = runs
            .get_mut(&run_id)
            .ok_or_else(|| Error::RunError(format!("Run {} not found", run_id)))?;

        match run.status {
            RunStatus::Pending | RunStatus::Queued => {
                run.status = RunStatus::Queued;
                drop(runs);
                Ok(())
            }
            _ => Err(Error::RunError(format!(
                "Run {} is not in a processable state",
                run_id
            ))),
        }
    }

    pub async fn subscribe_events(&self) -> broadcast::Receiver<RunEvent> {
        self.event_tx.subscribe()
    }
}

impl Default for RunManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Executor for running individual runs
pub struct RunExecutor {
    manager: Arc<RunManager>,
}

impl RunExecutor {
    pub fn new(manager: Arc<RunManager>) -> Self {
        Self { manager }
    }

    pub async fn execute_run(&self, run_id: Uuid) -> Result<()> {
        // Mark run as running
        {
            let mut runs = self.manager.runs.write().await;
            if let Some(run) = runs.get_mut(&run_id) {
                run.start();
            } else {
                return Err(Error::RunError(format!("Run {} not found", run_id)));
            }
        }

        let _ = self
            .manager
            .event_tx
            .send(RunEvent::RunStarted { run_id });

        // Create cancellation channel
        let (cancel_tx, mut cancel_rx) = mpsc::channel::<()>(1);

        // Store run handle
        let handle = RunHandle::new(run_id, cancel_tx);
        let mut active = self.manager.active_runs.write().await;
        active.insert(run_id, handle);
        drop(active);

        // Execute tasks (placeholder logic)
        let tasks = self.manager.list_tasks(run_id).await?;
        let mut success = true;

        for task in tasks {
            tokio::select! {
                result = self.execute_task(&task) => {
                    if let Err(e) = result {
                        success = false;
                        eprintln!("Task {} failed: {}", task.id, e);
                    }
                }
                _ = cancel_rx.recv() => {
                    self.manager.cancel_run(run_id).await?;
                    return Ok(());
                }
            }
        }

        // Mark run as completed
        {
            let mut runs = self.manager.runs.write().await;
            if let Some(run) = runs.get_mut(&run_id) {
                if success {
                    run.complete();
                } else {
                    run.fail();
                }
            }
        }

        let _ = self
            .manager
            .event_tx
            .send(RunEvent::RunCompleted { run_id, success });

        // Remove from active runs
        let mut active = self.manager.active_runs.write().await;
        active.remove(&run_id);

        Ok(())
    }

    async fn execute_task(&self, task: &Task) -> Result<()> {
        // Update task status to in progress
        {
            let mut tasks = self.manager.tasks.write().await;
            if let Some(t) = tasks.get_mut(&task.id) {
                t.status = TaskStatus::InProgress;
                t.started_at = Some(Utc::now());
            }
        }

        let _ = self.manager.event_tx.send(RunEvent::TaskStarted {
            run_id: task.run_id,
            task_id: task.id,
        });

        // Simulate task execution
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Mark task as completed
        {
            let mut tasks = self.manager.tasks.write().await;
            if let Some(t) = tasks.get_mut(&task.id) {
                t.status = TaskStatus::Completed;
                t.completed_at = Some(Utc::now());
            }
        }

        let _ = self.manager.event_tx.send(RunEvent::TaskCompleted {
            run_id: task.run_id,
            task_id: task.id,
            success: true,
        });

        Ok(())
    }
}

/// Monitor for tracking run statistics and status
pub struct RunMonitor {
    manager: Arc<RunManager>,
}

impl RunMonitor {
    pub fn new(manager: Arc<RunManager>) -> Self {
        Self { manager }
    }

    pub async fn get_stats(&self) -> RunStats {
        self.manager.get_stats().await
    }

    pub async fn watch_events(&self) -> broadcast::Receiver<RunEvent> {
        self.manager.subscribe_events().await
    }

    pub async fn is_run_active(&self, run_id: Uuid) -> bool {
        let active = self.manager.active_runs.read().await;
        active.contains_key(&run_id)
    }
}
