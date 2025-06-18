use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use std::time::Instant;
use volo_http::http::{StatusCode, Uri};
use volo_http::request::ServerRequest;
use volo_http::response::ServerResponse;
use volo_http::server::IntoResponse;
use volo_http::{context::ServerContext, server::middleware::Next};

pub fn setup_metrics_recorder() -> PrometheusHandle {
    const EXPONENTIAL_SECONDS: &[f64] = &[
        0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
    ];

    PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("http_requests_duration_seconds".to_string()),
            EXPONENTIAL_SECONDS,
        )
        .unwrap()
        .install_recorder()
        .unwrap()
}
pub async fn track_metrics(
    uri: Uri,
    cx: &mut ServerContext,
    req: ServerRequest,
    next: Next,
) -> Result<ServerResponse, StatusCode> {
    let start = Instant::now();

    let path = uri.path();
    let method = req.method().clone();

    let response = next.run(cx, req).await;

    let latency = start.elapsed().as_secs_f64();

    let Ok(r) = response else {
        return Ok(response.into_response());
    };

    let labels = [
        ("method", method.to_string()),
        ("path", path.to_string()),
        ("status", r.status().as_u16().to_string()),
    ];

    metrics::counter!("http_requests_total", &labels).increment(1);
    metrics::histogram!("http_requests_duration_seconds", &labels).record(latency);

    Ok(r)
}
