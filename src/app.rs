//! Active Storage server API

use crate::chunk_cache::{self, ChunkCache};
use crate::cli::CommandLineArgs;
use crate::error::ActiveStorageError;
use crate::filter_pipeline;
use crate::metrics::{metrics_handler, track_metrics, LOCAL_CACHE_MISSES};
use crate::models;
use crate::operation;
use crate::operations;
use crate::resource_manager::ResourceManager;
use crate::s3_client;
use crate::types::{ByteOrder, NATIVE_BYTE_ORDER};
use crate::validated_json::ValidatedJson;

use axum::middleware;
use axum::{
    extract::{Path, State},
    headers::authorization::{Authorization, Basic},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router, TypedHeader,
};
use bytes::Bytes;
use cached::IOCached;

use std::sync::Arc;
use tower::Layer;
use tower::ServiceBuilder;
use tower_http::normalize_path::NormalizePathLayer;
use tower_http::trace::TraceLayer;
use tracing::debug_span;
use tracing::Instrument;

/// `x-activestorage-dtype` header definition
static HEADER_DTYPE: header::HeaderName = header::HeaderName::from_static("x-activestorage-dtype");
/// `x-activestorage-shape` header definition
static HEADER_SHAPE: header::HeaderName = header::HeaderName::from_static("x-activestorage-shape");
/// `x-activestorage-count` header definition
static HEADER_COUNT: header::HeaderName = header::HeaderName::from_static("x-activestorage-count");
/// `x-activestorage-byte-order` header definition
static HEADER_BYTE_ORDER: header::HeaderName =
    header::HeaderName::from_static("x-activestorage-byte-order");
const HEADER_BYTE_ORDER_VALUE: &str = match NATIVE_BYTE_ORDER {
    ByteOrder::Big => "big",
    ByteOrder::Little => "little",
};

/// Shared application state passed to each operation request handler.
struct AppState {
    /// Command line arguments.
    args: CommandLineArgs,

    /// Map of S3 client objects.
    s3_client_map: s3_client::S3ClientMap,

    /// Resource manager.
    resource_manager: ResourceManager,

    /// Object chunk cache
    chunk_cache: ChunkCache,
}

impl AppState {
    /// Create and return an [AppState].
    fn new(args: &CommandLineArgs) -> Self {
        let task_limit = args.thread_limit.or_else(|| Some(num_cpus::get() - 1));
        let resource_manager =
            ResourceManager::new(args.s3_connection_limit, args.memory_limit, task_limit);
        let chunk_cache: ChunkCache = chunk_cache::build(args);

        Self {
            args: args.clone(),
            s3_client_map: s3_client::S3ClientMap::new(),
            resource_manager,
            chunk_cache,
        }
    }
}

/// AppState wrapped in an Atomic Reference Count (Arc) to allow multiple references.
type SharedAppState = Arc<AppState>;

impl IntoResponse for models::Response {
    /// Convert a [crate::models::Response] into a [axum::response::Response].
    fn into_response(self) -> Response {
        (
            [
                (
                    &header::CONTENT_TYPE,
                    mime::APPLICATION_OCTET_STREAM.to_string(),
                ),
                (&HEADER_DTYPE, self.dtype.to_string().to_lowercase()),
                (&HEADER_SHAPE, serde_json::to_string(&self.shape).unwrap()),
                (&HEADER_COUNT, serde_json::to_string(&self.count).unwrap()),
                (&HEADER_BYTE_ORDER, HEADER_BYTE_ORDER_VALUE.to_string()),
            ],
            self.body,
        )
            .into_response()
    }
}

/// Initialise the application
pub fn init(args: &CommandLineArgs) {
    if args.use_rayon {
        rayon::ThreadPoolBuilder::new()
            .num_threads(num_cpus::get() - 1)
            .build_global()
            .expect("Failed to build Rayon thread pool");
    };
}

/// Returns a [axum::Router] for the Active Storage server API
///
/// The router is populated with all routes as well as the following middleware:
///
/// * a [tower_http::trace::TraceLayer] for tracing requests and responses
fn router(args: &CommandLineArgs) -> Router {
    fn v1(state: SharedAppState) -> Router {
        Router::new()
            .route("/count", post(operation_handler::<operations::Count>))
            .route("/max", post(operation_handler::<operations::Max>))
            .route("/min", post(operation_handler::<operations::Min>))
            .route("/select", post(operation_handler::<operations::Select>))
            .route("/sum", post(operation_handler::<operations::Sum>))
            .route("/:operation", post(unknown_operation_handler))
            .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()))
            .with_state(state)
    }

    let state = SharedAppState::new(AppState::new(args));
    Router::new()
        .route("/.well-known/reductionist-schema", get(schema))
        .route("/metrics", get(metrics_handler))
        .nest("/v1", v1(state))
        .route_layer(middleware::from_fn(track_metrics))
}

/// Reductionist Server Service type alias
///
/// This type implements [tower::Service].
// FIXME: The Service type should be some form of tower::Service, but couldn't find the
// necessary trait bounds.
pub type Service = tower_http::normalize_path::NormalizePath<Router>;

/// Returns a [crate::app::Service] for the Active Storage server API
///
/// The service is populated with all routes as well as the following middleware:
///
/// * a [tower_http::trace::TraceLayer] for tracing requests and responses
/// * a [tower_http::validate_request::ValidateRequestHeaderLayer] for validating authorisation
///   headers
/// * a [tower_http::normalize_path::NormalizePathLayer] for trimming trailing slashes from
///   requests
pub fn service(args: &CommandLineArgs) -> Service {
    // Note that any middleware that should affect routing must wrap the router.
    // See
    // https://docs.rs/axum/0.6.18/axum/middleware/index.html#rewriting-request-uri-in-middleware.
    NormalizePathLayer::trim_trailing_slash().layer(router(args))
}

