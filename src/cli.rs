//! Command Line Interface (CLI) arguments.

use clap::Parser;

/// Reductionist command line interface
#[derive(Clone, Debug, Parser)]
pub struct CommandLineArgs {
    /// The IP address on which the proxy should listen
    #[arg(long, default_value = "0.0.0.0", env = "REDUCTIONIST_HOST")]
    pub host: String,
    /// The port to which the proxy should bind
    #[arg(long, default_value_t = 8080, env = "REDUCTIONIST_PORT")]
    pub port: u16,
    /// Flag indicating whether HTTPS should be used
    #[arg(long, default_value_t = false, env = "REDUCTIONIST_HTTPS")]
    pub https: bool,
    /// Path to the certificate file to be used for HTTPS encryption
    #[arg(
        long,
        default_value = "~/.config/reductionist/certs/cert.pem",
        env = "REDUCTIONIST_CERT_FILE"
    )]
    pub cert_file: String,
    /// Path to the key file to be used for HTTPS encryption
    #[arg(
        long,
        default_value = "~/.config/reductionist/certs/key.pem",
        env = "REDUCTIONIST_KEY_FILE"
    )]
    pub key_file: String,
    /// Maximum time in seconds to wait for operations to complete upon receiving `ctrl+c` signal.
    #[arg(long, default_value_t = 60, env = "REDUCTIONIST_SHUTDOWN_TIMEOUT")]
    pub graceful_shutdown_timeout: u64,
    /// Whether to enable sending traces to Jaeger.
    #[arg(long, default_value_t = false, env = "REDUCTIONIST_ENABLE_JAEGER")]
    pub enable_jaeger: bool,
    /// Whether to use Rayon for execution of CPU-bound tasks.
    #[arg(long, default_value_t = false, env = "REDUCTIONIST_USE_RAYON")]
    pub use_rayon: bool,
    /// Memory limit in bytes. Default is no limit.
    #[arg(long, env = "REDUCTIONIST_MEMORY_LIMIT")]
    pub memory_limit: Option<usize>,
    /// S3 connection limit. Default is no limit.
    #[arg(long, env = "REDUCTIONIST_S3_CONNECTION_LIMIT")]
    pub s3_connection_limit: Option<usize>,
    /// Thread limit for CPU-bound tasks. Default is one less than the number of CPUs. Used only
    /// when use_rayon is false.
    #[arg(long, env = "REDUCTIONIST_THREAD_LIMIT")]
    pub thread_limit: Option<usize>,

    /// Whether to enable caching of downloaded chunks.
    /// Default is disabled.
    #[arg(long, default_value_t = false, env = "REDUCTIONIST_USE_CHUNK_CACHE")]
    pub use_chunk_cache: bool,
    /// Path to the chunk cache store.
    /// This is required when the chunk cache is enabled.
    #[arg(long, env = "REDUCTIONIST_CHUNK_CACHE_PATH")]
    pub chunk_cache_path: Option<String>,
    /// Lifespan of cached chunks in seconds.
    /// Default is 1 day.
    #[arg(long, env = "REDUCTIONIST_CHUNK_CACHE_AGE")]
    pub chunk_cache_age: Option<u64>,
    /// Whether a cache hit refreshes the age of that chunk.
    /// Default is false.
    #[arg(long, default_value_t = false, env = "REDUCTIONIST_CHUNK_CACHE_AGE_REFRESH")]
    pub chunk_cache_age_refresh: bool,
}

/// Returns parsed command line arguments.
pub fn parse() -> CommandLineArgs {
    CommandLineArgs::parse()
}
