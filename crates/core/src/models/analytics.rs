/// Per-command usage statistics, aggregated from `command_logs`.
#[derive(Debug, Clone)]
pub struct CommandStats {
    pub name: String,
    pub total_use: i64,
    pub weekly_use: i64,
    pub monthly_use: i64,
    pub average_execution_time: i64,
}

/// High-level user activity counters.
#[derive(Debug, Clone)]
pub struct UsageStats {
    pub total_users: u64,
    pub daily_active_users: u64,
    pub monthly_active_users: u64,
}
