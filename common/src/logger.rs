//! 日志记录模块

use std::io;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Registry};

/// 初始化统一的 tracing 订阅器,
/// 返回 `WorkerGuard` 确保日志写入器生命周期正确
pub fn init_tracing() -> (WorkerGuard, WorkerGuard) {
    // 默认日志级别为 "info"，可通过 RUST_LOG 覆盖
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    // 输出到控制台（非阻塞）
    let (console_writer, console_guard) = tracing_appender::non_blocking(io::stdout());
    let (file_writer, file_guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::daily("logs", "app.log"));

    let console_layer = fmt::layer()
        .with_timer(fmt::time::ChronoLocal::rfc_3339())
        .with_target(false)
        .with_writer(console_writer)
        .with_ansi(atty::is(atty::Stream::Stdout)) // 自动检测TTY启用颜色
        .with_file(true) // 显示文件名
        .with_line_number(true) // 显示行号
        .with_thread_names(false)
        .compact();

    let file_layer = fmt::layer()
        .with_timer(fmt::time::ChronoLocal::rfc_3339())
        .with_target(false)
        .with_writer(file_writer)
        .with_ansi(false)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(false)
        .compact();

    // 组合所有 Layers
    let subscriber = Registry::default()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer);

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global tracing subscriber");

    // 返回 guard 避免日志写入器被提前释放
    (console_guard, file_guard)
}
