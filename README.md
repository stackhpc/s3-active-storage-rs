# Reductionist

This project implements simple reductions on S3 objects containing numeric binary data.
By implementing these reductions in the storage system the volume of data that needs to be
transferred to the end user is vastly reduced, leading to faster computations.

The work is funded by the
[ExCALIBUR project](https://www.metoffice.gov.uk/research/approach/collaboration/spf/excalibur)
and is done in collaboration with the
[University of Reading](http://www.reading.ac.uk/).

Documentation for the Reductionist application is hosted on [GitHub](https://stackhpc.github.io/reductionist-rs).
Documentation for the source code is available on [docs.rs](https://docs.rs/reductionist/latest/reductionist/).

This is a performant implementation of the active storage server.
The original Python functional prototype is available [here](https://github.com/stackhpc/s3-active-storage-prototype).

Note: The original S3 Active Storage project was renamed to Reductionist, to avoid confusion due to overuse of the term Active Storage.

## Features

Reductionist provides the following features:

-   HTTP(S) API with JSON request data
-   Access to data stored in S3-compatible storage
-   Basic numerical operations on multi-dimensional arrays (count, min, max, select, sum)
-   Perform calculations on a selection/slice of an array
-   Perform calculations allowing for missing data
-   Compressed data (GZip, Zlib)
-   Filtered data (byte shuffle)
-   Data with non-native byte order (endianness)
-   Server resource (CPU, memory, files) management
-   [Prometheus](https://prometheus.io/) metrics
-   Tracing with an option to send data to [Jaeger](https://www.jaegertracing.io/)
-   Ansible-based containerised deployment

## Related projects

-   [PyActiveStorage](https://github.com/NCAS-CMS/PyActiveStorage) is a Python library which performs reductions on numerical data in data sources such as netCDF4. It has support for delegating computation to Reductionist when the data is stored in an S3-compatible object store.

## Contributing

See the [contributor guide](https://stackhpc.github.io/reductionist-rs/contributing/) for information about contributing to Reductionist.

## License

This project is licensed under the Apache-2.0 License.
