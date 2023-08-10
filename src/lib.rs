//! This crate provides an Active Storage Server. It implements simple reductions on S3 objects
//! containing numeric binary data.  By implementing these reductions in the storage system the
//! volume of data that needs to be transferred to the end user is vastly reduced, leading to
//! faster computations.
//!
//! The work is funded by the
//! [ExCALIBUR project](https://www.metoffice.gov.uk/research/approach/collaboration/spf/excalibur)
//! and is done in collaboration with the
//! [University of Reading](http://www.reading.ac.uk/).
//!
//! This is a performant implementation of the Active Storage Server.
//! The original Python functional prototype is available
//! [here](https://github.com/stackhpc/s3-active-storage-prototype).
//!
//! The Active Storage Server is built on top of a number of open source components.
//!
//! * [Tokio](tokio), the most popular asynchronous Rust runtime.
//! * [Axum](axum) web framework, built by the Tokio team. Axum performs well in [various](https://github.com/programatik29/rust-web-benchmarks/blob/master/result/hello-world.md) [benchmarks](https://web-frameworks-benchmark.netlify.app/result?l=rust)
//!   and is built on top of various popular components, including the [hyper] HTTP library.
//! * [Serde](serde) performs (de)serialisation of JSON request and response data.
//! * [AWS SDK for S3](aws-sdk-s3) is used to interact with S3-compatible object stores.
//! * [ndarray] provides [NumPy](https://numpy.orgq)-like n-dimensional arrays used in numerical
//!   computation.
//!
//! ## Concepts
//!
//! The Reductionist server supports the application of reductions to S3 objects that contain numeric binary data. These reductions are specified by making a HTTP post request to the active storage proxy service.
//!
//! The Reductionist server does not attempt to infer the datatype - it must be told the datatype to use based on knowledge that the client already has about the S3 object.
//!
//! For example, if the original object has the following URL:
//!
//! ```not_rust
//! http[s]://s3.example.org/my-bucket/path/to/object
//! ```
//!
//! Then Reductionist server could be used by making post requests to specfic reducer endpoints:
//!
//! ```not_rust
//! http[s]://s3-proxy.example.org/v1/{reducer}/
//! ```
//!
//! with a JSON payload of the form:
//!
//! ```not_rust
//! {
//!     // The URL for the S3 source
//!     // - required
//!     "source": "https://s3.example.com/,
//!
//!     // The name of the S3 bucket
//!     // - required
//!     "bucket": "my-bucket",
//!
//!     // The path to the object within the bucket
//!     // - required
//!     "object": "path/to/object",
//!
//!     // The data type to use when interpreting binary data
//!     // - required
//!     "dtype": "int32|int64|uint32|uint64|float32|float64",
//!
//!     // The byte order (endianness) of the data
//!     // - optional, defaults to native byte order of Reductionist server
//!     "byte_order": "big|little",
//!
//!     // The offset in bytes to use when reading data
//!     // - optional, defaults to zero
//!     "offset": 0,
//!
//!     // The number of bytes to read
//!     // - optional, defaults to the size of the entire object
//!     "size": 128,
//!
//!     // The shape of the data (i.e. the size of each dimension)
//!     // - optional, defaults to a simple 1D array
//!     "shape": [20, 5],
//!
//!     // Indicates whether the data is in C order (row major)
//!     // or Fortran order (column major, indicated by 'F')
//!     // - optional, defaults to 'C'
//!     "order": "C|F",
//!
//!     // An array of [start, end, stride] tuples indicating the data to be operated on
//!     // (if given, you must supply one tuple per element of "shape")
//!     // - optional, defaults to the whole array
//!     "selection": [
//!         [0, 19, 2],
//!         [1, 3, 1]
//!     ],
//!
//!     // Algorithm used to compress the data
//!     // - optional, defaults to no compression
//!     "compression": {"id": "gzip|zlib"},
//!
//!     // List of algorithms used to filter the data
//!     // - optional, defaults to no filters
//!     "filters": [{"id": "shuffle", "element_size": 4}],
//!
//!     // Missing data description
//!     // - optional, defaults to no missing data
//!     // - exactly one of the keys below should be specified
//!     // - the values should match the data type (dtype)
//!     "missing": {
//!         "missing_value": 42,
//!         "missing_values": [42, -42],
//!         "valid_min": 42,
//!         "valid_max": 42,
//!         "valid_range": [-42, 42],
//!     }
//! }
//! ```
//!
//! The currently supported reducers are `max`, `min`, `sum`, `select` and `count`. All reducers return the result using the same datatype as specified in the request except for `count` which always returns the result as `int64`.
//!
//! The proxy returns the following headers to the HTTP response:
//!
//! * `x-activestorage-dtype`: The data type of the data in the response payload. One of `int32`, `int64`, `uint32`, `uint64`, `float32` or `float64`.
//! * `x-activestorage-byte-order`: The byte order of the data in the response payload. Either `big` or `little`.
//! * `x-activestrorage-shape`: A JSON-encoded list of numbers describing the shape of the data in the response payload. May be an empty list for a scalar result.
//! * `x-activestorage-count`: The number of non-missing array elements operated on while performing the requested reduction. This header is useful, for example, to calculate the mean over multiple requests where the number of items operated on may differ between chunks.
//!
//! ## Running
//!
//! There are various ways to run the Reductionist server.
//!
//! ### Running in a container
//!
//! The simplest method is to run it in a container using a pre-built image:
//!
//! ```sh
//! docker run -it --detach --rm --net=host --name reductionist ghcr.io/stackhpc/reductionist-rs:latest
//! ```
//!
//! Images are published to [GitHub Container Registry](https://github.com/stackhpc/reductionist-rs/pkgs/container/reductionist-rs) when the project is released.
//! The `latest` tag corresponds to the most recent release, or you can use a specific release e.g. `0.1.0`.
//!
//! This method does not require access to the source code.
//!
//! ### Building a container image
//!
//! If you need to use unreleased changes, but still want to run in a container, it is possible to build an image.
//! First, clone this repository:
//!
//! ```sh
//! git clone https://github.com/stackhpc/reductionist-rs.git
//! cd reductionist-rs
//! ```
//!
//! ```sh
//! make build
//! ```
//!
//! The image will be tagged as `reductionist`.
//! The image may be pushed to a registry, or deployed locally.
//!
//! ```sh
//! make run
//! ```
//!
//! ## Build
//!
//! If you prefer not to run the Reductionist server in a container, it will be necessary to build a binary.
//! Building locally may also be preferable during development to take advantage of incremental compilation.
//!
//! ### Prerequisites
//!
//! This project is written in Rust, and as such requires a Rust toolchain to be installed in order to build it.
//! The Minimum Supported Rust Version (MSRV) is 1.66.1, due to a dependency on the [AWS SDK](https://github.com/awslabs/aws-sdk-rust).
//! It may be necessary to use [rustup](https://rustup.rs/) rather than the OS provided Rust toolchain to meet this requirement.
//! See the [Rust book](https://doc.rust-lang.org/book/ch01-01-installation.html) for toolchain installation.
//!
//! ### Build and run Reductionist
//!
//! First, clone this repository:
//!
//! ```sh
//! git clone https://github.com/stackhpc/reductionist-rs.git
//! cd reductionist-rs
//! ```
//!
//! Next, use Cargo to build the package:
//!
//! ```sh
//! cargo build --release
//! ```
//!
//! The active storage server may be run using Cargo:
//!
//! ```sh
//! cargo run --release
//! ```
//!
//! Or installed to the system:
//!
//! ```sh
//! cargo install --path . --locked
//! ```
//!
//! Then run:
//!
//! ```sh
//! reductionist
//! ```
//!
//! ## Testing
//!
//! For simple testing purposes Minio is a convenient object storage server.
//!
//! ### Deploy Minio object storage
//!
//! Start a local [Minio](https://min.io/) server which serves the test data:
//!
//! ```sh
//! ./scripts/minio-start
//! ```
//!
//! The Minio server will run in a detached container and may be stopped:
//!
//! ```sh
//! ./scripts/minio-stop
//! ```
//!
//! Note that object data is not preserved when the container is stopped.
//!
//! ### Upload some test data
//!
//! A script is provided to upload some test data to minio.
//! In a separate terminal, set up the Python virtualenv then upload some sample data:
//!
//! ```sh
//! # Create a virtualenv
//! python3 -m venv ./venv
//! # Activate the virtualenv
//! source ./venv/bin/activate
//! # Install dependencies
//! pip install scripts/requirements.txt
//! # Upload some sample data to the running minio server
//! python ./scripts/upload_sample_data.py
//! ```
//!
//! ### Compliance test suite
//!
//! Proxy functionality can be tested using the [S3 active storage compliance suite](https://github.com/stackhpc/s3-active-storage-compliance-suite).
//!
//! ### Making requests to active storage endpoints
//!
//! Request authentication is implemented using [Basic Auth](https://en.wikipedia.org/wiki/Basic_access_authentication) with the username and password consisting of your S3 Access Key ID and Secret Access Key, respectively. These credentials are then used internally to authenticate with the upstream S3 source using [standard AWS authentication methods](https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-auth-using-authorization-header.html)
//!
//! A basic Python client is provided in `scripts/client.py`.
//! First install dependencies in a Python virtual environment:
//!
//! ```sh
//! # Create a virtualenv
//! python3 -m venv ./venv
//! # Activate the virtualenv
//! source ./venv/bin/activate
//! # Install dependencies
//! pip install scripts/requirements.txt
//! ```
//!
//! Then use the client to make a request:
//! ```sh
//! venv/bin/python ./scripts/client.py sum --server http://localhost:8080 --source http://localhost:9000 --username minioadmin --password minioadmin --bucket sample-data --object data-uint32.dat --dtype uint32
//! ```
//!
//! ---
//!
//! ## Documentation
//!
//! The source code is documented using [rustdoc](https://doc.rust-lang.org/rustdoc/what-is-rustdoc.html).
//! Documentation is available on [docs.rs](https://docs.rs/reductionist/latest/reductionist/).
//! It is also possible to build the documentation locally:
//!
//! ```sh
//! cargo doc --no-deps
//! ```
//!
//! The resulting documentation is available under `target/doc`, and may be viewed in a web browser using file:///path/to/reductionist/target/doc/reductionist/index.html.

pub mod app;
pub mod array;
pub mod cli;
pub mod compression;
pub mod error;
pub mod filter_pipeline;
pub mod filters;
pub mod metrics;
pub mod models;
pub mod operation;
pub mod operations;
pub mod s3_client;
pub mod server;
#[cfg(test)]
pub mod test_utils;
pub mod tracing;
pub mod types;
pub mod validated_json;
