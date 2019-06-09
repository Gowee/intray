use std::io;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "I/O Error: {}", _0)]
    Io(#[fail(cause)] io::Error),
    #[fail(display = "The file token is invalid.")]
    InvalidFileToken,
    #[fail(display = "The chunk index is invalid.")]
    InvalidChunkIndex,
    #[fail(display = "The chunk has already been written up.")]
    ChunkAlreadyWritten,
    #[fail(
        display = "The file has not been finished, first unfilled chunk: {}.",
        _0
    )]
    FileNotFilledUp(usize),
    #[fail(
        display = "Data does not fit in the file or the chunk, current position: {}.",
        _0
    )]
    DataNotFitIn(usize),
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Error {
        Error::Io(error)
    }
}
