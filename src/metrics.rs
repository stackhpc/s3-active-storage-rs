//! Prometheus metrics

use std::time::Instant;

use axum::{http::Request, middleware::Next, response::IntoResponse};
use lazy_static::lazy_static;
use prometheus::{self, Encoder, HistogramOpts, HistogramVec, IntCounterVec, Opts};

lazy_static! {
    // Simple request counter
    pub static ref INCOMING_REQUESTS: IntCounterVec = IntCounterVec::new(
        Opts::new("incoming_requests", "The number of HTTP requests received"),
        &["http_method", "path"]
    ).expect("Prometheus metric options should be valid");
    // Request counter by status code
    pub static ref RESPONSE_CODE_COLLECTOR: IntCounterVec = IntCounterVec::new(
        Opts::new("outgoing_response", "The number of responses sent"),
        &["status_code", "http_method", "path"]
    ).expect("Prometheus metric options should be valid");
    // Response histogram by response time
    pub static ref RESPONSE_TIME_COLLECTOR: HistogramVec = HistogramVec::new(
        HistogramOpts{
            common_opts: Opts::new("response_time", "The time taken to respond to each request"),
            buckets: prometheus::DEFAULT_BUCKETS.to_vec(), // Change buckets here if desired
        },
        &["status_code", "http_method", "path"],
    ).expect("Prometheus metric options should be valid");
    // Disk cache hit counter
    pub static ref LOCAL_CACHE_MISSES: IntCounterVec = IntCounterVec::new(
        Opts::new("cache_miss", "The number of times the requested chunk was not available in then local chunk cache"),
        &["cache"]
    ).expect("Prometheus metric options should be valid");
}

/// Registers various prometheus metrics with the global registry
pub fn register_metrics() {
    let registry = prometheus::default_registry();
    registry
        .register(Box::new(INCOMING_REQUESTS.clone()))
        .expect("Prometheus metrics registration should not fail during initialization");
    registry
        .register(Box::new(RESPONSE_CODE_COLLECTOR.clone()))
        .expect("Prometheus metrics registration should not fail during initialization");
    registry
        .register(Box::new(RESPONSE_TIME_COLLECTOR.clone()))
        .expect("Prometheus metrics registration should not fail during initialization");
    registry
        .register(Box::new(LOCAL_CACHE_MISSES.clone()))
        .expect("Prometheus metrics registration should not fail during initialization");
}

/// Returns currently gathered prometheus metrics
pub async fn metrics_handler() -> String {
    let encoder = prometheus::TextEncoder::new();
    let mut buffer = Vec::new();

    encoder
        .encode(&prometheus::gather(), &mut buffer)
        .expect("could not encode gathered metrics into temporary buffer");

    String::from_utf8(buffer).expect("could not convert metrics buffer into string")
}

pub async fn track_metrics<B>(request: Request<B>, next: Next<B>) -> impl IntoResponse {
    // Extract some useful quantities
    let timer = Instant::now();
    let http_method = &request.method().to_string().to_ascii_uppercase();
    let request_path = request.uri().path().to_string();

    // Increment request counter
    INCOMING_REQUESTS
        .with_label_values(&[http_method, &request_path])
        .inc();

    // Pass request onto next layer
    let response = next.run(request).await;
    let status_code = response.status();
    // Due to 'concentric shell model' for axum layers,
    // latency is time taken to traverse all inner
    // layers (including primary reduction operation)
    // and then back up the layer stack.
    let latency = timer.elapsed().as_secs_f64();

    // Record response metrics
    RESPONSE_CODE_COLLECTOR
        .with_label_values(&[status_code.as_str(), http_method, &request_path])
        .inc();
    RESPONSE_TIME_COLLECTOR
        .with_label_values(&[status_code.as_str(), http_method, &request_path])
        .observe(latency);

    response
}
