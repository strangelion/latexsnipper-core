use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};

/// User-visible urgency used by the deterministic runtime queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WorkloadPriority {
    Interactive,
    Foreground,
    Background,
}

impl WorkloadPriority {
    const fn rank(self) -> u8 {
        match self {
            Self::Interactive => 3,
            Self::Foreground => 2,
            Self::Background => 1,
        }
    }
}

/// Coarse execution resource. Backend-specific providers remain owned by the
/// runtime resolver; the scheduler only controls scarce concurrency slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RuntimeResource {
    Cpu,
    Accelerator,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeJobRequest {
    pub job_id: String,
    pub priority: WorkloadPriority,
    pub preferred_resource: RuntimeResource,
    pub allow_cpu_fallback: bool,
    pub estimated_memory_mb: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSchedulingLimits {
    pub max_active_cpu: usize,
    pub max_active_accelerator: usize,
    pub max_queued: usize,
    pub memory_budget_mb: u64,
}

impl Default for RuntimeSchedulingLimits {
    fn default() -> Self {
        Self {
            max_active_cpu: 1,
            max_active_accelerator: 1,
            max_queued: 32,
            memory_budget_mb: 4_096,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum RuntimeAdmissionCode {
    Started,
    Queued,
    InvalidRequest,
    DuplicateJob,
    QueueFull,
    MemoryBudgetExceeded,
    ResourceUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeAdmissionDecision {
    pub job_id: String,
    pub code: RuntimeAdmissionCode,
    pub resource: Option<RuntimeResource>,
    pub queue_position: Option<usize>,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledRuntimeJob {
    pub job_id: String,
    pub priority: WorkloadPriority,
    pub resource: RuntimeResource,
    pub estimated_memory_mb: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSchedulerSnapshot {
    pub active: Vec<ScheduledRuntimeJob>,
    pub queued: Vec<String>,
    pub active_memory_mb: u64,
    pub limits: RuntimeSchedulingLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeReleaseOutcome {
    pub released: bool,
    pub promoted: Vec<ScheduledRuntimeJob>,
}

#[derive(Debug, Clone)]
struct QueuedRuntimeJob {
    request: RuntimeJobRequest,
    sequence: u64,
}

/// Bounded, transport-neutral admission controller for recognition runtimes.
///
/// It owns no threads and executes no model code. Callers start jobs only after
/// `Started` (or promotion from the queue) and must release them on every exit
/// path. This keeps desktop, service, and mobile adapters on one policy.
#[derive(Debug, Clone)]
pub struct RuntimeAdmissionScheduler {
    limits: RuntimeSchedulingLimits,
    active: BTreeMap<String, ScheduledRuntimeJob>,
    queued: VecDeque<QueuedRuntimeJob>,
    next_sequence: u64,
}

impl RuntimeAdmissionScheduler {
    pub fn new(limits: RuntimeSchedulingLimits) -> Self {
        Self {
            limits,
            active: BTreeMap::new(),
            queued: VecDeque::new(),
            next_sequence: 0,
        }
    }

    pub fn submit(&mut self, request: RuntimeJobRequest) -> RuntimeAdmissionDecision {
        let job_id = request.job_id.trim().to_owned();
        if job_id.is_empty() || request.estimated_memory_mb == 0 {
            return rejected(job_id, RuntimeAdmissionCode::InvalidRequest, false);
        }
        if self.active.contains_key(&job_id)
            || self
                .queued
                .iter()
                .any(|queued| queued.request.job_id == job_id)
        {
            return rejected(job_id, RuntimeAdmissionCode::DuplicateJob, false);
        }
        if request.estimated_memory_mb > self.limits.memory_budget_mb {
            return rejected(job_id, RuntimeAdmissionCode::MemoryBudgetExceeded, false);
        }

        let mut request = request;
        request.job_id = job_id.clone();
        if let Some(resource) = self.runnable_resource(&request) {
            let scheduled = scheduled(&request, resource);
            self.active.insert(job_id.clone(), scheduled);
            return RuntimeAdmissionDecision {
                job_id,
                code: RuntimeAdmissionCode::Started,
                resource: Some(resource),
                queue_position: None,
                retryable: false,
            };
        }

        if !self.has_any_compatible_resource(&request) {
            return rejected(job_id, RuntimeAdmissionCode::ResourceUnavailable, false);
        }
        if self.queued.len() >= self.limits.max_queued {
            return rejected(job_id, RuntimeAdmissionCode::QueueFull, true);
        }

        let queued = QueuedRuntimeJob {
            request,
            sequence: self.next_sequence,
        };
        self.next_sequence = self.next_sequence.saturating_add(1);
        let insert_at = self
            .queued
            .iter()
            .position(|existing| queue_precedes(&queued, existing))
            .unwrap_or(self.queued.len());
        self.queued.insert(insert_at, queued);
        RuntimeAdmissionDecision {
            job_id,
            code: RuntimeAdmissionCode::Queued,
            resource: None,
            queue_position: Some(insert_at + 1),
            retryable: true,
        }
    }

    pub fn complete(&mut self, job_id: &str) -> RuntimeReleaseOutcome {
        let released = self.active.remove(job_id).is_some();
        let promoted = if released {
            self.promote_ready()
        } else {
            Vec::new()
        };
        RuntimeReleaseOutcome { released, promoted }
    }

    pub fn cancel(&mut self, job_id: &str) -> RuntimeReleaseOutcome {
        if let Some(index) = self
            .queued
            .iter()
            .position(|queued| queued.request.job_id == job_id)
        {
            self.queued.remove(index);
            return RuntimeReleaseOutcome {
                released: true,
                promoted: Vec::new(),
            };
        }
        self.complete(job_id)
    }

    pub fn snapshot(&self) -> RuntimeSchedulerSnapshot {
        RuntimeSchedulerSnapshot {
            active: self.active.values().cloned().collect(),
            queued: self
                .queued
                .iter()
                .map(|queued| queued.request.job_id.clone())
                .collect(),
            active_memory_mb: self.active_memory_mb(),
            limits: self.limits,
        }
    }

    fn promote_ready(&mut self) -> Vec<ScheduledRuntimeJob> {
        let mut promoted = Vec::new();
        while let Some((index, resource)) =
            self.queued.iter().enumerate().find_map(|(index, queued)| {
                self.runnable_resource(&queued.request)
                    .map(|resource| (index, resource))
            })
        {
            let queued = self.queued.remove(index).expect("queued index is valid");
            let job = scheduled(&queued.request, resource);
            self.active.insert(job.job_id.clone(), job.clone());
            promoted.push(job);
        }
        promoted
    }

    fn runnable_resource(&self, request: &RuntimeJobRequest) -> Option<RuntimeResource> {
        if self
            .active_memory_mb()
            .saturating_add(request.estimated_memory_mb)
            > self.limits.memory_budget_mb
        {
            return None;
        }
        if self.resource_has_capacity(request.preferred_resource) {
            return Some(request.preferred_resource);
        }
        if request.preferred_resource == RuntimeResource::Accelerator
            && request.allow_cpu_fallback
            && self.resource_has_capacity(RuntimeResource::Cpu)
        {
            return Some(RuntimeResource::Cpu);
        }
        None
    }

    fn has_any_compatible_resource(&self, request: &RuntimeJobRequest) -> bool {
        match request.preferred_resource {
            RuntimeResource::Cpu => self.limits.max_active_cpu > 0,
            RuntimeResource::Accelerator => {
                self.limits.max_active_accelerator > 0
                    || (request.allow_cpu_fallback && self.limits.max_active_cpu > 0)
            }
        }
    }

    fn resource_has_capacity(&self, resource: RuntimeResource) -> bool {
        let active = self
            .active
            .values()
            .filter(|job| job.resource == resource)
            .count();
        match resource {
            RuntimeResource::Cpu => active < self.limits.max_active_cpu,
            RuntimeResource::Accelerator => active < self.limits.max_active_accelerator,
        }
    }

    fn active_memory_mb(&self) -> u64 {
        self.active
            .values()
            .map(|job| job.estimated_memory_mb)
            .sum()
    }
}

fn scheduled(request: &RuntimeJobRequest, resource: RuntimeResource) -> ScheduledRuntimeJob {
    ScheduledRuntimeJob {
        job_id: request.job_id.clone(),
        priority: request.priority,
        resource,
        estimated_memory_mb: request.estimated_memory_mb,
    }
}

fn rejected(
    job_id: String,
    code: RuntimeAdmissionCode,
    retryable: bool,
) -> RuntimeAdmissionDecision {
    RuntimeAdmissionDecision {
        job_id,
        code,
        resource: None,
        queue_position: None,
        retryable,
    }
}

fn queue_precedes(candidate: &QueuedRuntimeJob, existing: &QueuedRuntimeJob) -> bool {
    candidate.request.priority.rank() > existing.request.priority.rank()
        || (candidate.request.priority.rank() == existing.request.priority.rank()
            && candidate.sequence < existing.sequence)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: &str, priority: WorkloadPriority, memory: u64) -> RuntimeJobRequest {
        RuntimeJobRequest {
            job_id: id.to_owned(),
            priority,
            preferred_resource: RuntimeResource::Cpu,
            allow_cpu_fallback: false,
            estimated_memory_mb: memory,
        }
    }

    #[test]
    fn interactive_work_overtakes_background_without_reordering_peers() {
        let mut scheduler = RuntimeAdmissionScheduler::new(RuntimeSchedulingLimits {
            max_active_cpu: 1,
            max_active_accelerator: 0,
            max_queued: 4,
            memory_budget_mb: 1_024,
        });
        assert_eq!(
            scheduler
                .submit(job("running", WorkloadPriority::Foreground, 128))
                .code,
            RuntimeAdmissionCode::Started
        );
        scheduler.submit(job("background-a", WorkloadPriority::Background, 128));
        scheduler.submit(job("interactive", WorkloadPriority::Interactive, 128));
        scheduler.submit(job("background-b", WorkloadPriority::Background, 128));

        assert_eq!(
            scheduler.snapshot().queued,
            vec!["interactive", "background-a", "background-b"]
        );
        let outcome = scheduler.complete("running");
        assert_eq!(outcome.promoted[0].job_id, "interactive");
    }

    #[test]
    fn memory_and_queue_limits_fail_closed() {
        let mut scheduler = RuntimeAdmissionScheduler::new(RuntimeSchedulingLimits {
            max_active_cpu: 1,
            max_active_accelerator: 0,
            max_queued: 1,
            memory_budget_mb: 256,
        });
        assert_eq!(
            scheduler
                .submit(job("too-large", WorkloadPriority::Foreground, 257))
                .code,
            RuntimeAdmissionCode::MemoryBudgetExceeded
        );
        scheduler.submit(job("running", WorkloadPriority::Foreground, 128));
        assert_eq!(
            scheduler
                .submit(job("queued", WorkloadPriority::Foreground, 128))
                .code,
            RuntimeAdmissionCode::Queued
        );
        assert_eq!(
            scheduler
                .submit(job("overflow", WorkloadPriority::Foreground, 64))
                .code,
            RuntimeAdmissionCode::QueueFull
        );
    }

    #[test]
    fn accelerator_jobs_use_only_explicit_cpu_fallback() {
        let limits = RuntimeSchedulingLimits {
            max_active_cpu: 1,
            max_active_accelerator: 0,
            max_queued: 2,
            memory_budget_mb: 512,
        };
        let mut scheduler = RuntimeAdmissionScheduler::new(limits);
        let unavailable = RuntimeJobRequest {
            job_id: "gpu-only".to_owned(),
            priority: WorkloadPriority::Interactive,
            preferred_resource: RuntimeResource::Accelerator,
            allow_cpu_fallback: false,
            estimated_memory_mb: 64,
        };
        assert_eq!(
            scheduler.submit(unavailable).code,
            RuntimeAdmissionCode::ResourceUnavailable
        );
        let fallback = RuntimeJobRequest {
            job_id: "fallback".to_owned(),
            allow_cpu_fallback: true,
            ..job("fallback", WorkloadPriority::Interactive, 64)
        };
        let fallback = RuntimeJobRequest {
            preferred_resource: RuntimeResource::Accelerator,
            ..fallback
        };
        let decision = scheduler.submit(fallback);
        assert_eq!(decision.code, RuntimeAdmissionCode::Started);
        assert_eq!(decision.resource, Some(RuntimeResource::Cpu));
    }

    #[test]
    fn cancellation_and_completion_release_capacity() {
        let mut scheduler = RuntimeAdmissionScheduler::new(RuntimeSchedulingLimits {
            max_active_cpu: 1,
            max_active_accelerator: 0,
            max_queued: 2,
            memory_budget_mb: 256,
        });
        scheduler.submit(job("active", WorkloadPriority::Foreground, 128));
        scheduler.submit(job("cancel-me", WorkloadPriority::Background, 128));
        assert!(scheduler.cancel("cancel-me").released);
        assert!(scheduler.snapshot().queued.is_empty());
        scheduler.submit(job("next", WorkloadPriority::Foreground, 128));
        let outcome = scheduler.complete("active");
        assert!(outcome.released);
        assert_eq!(outcome.promoted[0].job_id, "next");
        assert_eq!(scheduler.snapshot().active_memory_mb, 128);
    }

    #[test]
    fn duplicate_ids_and_invalid_requests_are_rejected() {
        let mut scheduler = RuntimeAdmissionScheduler::new(RuntimeSchedulingLimits::default());
        assert_eq!(
            scheduler
                .submit(job("", WorkloadPriority::Foreground, 1))
                .code,
            RuntimeAdmissionCode::InvalidRequest
        );
        scheduler.submit(job("same", WorkloadPriority::Foreground, 1));
        assert_eq!(
            scheduler
                .submit(job("same", WorkloadPriority::Interactive, 1))
                .code,
            RuntimeAdmissionCode::DuplicateJob
        );
    }
}
