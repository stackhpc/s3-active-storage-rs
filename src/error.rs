//! Error handling.

use aws_sdk_s3::error::ProvideErrorMetadata;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_smithy_types::byte_stream::error::Error as ByteStreamError;
use axum::{
    extract::rejection::JsonRejection,
    http::header,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use ndarray::ShapeError;
use serde::{Deserialize, Serialize};
use std::error::Error;
use thiserror::Error;
use tokio::sync::AcquireError;
use tracing::{event, Level};
use zune_inflate::errors::InflateDecodeErrors;

use crate::types::DValue;

/// Active Storage server error type
///
/// This type encapsulates the various errors that may occur.
/// Each variant may result in a different API error response.
#[derive(Debug, Error)]
pub enum ActiveStorageError {
    /// Error decompressing data
    #[error("failed to decompress data")]
    DecompressionFlate2(#[from] std::io::Error),

    /// Error decompressing data
    #[error("failed to decompress data")]
    DecompressionZune(#[from] InflateDecodeErrors),

    /// Attempt to perform an invalid operation on an empty array or selection
    #[error("cannot perform {operation} on empty array or selection")]
    EmptyArray { operation: &'static str },

    /// Error converting from bytes to a type
    #[error("failed to convert from bytes to {type_name}")]
    FromBytes { type_name: &'static str },

    /// Incompatible missing data descriptor
    #[error("Incompatible value {0} for missing")]
    IncompatibleMissing(DValue),

    /// Insufficient memory to process request
    #[error("Insufficient memory to process request ({requested} > {total})")]
    InsufficientMemory { requested: usize, total: usize },

    /// Error deserialising request data into RequestData
    #[error("request data is not valid")]
    RequestDataJsonRejection(#[from] JsonRejection),

    /// Error validating RequestData (single error)
    #[error("request data is not valid")]
    RequestDataValidationSingle(#[from] validator::ValidationError),

    /// Error validating RequestData (multiple errors)
    #[error("request data is not valid")]
    RequestDataValidation(#[from] validator::ValidationErrors),

    /// Error reading object data from S3
    #[error("error receiving object from S3 storage")]
    S3ByteStream(#[from] ByteStreamError),

    /// Missing Content-Length header in S3 response.
    #[error("S3 response missing Content-Length header")]
    S3ContentLengthMissing,

    /// Error while retrieving an object from S3
    #[error("error retrieving object from S3 storage")]
    S3GetObject(#[from] SdkError<GetObjectError>),

    /// Error acquiring a semaphore
    #[error("error acquiring resources")]
    SemaphoreAcquireError(#[from] AcquireError),

    /// Error creating ndarray ArrayView from Shape
    #[error("failed to create array from shape")]
    ShapeInvalid(#[from] ShapeError),

    /// Error converting between integer types
    #[error(transparent)]
    TryFromInt(#[from] std::num::TryFromIntError),

    /// Unsupported operation requested
    #[error("unsupported operation {operation}")]
    UnsupportedOperation { operation: String },

    /// Error using function cache
    #[error("function cache error {error}")]
    CacheError { error: String },
}

impl IntoResponse for ActiveStorageError {
    /// Convert from an `ActiveStorageError` into an [axum::response::Response].
    fn into_response(self) -> Response {
        ErrorResponse::from(self).into_response()
    }
}

/// Body of error response
///
/// Implements serde (de)serialise.
#[derive(Deserialize, Serialize)]
struct ErrorBody {
    /// Main error message
    message: String,

    /// Optional list of causes
    #[serde(skip_serializing_if = "Option::is_none")]
    caused_by: Option<Vec<String>>,
}

impl ErrorBody {
    /// Return a new ErrorBody
    ///
    /// # Arguments
    ///
    /// * `error`: The error that occurred
    fn new<E>(error: &E) -> Self
    where
        E: std::error::Error + Send + Sync,
    {
        let message = error.to_string();
        let mut caused_by = None;
        let mut current = error.source();
        while let Some(source) = current {
            let mut causes: Vec<String> = caused_by.unwrap_or_default();
            causes.push(source.to_string());
            caused_by = Some(causes);
            current = source.source();
        }
        // Remove duplicate entries.
        if let Some(caused_by) = caused_by.as_mut() {
            caused_by.dedup()
        }
        ErrorBody { message, caused_by }
    }
}

/// A response to send in error cases
///
/// Implements serde (de)serialise.
#[derive(Deserialize, Serialize)]
struct ErrorResponse {
    /// HTTP status of the response
    #[serde(skip)]
    status: StatusCode,

    /// Response body
    error: ErrorBody,
}

impl ErrorResponse {
    /// Return a new ErrorResponse
    ///
    /// # Arguments
    ///
    /// * `status`: HTTP status of the response
    /// * `error`: The error that occurred. This will be formatted into a suitable `ErrorBody`
    fn new<E>(status: StatusCode, error: &E) -> Self
    where
        E: std::error::Error + Send + Sync,
    {
        ErrorResponse {
            status,
            error: ErrorBody::new(error),
        }
    }

    /// Return a 400 bad request ErrorResponse
    fn bad_request<E>(error: &E) -> Self
    where
        E: std::error::Error + Send + Sync,
    {
        Self::new(StatusCode::BAD_REQUEST, error)
    }

    /// Return a 401 unauthorised ErrorResponse
    fn unauthorised<E>(error: &E) -> Self
    where
        E: std::error::Error + Send + Sync,
    {
        Self::new(StatusCode::UNAUTHORIZED, error)
    }

    /// Return a 404 not found ErrorResponse
    fn not_found<E>(error: &E) -> Self
    where
        E: std::error::Error + Send + Sync,
    {
        Self::new(StatusCode::NOT_FOUND, error)
    }

    /// Return a 500 internal server error ErrorResponse
    fn internal_server_error<E>(error: &E) -> Self
    where
        E: std::error::Error + Send + Sync,
    {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, error)
    }
}

impl From<ActiveStorageError> for ErrorResponse {
    /// Convert from an `ActiveStorageError` into an `ErrorResponse`.
    fn from(error: ActiveStorageError) -> Self {
        let response = match &error {
            // Bad request
            ActiveStorageError::DecompressionFlate2(_)
            | ActiveStorageError::DecompressionZune(_)
            | ActiveStorageError::EmptyArray { operation: _ }
            | ActiveStorageError::IncompatibleMissing(_)
            | ActiveStorageError::InsufficientMemory {
                requested: _,
                total: _,
            }
            | ActiveStorageError::RequestDataJsonRejection(_)
            | ActiveStorageError::RequestDataValidationSingle(_)
            | ActiveStorageError::RequestDataValidation(_)
            | ActiveStorageError::S3ContentLengthMissing
            | ActiveStorageError::ShapeInvalid(_) => Self::bad_request(&error),

            // Not found
            ActiveStorageError::UnsupportedOperation { operation: _ } => Self::not_found(&error),

            // Internal server error
            ActiveStorageError::FromBytes { type_name: _ }
            | ActiveStorageError::TryFromInt(_)
            | ActiveStorageError::S3ByteStream(_)
            | ActiveStorageError::SemaphoreAcquireError(_) => Self::internal_server_error(&error),

            ActiveStorageError::S3GetObject(sdk_error) => {
                // Tailor the response based on the specific SdkError variant.
                match &sdk_error {
                    // These are generic SdkError variants.
                    // Internal server error
                    SdkError::ConstructionFailure(_)
                    | SdkError::DispatchFailure(_)
                    | SdkError::ResponseError(_)
                    | SdkError::TimeoutError(_) => Self::internal_server_error(&error),

                    // This is a more specific ServiceError variant, with GetObjectError as the
                    // inner error.
                    SdkError::ServiceError(get_obj_error) => {
                        let get_obj_error = get_obj_error.err();
                        match get_obj_error {
                            GetObjectError::InvalidObjectState(_)
                            | GetObjectError::NoSuchKey(_) => Self::bad_request(&error),

                            // Quite a lot of error cases end up as unhandled. Attempt to determine
                            // the error from the code.
                            _ => {
                                match get_obj_error.code() {
                                    // Bad request
                                    Some("NoSuchBucket") => Self::bad_request(&error),

                                    // Unauthorised
                                    Some("InvalidAccessKeyId")
                                    | Some("SignatureDoesNotMatch")
                                    | Some("AccessDenied") => Self::unauthorised(&error),

                                    // Internal server error
                                    _ => Self::internal_server_error(&error),
                                }
                            }
                        }
                    }

                    // The enum is marked as non-exhaustive
                    _ => Self::internal_server_error(&error),
                }
            }
            ActiveStorageError::CacheError { error: _ } => todo!(),
        };

        // Log server errors.
        if response.status.is_server_error() {
            event!(Level::ERROR, "{}", error.to_string());
            let mut current = error.source();
            while let Some(source) = current {
                event!(Level::ERROR, "Caused by: {}", source.to_string());
                current = source.source();
            }
        }

        response
    }
}

impl IntoResponse for ErrorResponse {
    /// Convert from an `ErrorResponse` into an `axum::response::Response`.
    ///
    /// Renders the response as JSON.
    fn into_response(self) -> Response {
        let json_body = serde_json::to_string_pretty(&self);
        match json_body {
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to serialise error response: {}", err),
            )
                .into_response(),
            Ok(json_body) => (
                self.status,
                [(&header::CONTENT_TYPE, mime::APPLICATION_JSON.to_string())],
                json_body,
            )
                .into_response(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use aws_sdk_s3::types::error::NoSuchKey;
    use aws_smithy_runtime_api::http::Response as SmithyResponse;
    use aws_smithy_runtime_api::http::StatusCode as SmithyStatusCode;
    use aws_smithy_types::error::ErrorMetadata as SmithyError;
    use hyper::HeaderMap;

    // Jump through the hoops to get the body as a string.
    async fn body_string(response: Response) -> String {
        String::from_utf8(
            hyper::body::to_bytes(response.into_body())
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap()
    }

    async fn test_active_storage_error(
        error: ActiveStorageError,
        status: StatusCode,
        message: &str,
        caused_by: Option<Vec<&'static str>>,
    ) {
        let response = error.into_response();
        assert_eq!(status, response.status());
        let mut headers = HeaderMap::new();
        headers.insert(&header::CONTENT_TYPE, "application/json".parse().unwrap());
        assert_eq!(headers, *response.headers());
        let error_response: ErrorResponse =
            serde_json::from_str(&body_string(response).await).unwrap();
        assert_eq!(message.to_string(), error_response.error.message);
        // Map Vec items from str to String
        let caused_by = caused_by.map(|cb| cb.iter().map(|s| s.to_string()).collect());
        assert_eq!(caused_by, error_response.error.caused_by);
    }

    #[tokio::test]
    async fn decompression_flate2_error() {
        let io_error = std::io::Error::new(std::io::ErrorKind::InvalidInput, "decompression error");
        let error = ActiveStorageError::DecompressionFlate2(io_error);
        let message = "failed to decompress data";
        let caused_by = Some(vec!["decompression error"]);
        test_active_storage_error(error, StatusCode::BAD_REQUEST, message, caused_by).await;
    }

    #[tokio::test]
    async fn decompression_zune_error() {
        let zune_error = InflateDecodeErrors::new_with_error(
            zune_inflate::errors::DecodeErrorStatus::InsufficientData,
        );
        let error = ActiveStorageError::DecompressionZune(zune_error);
        let message = "failed to decompress data";
        let caused_by = Some(vec!["Insufficient data\n\n\n"]);
        test_active_storage_error(error, StatusCode::BAD_REQUEST, message, caused_by).await;
    }

    #[tokio::test]
    async fn empty_array_op_error() {
        let error = ActiveStorageError::EmptyArray { operation: "foo" };
        let message = "cannot perform foo on empty array or selection";
        let caused_by = None;
        test_active_storage_error(error, StatusCode::BAD_REQUEST, message, caused_by).await;
    }

    #[tokio::test]
    async fn from_bytes_error() {
        let error = ActiveStorageError::FromBytes { type_name: "foo" };
        let message = "failed to convert from bytes to foo";
        let caused_by = None;
        test_active_storage_error(error, StatusCode::INTERNAL_SERVER_ERROR, message, caused_by)
            .await;
    }

    #[tokio::test]
    async fn incompatible_missing() {
        let value = 32.into();
        let error = ActiveStorageError::IncompatibleMissing(value);
        let message = "Incompatible value 32 for missing";
        let caused_by = None;
        test_active_storage_error(error, StatusCode::BAD_REQUEST, message, caused_by).await;
    }

    #[tokio::test]
    async fn insufficient_memory() {
        let error = ActiveStorageError::InsufficientMemory {
            requested: 2,
            total: 1,
        };
        let message = "Insufficient memory to process request (2 > 1)";
        let caused_by = None;
        test_active_storage_error(error, StatusCode::BAD_REQUEST, message, caused_by).await;
    }

    #[tokio::test]
    async fn request_data_validation_single() {
        let validation_error = validator::ValidationError::new("foo");
        let error = ActiveStorageError::RequestDataValidationSingle(validation_error);
        let message = "request data is not valid";
        let caused_by = Some(vec!["Validation error: foo [{}]"]);
        test_active_storage_error(error, StatusCode::BAD_REQUEST, message, caused_by).await;
    }

    #[tokio::test]
    async fn request_data_validation() {
        let mut validation_errors = validator::ValidationErrors::new();
        let validation_error = validator::ValidationError::new("foo");
        validation_errors.add("bar", validation_error);
        let error = ActiveStorageError::RequestDataValidation(validation_errors);
        let message = "request data is not valid";
        let caused_by = Some(vec!["bar: Validation error: foo [{}]"]);
        test_active_storage_error(error, StatusCode::BAD_REQUEST, message, caused_by).await;
    }

    #[tokio::test]
    async fn s3_content_length_missing() {
        let error = ActiveStorageError::S3ContentLengthMissing;
        let message = "S3 response missing Content-Length header";
        test_active_storage_error(error, StatusCode::BAD_REQUEST, message, None).await;
    }

    // Helper function for S3 GetObjectError errors
    async fn test_s3_get_object_error(
        sdk_error: SdkError<GetObjectError>,
        status: StatusCode,
        caused_by: Option<Vec<&'static str>>,
    ) {
        let error = ActiveStorageError::S3GetObject(sdk_error);
        let message = "error retrieving object from S3 storage";
        test_active_storage_error(error, status, message, caused_by).await;
    }

    fn get_smithy_response() -> SmithyResponse {
        let sdk_body = "body";
        let status: SmithyStatusCode = 400.try_into().unwrap();
        SmithyResponse::new(status, sdk_body.into())
    }

    #[tokio::test]
    async fn s3_get_object_error() {
        // Jump through hoops to create an SdkError.
        let no_such_key = NoSuchKey::builder().build();
        let get_object_error = GetObjectError::NoSuchKey(no_such_key);
        let sdk_error = SdkError::service_error(get_object_error, get_smithy_response());
        let caused_by = Some(vec!["service error", "NoSuchKey"]);
        test_s3_get_object_error(sdk_error, StatusCode::BAD_REQUEST, caused_by).await;
    }

    #[tokio::test]
    async fn s3_get_object_invalid_access_key_error() {
        // Jump through hoops to create an SdkError.
        let smithy_error = SmithyError::builder()
            .message("fake smithy error")
            .code("InvalidAccessKeyId")
            .build();
        let get_object_error = GetObjectError::generic(smithy_error);
        let sdk_error = SdkError::service_error(get_object_error, get_smithy_response());
        let caused_by = Some(vec![
            "service error",
            "unhandled error (InvalidAccessKeyId)",
            "Error { code: \"InvalidAccessKeyId\", message: \"fake smithy error\" }",
        ]);
        test_s3_get_object_error(sdk_error, StatusCode::UNAUTHORIZED, caused_by).await;
    }

    #[tokio::test]
    async fn s3_get_object_no_such_bucket() {
        // Jump through hoops to create an SdkError.
        let smithy_error = SmithyError::builder()
            .message("fake smithy error")
            .code("NoSuchBucket")
            .build();
        let get_object_error = GetObjectError::generic(smithy_error);
        let sdk_error = SdkError::service_error(get_object_error, get_smithy_response());
        let caused_by = Some(vec![
            "service error",
            "unhandled error (NoSuchBucket)",
            "Error { code: \"NoSuchBucket\", message: \"fake smithy error\" }",
        ]);
        test_s3_get_object_error(sdk_error, StatusCode::BAD_REQUEST, caused_by).await;
    }

    #[tokio::test]
    async fn s3_get_object_sig_does_not_match_error() {
        // Jump through hoops to create an SdkError.
        let smithy_error = SmithyError::builder()
            .message("fake smithy error")
            .code("SignatureDoesNotMatch")
            .build();
        let get_object_error = GetObjectError::generic(smithy_error);
        let sdk_error = SdkError::service_error(get_object_error, get_smithy_response());
        let caused_by = Some(vec![
            "service error",
            "unhandled error (SignatureDoesNotMatch)",
            "Error { code: \"SignatureDoesNotMatch\", message: \"fake smithy error\" }",
        ]);
        test_s3_get_object_error(sdk_error, StatusCode::UNAUTHORIZED, caused_by).await;
    }

    #[tokio::test]
    async fn s3_get_object_access_denied_error() {
        // Jump through hoops to create an SdkError.
        let smithy_error = SmithyError::builder()
            .message("fake smithy error")
            .code("AccessDenied")
            .build();
        let get_object_error = GetObjectError::generic(smithy_error);
        let sdk_error = SdkError::service_error(get_object_error, get_smithy_response());
        let caused_by = Some(vec![
            "service error",
            "unhandled error (AccessDenied)",
            "Error { code: \"AccessDenied\", message: \"fake smithy error\" }",
        ]);
        test_s3_get_object_error(sdk_error, StatusCode::UNAUTHORIZED, caused_by).await;
    }

    #[tokio::test]
    async fn s3_byte_stream_error() {
        // ByteStreamError provides a From impl for std::io:Error.
        let error = ActiveStorageError::S3ByteStream(
            std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into(),
        );
        let message = "error receiving object from S3 storage";
        let caused_by = Some(vec!["IO error", "unexpected end of file"]);
        test_active_storage_error(error, StatusCode::INTERNAL_SERVER_ERROR, message, caused_by)
            .await;
    }

    #[tokio::test]
    async fn semaphore_acquire_error() {
        let sem = tokio::sync::Semaphore::new(1);
        sem.close();
        let error = ActiveStorageError::SemaphoreAcquireError(sem.acquire().await.unwrap_err());
        let message = "error acquiring resources";
        let caused_by = Some(vec!["semaphore closed"]);
        test_active_storage_error(error, StatusCode::INTERNAL_SERVER_ERROR, message, caused_by)
            .await;
    }

    #[tokio::test]
    async fn shape_error() {
        let error = ActiveStorageError::ShapeInvalid(ShapeError::from_kind(
            ndarray::ErrorKind::OutOfBounds,
        ));
        let message = "failed to create array from shape";
        let caused_by = Some(vec!["ShapeError/OutOfBounds: out of bounds indexing"]);
        test_active_storage_error(error, StatusCode::BAD_REQUEST, message, caused_by).await;
    }

    #[tokio::test]
    async fn try_from_int_error() {
        let error = ActiveStorageError::TryFromInt(u8::try_from(-1_i8).unwrap_err());
        let message = "out of range integral type conversion attempted";
        let caused_by = None;
        test_active_storage_error(error, StatusCode::INTERNAL_SERVER_ERROR, message, caused_by)
            .await;
    }

    #[tokio::test]
    async fn unsupported_operation() {
        let error = ActiveStorageError::UnsupportedOperation {
            operation: "foo".to_string(),
        };
        let message = "unsupported operation foo";
        let caused_by = None;
        test_active_storage_error(error, StatusCode::NOT_FOUND, message, caused_by).await;
    }
}
