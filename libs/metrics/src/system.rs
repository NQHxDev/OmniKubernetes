//! System and Tokio runtime telemetry collection (zero-`unsafe`).
//!
//! Gathers Tokio reactor saturation, task counts, scheduler lag, and Linux `/proc/self` process metrics
//! with safe fallback on non-Linux platforms.

#![allow(clippy::cast_precision_loss)]

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use metrics::gauge;
use metrics_exporter_prometheus::PrometheusHandle;

use crate::{config::MetricsConfig, probes::ProbeState};

/// Metric names for Tokio runtime and process telemetry.
pub const METRIC_TOKIO_RUNTIME_WORKERS_COUNT: &str = "tokio_runtime_workers_count";
pub const METRIC_TOKIO_RUNTIME_ALIVE_TASKS: &str = "tokio_runtime_alive_tasks";
pub const METRIC_TOKIO_RUNTIME_GLOBAL_QUEUE_DEPTH: &str = "tokio_runtime_global_queue_depth";
pub const METRIC_TOKIO_RUNTIME_WORKER_BUSY_DURATION_SECONDS: &str =
    "tokio_runtime_worker_busy_duration_seconds";
pub const METRIC_TOKIO_RUNTIME_WORKER_PARK_COUNT_TOTAL: &str =
    "tokio_runtime_worker_park_count_total";
pub const METRIC_TOKIO_WATCHDOG_CHECK_LAG_SECONDS: &str = "tokio_watchdog_check_lag_seconds";

pub const METRIC_PROCESS_RESIDENT_MEMORY_BYTES: &str = "process_resident_memory_bytes";
pub const METRIC_PROCESS_VIRTUAL_MEMORY_BYTES: &str = "process_virtual_memory_bytes";
pub const METRIC_PROCESS_CPU_SECONDS_TOTAL: &str = "process_cpu_seconds_total";
pub const METRIC_PROCESS_OPEN_FDS: &str = "process_open_fds";
pub const METRIC_PROCESS_MAX_FDS: &str = "process_max_fds";
pub const METRIC_PROCESS_THREADS: &str = "process_threads";
pub const METRIC_PROCESS_UPTIME_SECONDS: &str = "process_uptime_seconds";
pub const METRIC_OMNI_BUILD_INFO: &str = "omni_build_info";

/// Khởi chạy tác vụ nền thu thập chỉ số hệ thống, độ bão hòa Tokio runtime và nhịp tim Watchdog.
pub fn spawn_system_metrics_collector(
    probe_state: Arc<ProbeState>,
    prometheus_handle: PrometheusHandle,
    config: MetricsConfig,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let heartbeat_interval = Duration::from_secs(1);
        let upkeep_interval = config.upkeep_interval;
        let started_at = Instant::now();
        let mut last_upkeep = Instant::now();

        loop {
            let sleep_start = Instant::now();
            tokio::time::sleep(heartbeat_interval).await;
            let actual_elapsed = sleep_start.elapsed();

            // 1. Cập nhật nhịp tim Watchdog & đo độ trễ lập lịch của Tokio Reactor
            probe_state.record_watchdog_heartbeat();
            let lag_secs = actual_elapsed
                .saturating_sub(heartbeat_interval)
                .as_secs_f64();
            gauge!(METRIC_TOKIO_WATCHDOG_CHECK_LAG_SECONDS).set(lag_secs);

            // 2. Chạy upkeep và cập nhật chỉ số chi tiết định kỳ theo `upkeep_interval`
            if last_upkeep.elapsed() >= upkeep_interval {
                // Xả AtomicBucket của Histograms để giải phóng bộ đệm định kỳ
                prometheus_handle.run_upkeep();

                // Cập nhật Tokio Runtime Saturation metrics
                collect_tokio_metrics();

                // Cập nhật tài nguyên tiến trình OS
                collect_process_metrics(started_at);

                // Cập nhật lại build info
                gauge!(
                    METRIC_OMNI_BUILD_INFO,
                    "version" => config.service_version.clone(),
                    "rust_version" => option_env!("CARGO_PKG_RUST_VERSION").unwrap_or("1.85+")
                )
                .set(1.0);

                last_upkeep = Instant::now();
            }
        }
    })
}

/// Thu thập chỉ số Tokio Runtime Metrics không dùng cờ unstable.
pub fn collect_tokio_metrics() {
    let handle = tokio::runtime::Handle::current();
    let metrics = handle.metrics();

    let workers = metrics.num_workers();
    gauge!(METRIC_TOKIO_RUNTIME_WORKERS_COUNT).set(workers as f64);
    gauge!(METRIC_TOKIO_RUNTIME_ALIVE_TASKS).set(metrics.num_alive_tasks() as f64);
    gauge!(METRIC_TOKIO_RUNTIME_GLOBAL_QUEUE_DEPTH).set(metrics.global_queue_depth() as f64);

    for w in 0..workers {
        let worker_str = w.to_string();
        let busy_secs = metrics.worker_total_busy_duration(w).as_secs_f64();
        let park_count = metrics.worker_park_count(w);

        gauge!(
            METRIC_TOKIO_RUNTIME_WORKER_BUSY_DURATION_SECONDS,
            "worker" => worker_str.clone()
        )
        .set(busy_secs);

        gauge!(
            METRIC_TOKIO_RUNTIME_WORKER_PARK_COUNT_TOTAL,
            "worker" => worker_str
        )
        .set(park_count as f64);
    }
}

