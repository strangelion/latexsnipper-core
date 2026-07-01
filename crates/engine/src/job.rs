use log::info;
use std::collections::VecDeque;

/// Status of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// A unit of work to be executed by the engine.
pub struct Job {
    pub id: String,
    pub name: String,
    pub status: JobStatus,
    pub result: Option<String>,
}

impl Job {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            status: JobStatus::Pending,
            result: None,
        }
    }
}

/// A queue for managing jobs.
pub struct JobQueue {
    pending: VecDeque<Job>,
    active: HashMap<String, Job>,
    completed: HashMap<String, Job>,
}

impl JobQueue {
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            active: HashMap::new(),
            completed: HashMap::new(),
        }
    }

    /// Submit a job to the queue.
    pub fn submit(&mut self, job: Job) {
        info!("Job '{}' submitted: {}", job.id, job.name);
        self.pending.push_back(job);
    }

    /// Get the next job to execute.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Job> {
        self.pending.pop_front()
    }

    /// Mark a job as running.
    pub fn start(&mut self, job: Job) {
        let id = job.id.clone();
        info!("Job '{}' started", id);
        self.active.insert(id, job);
    }

    /// Mark a job as completed.
    pub fn complete(&mut self, id: &str, result: String) {
        if let Some(mut job) = self.active.remove(id) {
            job.status = JobStatus::Completed;
            job.result = Some(result);
            info!("Job '{}' completed", id);
            self.completed.insert(id.to_string(), job);
        }
    }

    /// Mark a job as failed.
    pub fn fail(&mut self, id: &str, error: String) {
        if let Some(mut job) = self.active.remove(id) {
            job.status = JobStatus::Failed;
            job.result = Some(error);
            info!("Job '{}' failed", id);
            self.completed.insert(id.to_string(), job);
        }
    }

    /// Cancel a job.
    pub fn cancel(&mut self, id: &str) {
        if let Some(mut job) = self.active.remove(id) {
            job.status = JobStatus::Cancelled;
            info!("Job '{}' cancelled", id);
            self.completed.insert(id.to_string(), job);
        } else if let Some(job) = self.pending.iter_mut().find(|j| j.id == id) {
            job.status = JobStatus::Cancelled;
        }
    }

    /// Get the number of pending jobs.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Get the number of active jobs.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Get a job by ID.
    pub fn get(&self, id: &str) -> Option<&Job> {
        self.active.get(id).or_else(|| self.completed.get(id))
    }
}

impl Default for JobQueue {
    fn default() -> Self {
        Self::new()
    }
}

use std::collections::HashMap;
