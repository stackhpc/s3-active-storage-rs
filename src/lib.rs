//! Reductionist is a web service that provides an API to perform reductions on numerical data
//! stored in an S3-compatible object store.  By implementing these reductions in the storage
//! system the volume of data that needs to be transferred to the end user is vastly reduced,
//! leading to faster computations.
//!
//! The work is funded by the
//! [ExCALIBUR project](https://www.metoffice.gov.uk/research/approach/collaboration/spf/excalibur)
//! and is done in collaboration with the
//! [University of Reading](http://www.reading.ac.uk/).
//!
//! This site provides documentation for the Reductionist source code.
//! Documentation for the Reductionist application is hosted on [GitHub](https://stackhpc.github.io/reductionist-rs).
//!
//! This is a performant implementation of the Active Storage Server.
//! The original Python functional prototype is available
//! [here](https://github.com/stackhpc/s3-active-storage-prototype).
//!
//! ## Features
//!
//! Reductionist provides the following features:
//!
//! * HTTP(S) API with JSON request data
//! * Access to data stored in S3-compatible storage
//! * Basic numerical operations on multi-dimensional arrays (count, min, max, select, sum)
//! * Perform calculations on a selection/slice of an array
//! * Perform calculations allowing for missing data
//! * Compressed data (GZip, Zlib)
//! * Filtered data (byte shuffle)
//! * Data with non-native byte order (endianness)
//! * Server resource (CPU, memory, files) management
//! * [Prometheus](https://prometheus.io/) metrics
//! * Tracing with an option to send data to [Jaeger](https://www.jaegertracing.io/)
//! * Ansible-based containerised deployment
//!
//! Reductionist is built on top of a number of open source components.
//!
//! * [Tokio](tokio), the most popular asynchronous Rust runtime.
//! * [Axum](axum) web framework, built by the Tokio team. Axum performs well in [various](https://github.com/programatik29/rust-web-benchmarks/blob/master/result/hello-world.md) [benchmarks](https://web-frameworks-benchmark.netlify.app/result?l=rust)
//!   and is built on top of various popular components, including the [hyper] HTTP library.
//! * [Serde](serde) performs (de)serialisation of JSON request and response data.
//! * [AWS SDK for S3](aws-sdk-s3) is used to interact with S3-compatible object stores.
//! * [ndarray] provides [NumPy](https://numpy.orgq)-like n-dimensional arrays used in numerical
//!   computation.
//!
//! ## Related projects
//!
//! * [PyActiveStorage](https://github.com/valeriupredoi/PyActiveStorage) is a Python library which performs reductions on numerical data in data sources such as netCDF4. It has support for delegating computation to Reductionist when the data is stored in an S3-compatible object store.

pub mod app;
pub mod array;
pub mod chunk_cache;
pub mod cli;
pub mod compression;
pub mod error;
pub mod filter_pipeline;
pub mod filters;
pub mod metrics;
pub mod models;
pub mod operation;
pub mod operations;
pub mod resource_manager;
pub mod s3_client;
pub mod server;
#[cfg(test)]
pub mod test_utils;
pub mod tracing;
pub mod types;
pub mod validated_json;
