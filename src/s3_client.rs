//! A simplified S3 client that supports downloading objects.
//! It attempts to hide the complexities of working with the AWS SDK for S3.

use std::fmt::Display;

use crate::error::ActiveStorageError;
use crate::resource_manager::ResourceManager;

use aws_credential_types::Credentials;
use aws_sdk_s3::config::BehaviorVersion;
use aws_sdk_s3::Client;
use aws_types::region::Region;
use axum::body::Bytes;
use hashbrown::HashMap;
use tokio::sync::{RwLock, SemaphorePermit};
use tracing::Instrument;
use url::Url;

#[derive(Clone, Eq, Hash, PartialEq)]
pub enum S3Credentials {
    AccessKey {
        access_key: String,
        secret_key: String,
    },
    None,
}

impl S3Credentials {
    /// Create an access key credential.
    pub fn access_key(access_key: &str, secret_key: &str) -> Self {
        S3Credentials::AccessKey {
            access_key: access_key.to_string(),
            secret_key: secret_key.to_string(),
        }
    }
}

/// A map containing initialised S3Client objects.
///
/// The [aws_sdk_s3::Client] object is relatively expensive to create, so we reuse them where
/// possible. This type provides a map for storing the clients objects.
///
/// The map's key is a 2-tuple of the S3 URL and credentials.
/// The value is the corresponding client object.
pub struct S3ClientMap {
    /// A [hashbrown::HashMap] for storing the S3 clients. A read-write lock synchronises access to
    /// the map, optimised for reads.
    map: RwLock<HashMap<(Url, S3Credentials), S3Client>>,
}

// FIXME: Currently clients are never removed from the map. If a large number of endpoints or
// credentials are used this will cause the map to grow indefinitely with a large number of
// clients. An ageing mechanism should be implemented
impl S3ClientMap {
    /// Create and return an [crate::s3_client::S3ClientMap].
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        S3ClientMap {
            map: RwLock::new(HashMap::new()),
        }
    }

    /// Get or create an [crate::s3_client::S3Client] object from the map.
    ///
    /// # Arguments
    ///
    /// * `url`: Object storage API URL
    /// * `credentials`: Object storage account credentials
    pub async fn get(&self, url: &Url, credentials: S3Credentials) -> S3Client {
        let key = (url.clone(), credentials.clone());
        // Common case: return an existing client from the map.
        {
            let map = self.map.read().await;
            if let Some(client) = map.get(&key) {
                return client.clone();
            }
        }
        // Less common case: create a new client, insert it into the map and return it.
        let mut map = self.map.write().await;
        // Allow for a possible race here since we dropped the read lock.
        if let Some(client) = map.get(&key) {
            client.clone()
        } else {
            tracing::info!("Creating new S3 client for {}", url);
            let client = S3Client::new(url, credentials).await;
            let (_, client) = map.insert_unique_unchecked(key, client);
            client.clone()
        }
    }
}

/// S3 client object.
#[derive(Clone)]
pub struct S3Client {
    /// Underlying AWS SDK S3 client object.
    client: Client,
    /// A unique identifier for the client
    // TODO: Make this a hash of url + access key + secret key
    // using https://github.com/RustCrypto/hashes?tab=readme-ov-file
    // This will be more urgently required once an ageing mechanism
    // is implemented for [crate::S3ClientMap].
    id: String,
}

// Required so that client can be used as part of the lookup
// key for a local chunk cache.
impl Display for S3Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl S3Client {
    /// Creates an S3Client object
    ///
    /// # Arguments
    ///
    /// * `url`: Object storage API URL
    /// * `credentials`: Object storage account credentials
    pub async fn new(url: &Url, credentials: S3Credentials) -> Self {
        let region = Region::new("us-east-1");
        let builder = aws_sdk_s3::Config::builder().behavior_version(BehaviorVersion::latest());
        let builder = match credentials {
            S3Credentials::AccessKey {
                access_key,
                secret_key,
            } => {
                let credentials = Credentials::from_keys(access_key, secret_key, None);
                builder.credentials_provider(credentials)
            }
            S3Credentials::None => builder,
        };
        let s3_config = builder
            .region(Some(region))
            .endpoint_url(url.to_string())
            .force_path_style(true)
            .build();
        let client = Client::from_conf(s3_config);
        Self {
            client,
            id: uuid::Uuid::new_v4().to_string(),
        }
    }

