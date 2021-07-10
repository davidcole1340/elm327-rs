/// Result type used throughout the library.
pub type Result<T> = std::result::Result<T, Error>;

/// Error type used throughout the library.
#[derive(Debug)]
pub enum Error {
    Serial(tokio_serial::Error),
    Conversion,
    Clear,
    Write,
    Flush,
    Read,
    TimedOut,
    Packet(&'static str),
}
