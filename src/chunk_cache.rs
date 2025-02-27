use crate::cli::CommandLineArgs;

use bytes::Bytes;
use cached::stores::{DiskCacheBuilder, DiskCache};

pub type ChunkCache = DiskCache<String, Bytes>;

/// Build a disk based cache for downloaded chunks
///
/// Returns a `ChunkCache` with [cached::stores::DiskCache] on success.
///
/// # Arguments
///
/// * `args`: CommandLineArgs object
pub fn build(args: &CommandLineArgs) -> ChunkCache {

    if args.use_chunk_cache {
        // Specify the path to the cache directory,
        // this is required when the optional cache is enabled.
        let chunk_cache_path = <Option<String> as Clone>::clone(&args.chunk_cache_path)
            .expect("The chunk cache path must be specified when the chunk cache is enabled");
        // Specify the cache TTL/lifespan in seconds.
        let chunk_cache_lifespan: u64 = args.chunk_cache_age.unwrap_or(86400);
        // Specify whether cache hits refresh the TTL.
        let chunk_cache_refresh: bool = args.chunk_cache_age_refresh;
        DiskCacheBuilder::new("chunk_cache")
            .set_disk_directory(chunk_cache_path)
            .set_lifespan(chunk_cache_lifespan)
            .set_refresh(chunk_cache_refresh)
            .build()
            .expect("Failed to build object chunk cache")
    } else {
        // Create a dummy cache that does nothing, it's not used.
        DiskCacheBuilder::new("DUMMY")
            .build()
            .expect("Failed to build dummy object chunk cache")
    }     
}