    /// Downloads an object from object storage and returns the data as Bytes
    ///
    /// # Arguments
    ///
    /// * `bucket`: Name of the bucket
    /// * `key`: Name of the object in the bucket
    /// * `range`: Optional byte range
    /// * `resource_manager`: ResourceManager object
    /// * `mem_permits`: Optional SemaphorePermit for any memory resources reserved
    pub async fn download_object<'a>(
        self: &S3Client,
        bucket: &str,
        key: &str,
        range: Option<String>,
        resource_manager: &'a ResourceManager,
        mem_permits: &mut Option<SemaphorePermit<'a>>,
    ) -> Result<Bytes, ActiveStorageError> {
        let mut response = self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .set_range(range)
            .send()
            .instrument(tracing::Span::current())
            .await?;
        // Fail if the content length header is missing.
        let content_length: usize = response
            .content_length()
            .ok_or(ActiveStorageError::S3ContentLengthMissing)?
            .try_into()?;

        // FIXME: how to account for compressed data?
        if mem_permits.is_none() || mem_permits.as_ref().unwrap().num_permits() == 0 {
            *mem_permits = resource_manager.memory(content_length).await?;
        };

        // The data returned by the S3 client does not have any alignment guarantees. In order to
        // reinterpret the data as an array of numbers with a higher alignment than 1, we need to
        // return the data in Bytes object in which the underlying data has a higher alignment.
        // For now we're hard-coding an alignment of 8 bytes, although this should depend on the
        // data type, and potentially whether there are any SIMD requirements.
        // Create an 8-byte aligned Vec<u8>.
        let mut buf = maligned::align_first::<u8, maligned::A8>(content_length);

        // Iterate over the streaming response, copying data into the aligned Vec<u8>.
        while let Some(bytes) = response
            .body
            .try_next()
            .instrument(tracing::Span::current())
            .await?
        {
            buf.extend_from_slice(&bytes)
        }
        // Return as Bytes.
        Ok(buf.into())
    }
}

/// Return an optional byte range string based on the offset and size.
///
/// The returned string is compatible with the HTTP Range header.
///
/// # Arguments
///
/// * `offset`: Optional offset of data in bytes
/// * `size`: Optional size of data in bytes
pub fn get_range(offset: Option<usize>, size: Option<usize>) -> Option<String> {
    match (offset, size) {
        (offset, Some(size)) => {
            // Default offset to 0.
            let offset = offset.unwrap_or(0);
            // Range-end is inclusive.
            let end = offset + size - 1;
            Some(format!("bytes={}-{}", offset, end))
        }
        (Some(offset), None) => Some(format!("bytes={}-", offset)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    fn make_access_key() -> S3Credentials {
        S3Credentials::access_key("user", "password")
    }

    fn make_alt_access_key() -> S3Credentials {
        S3Credentials::access_key("user2", "password")
    }

    #[tokio::test]
    async fn s3_client_map() {
        let url = Url::parse("http://example.com").unwrap();
        let map = S3ClientMap::new();
        map.get(&url, make_access_key()).await;
        map.get(&url, make_access_key()).await;
        assert_eq!(map.map.read().await.len(), 1);
        map.get(&url, make_alt_access_key()).await;
        assert_eq!(map.map.read().await.len(), 2);
        map.get(&url, S3Credentials::None).await;
        map.get(&url, S3Credentials::None).await;
        assert_eq!(map.map.read().await.len(), 3);
    }

    #[tokio::test]
    async fn new() {
        let url = Url::parse("http://example.com").unwrap();
        S3Client::new(&url, make_access_key()).await;
    }

    #[tokio::test]
    async fn new_no_auth() {
        let url = Url::parse("http://example.com").unwrap();
        S3Client::new(&url, S3Credentials::None).await;
    }

    #[test]
    fn get_range_none() {
        assert_eq!(None, get_range(None, None));
    }

    #[test]
    fn get_range_both() {
        assert_eq!(Some("bytes=1-2".to_string()), get_range(Some(1), Some(2)));
    }

    #[test]
    fn get_range_offset() {
        assert_eq!(Some("bytes=1-".to_string()), get_range(Some(1), None));
    }

    #[test]
    fn get_range_size() {
        assert_eq!(Some("bytes=0-1".to_string()), get_range(None, Some(2)));
    }
}
