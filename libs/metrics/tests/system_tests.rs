//! Unit and integration tests for Tokio runtime and system telemetry (Linux /proc/self reader & fallback).

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use omni_metrics::{
    MetricsConfig, ProbeState,
    system::{
        collect_process_metrics, collect_tokio_metrics, parse_proc_limits, parse_proc_stat,
        parse_proc_status, spawn_system_metrics_collector,
    },
};

#[test]
fn test_parse_proc_status_parsing() {
    let sample_status = r"
Name:   order_service
Umask:  0022
State:  S (sleeping)
Tgid:   1234
VmPeak:    55432 kB
VmSize:    52100 kB
VmLck:         0 kB
VmPin:         0 kB
VmHWM:     18450 kB
VmRSS:     16200 kB
RssAnon:   12000 kB
RssFile:    4200 kB
Threads: 8
";

    let (vm_rss, vm_size, threads) = parse_proc_status(sample_status);
    assert_eq!(vm_rss, Some(16_200 * 1024));
    assert_eq!(vm_size, Some(52_100 * 1024));
    assert_eq!(threads, Some(8));
}

#[test]
fn test_parse_proc_stat_with_special_process_names() {
    // 1. Tên tiến trình thông thường
    let sample_stat1 = "1234 (order_service) S 1 1234 1234 0 -1 4194304 312 0 0 0 150 50 0 0 20 0 8 0 1000 53350400 4050 18446744073709551615 1 1 0 0 0 0 0 0 0 0 0 0 17 2 0 0 0 0 0";
    let cpu1 = parse_proc_stat(sample_stat1).unwrap();
    // utime = 150, stime = 50 -> 200 ticks / 100 = 2.0s
    assert!((cpu1 - 2.0).abs() < f64::EPSILON);

    // 2. Tên tiến trình có dấu ngoặc đơn và khoảng trắng lồng nhau
    let sample_stat2 =
        "5678 (tokio: worker (1)) S 1 5678 5678 0 -1 4194304 312 0 0 0 350 150 0 0 20 0 8 0 1000";
    let cpu2 = parse_proc_stat(sample_stat2).unwrap();
    // utime = 350, stime = 150 -> 500 ticks / 100 = 5.0s
    assert!((cpu2 - 5.0).abs() < f64::EPSILON);
}

#[test]
fn test_parse_proc_limits_parsing() {
    let sample_limits = r"
Limit                     Soft Limit           Hard Limit           Units     
Max cpu time              unlimited            unlimited            seconds   
Max file size             unlimited            unlimited            bytes     
Max open files            65536                1048576              files     
Max locked memory         65536                65536                bytes     
";

    let max_fds = parse_proc_limits(sample_limits);
    assert_eq!(max_fds, Some(65536));
}

#[tokio::test]
async fn test_runtime_metrics_collection_without_panic() {
    let (recorder, _handle) =
        omni_metrics::create_isolated_recorder(&MetricsConfig::default()).unwrap();

    metrics::with_local_recorder(&recorder, || {
        // Thu thập Tokio runtime metrics
        collect_tokio_metrics();

        // Thu thập Process metrics (an toàn trên cả Linux lẫn macOS)
        collect_process_metrics(Instant::now());
    });
}

#[tokio::test]
async fn test_spawn_system_metrics_collector_and_watchdog_loop() {
    let config = MetricsConfig {
        upkeep_interval: Duration::from_millis(50),
        liveness_stall_threshold: Duration::from_secs(5),
        ..Default::default()
    };
    let (_recorder, handle) = omni_metrics::create_isolated_recorder(&config).unwrap();
    let state = Arc::new(ProbeState::new(true, Duration::from_secs(5)));

    let task_handle = spawn_system_metrics_collector(state.clone(), handle, config);

    // Chờ 1.1s để tác vụ nền chạy ít nhất 1 chu kỳ heartbeat
    tokio::time::sleep(Duration::from_millis(1100)).await;

    // Đảm bảo Watchdog liên tục khỏe mạnh nhờ tác vụ nền cập nhật heartbeat
    assert!(state.is_watchdog_healthy());

    task_handle.abort();
}
