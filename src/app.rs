//! Active Storage server API

use crate::error::ActiveStorageError;
use crate::models;
use crate::operation;
use crate::operations;
use crate::s3_client;
use crate::validated_json::ValidatedJson;

use axum::{
    body::{Body, Bytes},
    extract::{Path, State},
    headers::authorization::{Authorization, Basic},
    http::header,
    http::Request,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router, TypedHeader,
};

use tower::ServiceBuilder;
use tower_http::normalize_path::NormalizePathLayer;
use tower_http::trace::TraceLayer;
use tower_http::validate_request::ValidateRequestHeaderLayer;

/// `x-activestorage-dtype` header definition
static HEADER_DTYPE: header::HeaderName = header::HeaderName::from_static("x-activestorage-dtype");
/// `x-activestorage-shape` header definition
static HEADER_SHAPE: header::HeaderName = header::HeaderName::from_static("x-activestorage-shape");

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
            ],
            self.body,
        )
            .into_response()
    }
}

/// Returns a [axum::Router] for the Active Storage server API
///
/// The router is populated with all routes as well as the following middleware:
///
/// * a [tower_http::trace::TraceLayer] for tracing requests and responses
/// * a [tower_http::validate_request::ValidateRequestHeaderLayer] for validating authorisation
///   headers
/// * a [tower_http::normalize_path::NormalizePathLayer] for trimming trailing slashes from
///   requests
pub fn router() -> Router {
    fn v1() -> Router {
        Router::new()
            .route(
                "/count",
                post(operation_handler).with_state(&operations::Count {}),
            )
            .route(
                "/max",
                post(operation_handler).with_state(&operations::Max {}),
            )
            .route(
                "/mean",
                post(operation_handler).with_state(&operations::Mean {}),
            )
            .route(
                "/min",
                post(operation_handler).with_state(&operations::Min {}),
            )
            //.route("/select", post(operation_handler).with_state(&operations::Select { }))
            .route(
                "/sum",
                post(operation_handler).with_state(&operations::Sum {}),
            )
            .route("/:operation", post(unknown_operation_handler))
            .layer(
                ServiceBuilder::new()
                    .layer(TraceLayer::new_for_http())
                    .layer(ValidateRequestHeaderLayer::custom(
                        // Validate that an authorization header has been provided.
                        |request: &mut Request<Body>| {
                            if request.headers().contains_key(header::AUTHORIZATION) {
                                Ok(())
                            } else {
                                Err(StatusCode::UNAUTHORIZED.into_response())
                            }
                        },
                    )),
            )
    }

    Router::new()
        .route("/.well-known/s3-active-storage-schema", get(schema))
        .nest("/v1", v1())
        .layer(NormalizePathLayer::trim_trailing_slash())
}

/// TODO: Return an OpenAPI schema
async fn schema() -> &'static str {
    "Hello, world!"
}

/// Download an object from S3
///
/// Requests a byte range if `offset` or `size` is specified in the request.
///
/// # Arguments
///
/// * `auth`: Basic authentication credentials
/// * `request_data`: RequestData object for the request
async fn download_object(
    auth: &Authorization<Basic>,
    request_data: &models::RequestData,
) -> Result<Bytes, ActiveStorageError> {
    let range = s3_client::get_range(request_data.offset, request_data.size);
    s3_client::S3Client::new(&request_data.source, auth.username(), auth.password())
        .await
        .download_object(&request_data.bucket, &request_data.object, range)
        .await
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
/// * `auth`: Basic authorization header
/// * `request_data`: RequestData object for the request
async fn operation_handler<T: operation::Operation>(
    State(operation): State<&T>,
    TypedHeader(auth): TypedHeader<Authorization<Basic>>,
    ValidatedJson(request_data): ValidatedJson<models::RequestData>,
) -> Result<models::Response, ActiveStorageError> {
    let data = download_object(&auth, &request_data).await?;
    operation.execute(&request_data, &data)
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
