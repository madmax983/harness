//! Contribution weight calculation for LoRA delta aggregation.

/// A computed weight for an agent's contribution during aggregation.
#[derive(Debug, Clone, Copy)]
pub struct ContributionWeight {
    /// The computed weight value (higher = more influence).
    pub value: f64,
}

impl ContributionWeight {
    /// Compute a contribution weight from agent performance metrics.
    ///
    /// Weight = log2(1 + task_count) * success_rate * sqrt(recency_factor)
    ///
    /// Square root dampens the recency penalty so experienced agents retain
    /// value even when slightly stale. This prevents recency from completely
    /// overwhelming experience in federated averaging.
    ///
    /// - `task_count`: how many tasks contributed to this delta
    /// - `success_rate`: fraction of tasks completed successfully (0.0 - 1.0)
    /// - `recency_factor`: how recent (1.0 = just now, 0.0 = very stale)
    pub fn compute(task_count: u32, success_rate: f64, recency_factor: f64) -> Self {
        let experience = (1.0 + task_count as f64).ln() / std::f64::consts::LN_2;
        // Apply sqrt to recency to soften the staleness penalty
        let value = experience * success_rate * recency_factor.sqrt();
        Self { value }
    }
}