/// TODO: Return an OpenAPI schema
async fn schema() -> &'static str {
    "Hello, world!"
}

/// Download and optionally cache an object from S3
///
/// Requests a byte range if `offset` or `size` is specified in the request.
///
/// # Arguments
///
/// * `client`: S3 client object
/// * `request_data`: RequestData object for the request
/// * `resource_manager`: ResourceManager object
/// * `chunk_cache`: ChunkCache object
/// * `args`: CommandLineArgs object
async fn download_object<'a>(
    client: &s3_client::S3Client,
    request_data: &models::RequestData,
    resource_manager: &'a ResourceManager,
    chunk_cache: &ChunkCache,
    args: &CommandLineArgs,
) -> Result<Bytes, ActiveStorageError> {

    let key = format!("{},{:?}", client, request_data);

    // If we're using the chunk cache,
    // check if the key is in the cache and return the cached bytes if so.
    if args.use_chunk_cache {
        match chunk_cache.cache_get(&key) {
            Ok(cache_value_for_key) => {
                if let Some(cached_bytes) = cache_value_for_key {
                    return Ok(cached_bytes);
                }
            },
            Err(e) => {
                    // Propagate any cache error back as an ActiveStorageError
                    return Err(ActiveStorageError::CacheError{ error: format!("{:?}", e) });
            }
        };
    }

    // If we're given a size in the request data then use this to
    // get an initial guess at the required memory resources.
    let memory = request_data.size.unwrap_or(0);
    let mut mem_permits = resource_manager.memory(memory).await?;

    let range = s3_client::get_range(request_data.offset, request_data.size);
    let _conn_permits = resource_manager.s3_connection().await?;

    let data = client
        .download_object(
            &request_data.bucket,
            &request_data.object,
            range,
            resource_manager,
            &mut mem_permits,
        )
        .await;

    // If we're using the chunk cache,
    // store the data that has been successfully downloaded
    if args.use_chunk_cache {
        if let Ok(data_bytes) = &data {
            match chunk_cache.cache_set(key, data_bytes.clone()) {
                Ok(_) => {},
                Err(e) => {
                    // Propagate any cache error back as an ActiveStorageError
                    return Err(ActiveStorageError::CacheError{ error: format!("{:?}", e) });
                }
            }
        }
        // Increment the prometheus metric for cache misses
        LOCAL_CACHE_MISSES.with_label_values(&["disk"]).inc();
    }

    data
}

/// Handler for Active Storage operations
///
/// Downloads object data from S3 storage and executes the requested reduction operation.
///
/// This function is generic over any type implementing the [crate::operation::Operation] trait,
/// allowing it to handle any operation conforming to that interface.
///
/// Returns a `Result` with [crate::models::Response] on success and
/// [crate::error::ActiveStorageError] on failure.
///
/// # Arguments
///
/// * `auth`: Optional basic authentication header
/// * `request_data`: RequestData object for the request
async fn operation_handler<T: operation::Operation>(
    State(state): State<SharedAppState>,
    auth: Option<TypedHeader<Authorization<Basic>>>,
    ValidatedJson(request_data): ValidatedJson<models::RequestData>,
) -> Result<models::Response, ActiveStorageError> {
    let credentials = if let Some(TypedHeader(auth)) = auth {
        s3_client::S3Credentials::access_key(auth.username(), auth.password())
    } else {
        s3_client::S3Credentials::None
    };
    let s3_client = state
        .s3_client_map
        .get(&request_data.source, credentials)
        .instrument(tracing::Span::current())
        .await;

    let data = download_object(
        &s3_client,
        &request_data,
        &state.resource_manager,
        &state.chunk_cache,
        &state.args,
    )
    .instrument(tracing::Span::current())
    .await?;

    // All remaining work is synchronous. If the use_rayon argument was specified, delegate to the
    // Rayon thread pool. Otherwise, execute as normal using Tokio.
    if state.args.use_rayon {
        tokio_rayon::spawn(move || operation::<T>(request_data, data)).await
    } else {
        let _task_permit = state.resource_manager.task().await?;
        operation::<T>(request_data, data)
    }
}

/// Perform a reduction operation
///
/// This function encapsulates the synchronous part of an operation.
///
/// # Arguments
///
/// * `request_data`: RequestData object for the request.
/// * `data`: Object data `Bytes`.
fn operation<T: operation::Operation>(
    request_data: models::RequestData,
    data: Bytes,
) -> Result<models::Response, ActiveStorageError> {
    let ptr = data.as_ptr();
    let data = filter_pipeline::filter_pipeline(&request_data, data)?;
    if request_data.compression.is_some() || request_data.size.is_none() {
        // Validate the raw uncompressed data size now that we know it.
        models::validate_raw_size(data.len(), request_data.dtype, &request_data.shape)?;
    }
    if request_data.compression.is_none() && request_data.filters.is_none() {
        // Assert that we're using zero-copy.
        assert_eq!(ptr, data.as_ptr());
    }
    // Convert to a mutable vector to allow in-place byte order conversion.
    let ptr = data.as_ptr();
    let vec: Vec<u8> = data.into();
    // Assert that we're using zero-copy.
    assert_eq!(ptr, vec.as_ptr());
    debug_span!("operation").in_scope(|| T::execute(&request_data, vec))
}

/// Handler for unknown operations
///
/// Returns an [crate::error::ActiveStorageError].
///
/// # Arguments
///
/// * `operation`: the unknown operation from the URL path
async fn unknown_operation_handler(Path(operation): Path<String>) -> ActiveStorageError {
    ActiveStorageError::UnsupportedOperation { operation }
}