/// Thu thập tài nguyên tiến trình OS (đọc từ Linux `/proc/self` hoặc an toàn fallback).
pub fn collect_process_metrics(started_at: Instant) {
    gauge!(METRIC_PROCESS_UPTIME_SECONDS).set(started_at.elapsed().as_secs_f64());

    // Đọc /proc/self/status nếu có (Linux Container)
    if let Ok(status_str) = std::fs::read_to_string("/proc/self/status") {
        let (vm_rss, vm_size, threads) = parse_proc_status(&status_str);
        if let Some(rss) = vm_rss {
            gauge!(METRIC_PROCESS_RESIDENT_MEMORY_BYTES).set(rss as f64);
        }
        if let Some(size) = vm_size {
            gauge!(METRIC_PROCESS_VIRTUAL_MEMORY_BYTES).set(size as f64);
        }
        if let Some(th) = threads {
            gauge!(METRIC_PROCESS_THREADS).set(th as f64);
        }
    }

    // Đọc /proc/self/stat nếu có (CPU seconds)
    if let Ok(stat_str) = std::fs::read_to_string("/proc/self/stat")
        && let Some(cpu_secs) = parse_proc_stat(&stat_str)
    {
        gauge!(METRIC_PROCESS_CPU_SECONDS_TOTAL).set(cpu_secs);
    }

    // Đếm số lượng File Descriptors đang mở
    if let Some(open_fds) = read_open_fds() {
        gauge!(METRIC_PROCESS_OPEN_FDS).set(open_fds as f64);
    }

    // Đọc giới hạn tối đa File Descriptors từ /proc/self/limits
    if let Ok(limits_str) = std::fs::read_to_string("/proc/self/limits")
        && let Some(max_fds) = parse_proc_limits(&limits_str)
    {
        gauge!(METRIC_PROCESS_MAX_FDS).set(max_fds as f64);
    }
}

/// Phân tích nội dung `/proc/self/status` lấy `(VmRSS_bytes, VmSize_bytes, Threads)`.
#[must_use]
pub fn parse_proc_status(content: &str) -> (Option<u64>, Option<u64>, Option<u64>) {
    let mut vm_rss = None;
    let mut vm_size = None;
    let mut threads = None;

    for line in content.lines() {
        if let Some(val_str) = line.strip_prefix("VmRSS:") {
            if let Some(kb) = parse_kb_value(val_str) {
                vm_rss = Some(kb.saturating_mul(1024));
            }
        } else if let Some(val_str) = line.strip_prefix("VmSize:") {
            if let Some(kb) = parse_kb_value(val_str) {
                vm_size = Some(kb.saturating_mul(1024));
            }
        } else if let Some(val_str) = line.strip_prefix("Threads:")
            && let Ok(num) = val_str.trim().parse::<u64>()
        {
            threads = Some(num);
        }
    }

    (vm_rss, vm_size, threads)
}

#[inline]
fn parse_kb_value(s: &str) -> Option<u64> {
    s.split_whitespace()
        .next()
        .and_then(|t| t.parse::<u64>().ok())
}

/// Phân tích nội dung `/proc/self/stat` lấy tổng thời gian CPU (User + System mode tính bằng giây).
#[must_use]
pub fn parse_proc_stat(content: &str) -> Option<f64> {
    let close_paren_idx = content.rfind(')')?;
    let rest = content.get(close_paren_idx + 1..)?.trim();
    let tokens: Vec<&str> = rest.split_whitespace().collect();

    // Trong rest:
    // Field 3 (state) là index 0
    // Field 14 (utime) là index 11
    // Field 15 (stime) là index 12
    let utime: u64 = tokens.get(11)?.parse().ok()?;
    let stime: u64 = tokens.get(12)?.parse().ok()?;

    // Chuẩn USER_HZ thông thường trên Linux = 100
    Some((utime.saturating_add(stime)) as f64 / 100.0)
}

/// Phân tích nội dung `/proc/self/limits` lấy Soft Limit của `Max open files`.
#[must_use]
pub fn parse_proc_limits(content: &str) -> Option<u64> {
    for line in content.lines() {
        if line.starts_with("Max open files") {
            let rest = line.get("Max open files".len()..)?.trim();
            let soft_limit_str = rest.split_whitespace().next()?;
            return soft_limit_str.parse::<u64>().ok();
        }
    }
    None
}

/// Đếm số File Descriptors đang mở trong `/proc/self/fd`.
#[must_use]
pub fn read_open_fds() -> Option<u64> {
    std::fs::read_dir("/proc/self/fd")
        .ok()
        .map(|entries| entries.count() as u64)
}
