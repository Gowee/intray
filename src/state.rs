use futures::{
    compat::{Future01CompatExt, Stream01CompatExt},
    lock::Mutex,
    Future, Stream, StreamExt,
};
use tokio::{
    fs::{remove_file, File, OpenOptions},
    io::{shutdown, write_all},
    prelude::{future::poll_fn, Async as Async01, Future as Future01, Stream as Stream01},
    timer::{delay_queue::Key as DQKey, DelayQueue, Interval},
};
use uuid::Uuid as UUID;

use std::{
    cmp::min,
    collections::HashMap,
    ffi::{OsStr, OsString},
    io::{self, SeekFrom},
    ops::Drop,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use crate::{bitmap::BitMap, error::Error, opt::OPT};

static EXPIRATION_INTERVAL: Duration = Duration::from_secs(30);

macro_rules! try_finally {
    ($expr:expr, $finally:expr) => {
        match $expr {
            Ok(val) => val,
            Err(err) => {
                $finally;
                return Err(From::from(err));
            }
        }
    };
}

#[allow(unused)]
async fn create_file(
    file_name: impl AsRef<OsStr>,
    ext_hint: Option<impl AsRef<OsStr>>,
) -> io::Result<(File, PathBuf)> {
    // TODO: Create temporary file for pending task and then rename it
    let path = PathBuf::from(file_name.as_ref());
    let stem = path
        .file_stem()
        .unwrap_or_else(|| OsStr::new("UnnamedFile"));
    let ext = path
        .extension()
        .or_else(|| ext_hint.as_ref().map(|i| i.as_ref()));

    let mut count = 0;
    loop {
        let file_name = if count == 0 {
            let mut s = OsString::from(stem);
            if let Some(ext) = ext {
                s.push(".");
                s.push(ext);
            }
            s
        } else {
            let mut s = OsString::from(stem);
            s.push("_");
            s.push(count.to_string());
            if let Some(ext) = ext {
                s.push(".");
                s.push(ext);
            }
            s
        };
        let path = OPT.dir().join(file_name);
        let result = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.clone())
            .compat()
            .await;
        match result {
            Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e),
            Ok(file) => return Ok((file, path)),
        }
        count += 1;
    }
}

#[derive(Debug)]
struct PendingFile {
    token: UUID,
    name: String,
    size: usize,
    path: PathBuf,
    handle: Option<File>,
    chunk_size: usize,
    /// Chunks bitmap
    chunks: Vec<u8>,
    /// The number of filled chunks
    filled: usize,
}

impl PendingFile {
    pub fn new(
        token: UUID,
        name: String,
        size: usize,
        path: PathBuf,
        handle: File,
        chunk_size: usize,
    ) -> Self {
        //let file = await File
        let handle = Some(handle);
        let chunks = vec![];
        let filled = 0;
        PendingFile {
            token,
            name,
            size,
            path,
            handle,
            chunk_size,
            chunks,
            filled,
        }
    }

    pub fn chunk_number(&self) -> usize {
        //(self.size as f64 / self.chunk_size as f64).ceil() as usize
        // https://stackoverflow.com/questions/2745074/fast-ceiling-of-an-integer-division-in-c-c
        if self.size == 0 {
            0
        } else {
            1 + ((self.size - 1) / self.chunk_size)
        }
    }

    pub async fn write_chunk(
        &mut self,
        chunk_index: usize,
        mut data: impl Stream<Item = io::Result<impl AsRef<[u8]>>> + Unpin,
    ) -> Result<usize, Error> {
        if self.chunks.get_bit(chunk_index) {
            return Err(Error::ChunkAlreadyWritten);
        }
        let pos = self.chunk_size * chunk_index;
        if pos > self.size {
            return Err(Error::InvalidChunkIndex);
        }
        let file = self.handle.take().unwrap();
        let size = min(self.chunk_size, self.size - pos);
        let mut file = file.seek(SeekFrom::Start(pos as u64)).compat().await?.0;
        let mut count = 0;
        while let Some(bytes) = data.next().await {
            let bytes = try_finally!(bytes, self.handle = Some(file));
            count += bytes.as_ref().len();
            if count > size {
                self.handle = Some(file);
                return Err(Error::DataNotFitIn(pos + count));
            }
            // TODO: it seems that write_all flushes by design, which may result in unbearable
            // performance penalty
            file = write_all(file, bytes).compat().await?.0;
        }
        self.handle = Some(file);
        if count != size {
            return Err(Error::DataNotFitIn(pos + count));
        }
        self.chunks.set_bit(chunk_index);
        self.filled += 1;
        Ok(size)
    }

    pub async fn finish(&mut self) -> Result<(), Error> {
        // avoid async fn here to minimize possile contention (but is it really necessary?)
        debug_assert!(self.filled <= self.chunk_number());
        if self.filled < self.chunk_number() {
            return Err(Error::FileNotFilledUp(self.chunks.first_unset()));
        }
        let mut file = self.handle.take().expect("Take file out");
        poll_fn(|| file.poll_sync_data())
            .map_err(|e| Error::from(e))
            .compat()
            .await?;
        // self.handle = // Do not give back. O.W. the file will be removed when `self.drop`.
        let _ = Some(shutdown(file).map_err(|e| Error::from(e)).compat().await?);
        info!("Uploaded file: {:?}", &self.path);
        Ok(())
        // // TODO: move the temporary file (not implemented yet) to the final dest
        // Ok(poll_fn(move || file.poll_sync_data(); file)
        //     .and_then(|_| shutdown(file))
        //     .map(|_| {
        //         //info!("Uploaded file: {:?}", self.path.clone());
        //         ()
        //     })
        //     .compat())
    }

    pub fn cancel(&mut self) -> impl Future<Output = io::Result<()>> {
        // take the file out and drop it,
        // then remove the file
        let path = self.path.clone();
        shutdown(self.handle.take().expect("Take file out"))
            .then(|_| remove_file(path)) // TODO: map to chain error?
            .compat()
    }
}

// TODO: AsyncWrite::shutdown-list method for PendingFile
impl Drop for PendingFile {
    fn drop(&mut self) {
        use std::fs::remove_file;
        // synchronously remove the file if it is not taked by `cancel`
        // TODO: Is it really dropped & closed here?
        if let Some(_) = self.handle.take() {
            trace!(
                "Synchronously remove the file: {}, result: {:?}",
                self.path.to_str().unwrap_or("INVALID_ENCODING_IN_PATH"),
                remove_file(&self.path)
            );
        }
    }
}

struct FileQueue {
    pending_files: HashMap<UUID, (Arc<Mutex<PendingFile>>, Option<DQKey>)>,
    expirations: DelayQueue<UUID>,
}

impl Default for FileQueue {
    fn default() -> Self {
        FileQueue {
            pending_files: HashMap::default(),
            expirations: DelayQueue::with_capacity(0),
        }
    }
}

impl FileQueue {
    pub async fn keep_expiring(this: Arc<Mutex<FileQueue>>) {
        debug!(
            "Pending files expiration task starts with interval {:?}",
            EXPIRATION_INTERVAL
        );
        let mut interval = Interval::new_interval(EXPIRATION_INTERVAL).compat();
        while let Some(Ok(instant)) = interval.next().await {
            let expired = FileQueue::expire(&this).await;
            debug!("{} pending files expired at {:?}", expired, instant);
        }
        error!("Pending files expiration task terminates unexpectedly!");
    }

    async fn expire(this: &Arc<Mutex<FileQueue>>) -> usize {
        let mut expired = vec![];
        {
            let mut file_queue = this.lock().await;
            while let Some(entry) = match file_queue.expirations.poll() {
                Ok(Async01::Ready(t)) => t,
                // according to the doc of DelayQueue,
                // NotReady indicates that there are some unexpired
                Ok(Async01::NotReady) => None,
                Err(e) => {
                    warn!("Error when polling expirations: {}", e);
                    None
                }
            } {
                trace!("File expired: {}", entry.get_ref().to_hyphenated());
                if let Some(file) = file_queue.pending_files.remove(entry.get_ref()) {
                    expired.push(file.0);
                } else {
                    unreachable!(
                        "File not found when expiring, UUID: {}",
                        entry.get_ref().to_hyphenated()
                    );
                }
            }
            if file_queue.pending_files.len() > 0 {
                debug!("Pending files: {}.", file_queue.pending_files.len());
            }
            // the lock to file_queue gets released hereinafter
        }
        let count = expired.len();
        for file in expired.into_iter() {
            file.try_lock()
                .expect("Expired file not held elsewhere")
                .cancel()
                .await
                .unwrap_or_else(|e| {
                    error!("Error when remove a stale file: {}", e);
                });
        }
        count
    }

    pub fn add_file(
        &mut self,
        name: String,
        size: usize,
        path: PathBuf,
        handle: File,
        chunk_size: usize,
    ) -> UUID {
        let token = UUID::new_v4();
        let delay = self.expirations.insert(token, EXPIRATION_INTERVAL);
        self.pending_files.insert(
            token,
            (
                Arc::new(Mutex::new(PendingFile::new(
                    token, name, size, path, handle, chunk_size,
                ))),
                Some(delay),
            ),
        );
        token
    }

    pub fn acquire_file(&mut self, token: UUID) -> Result<Arc<Mutex<PendingFile>>, Error> {
        let (file, dqkey) = self
            .pending_files
            .get_mut(&token)
            .ok_or(Error::InvalidFileToken)?;
        // the contention won't make dqkey invalid
        if let Some(dqkey) = dqkey.take() {
            // if no others have disabled the expiration
            self.expirations.remove(&dqkey);
            trace!(
                "File {} acquired with expiration disabled.",
                token.to_hyphenated()
            );
        } else {
            trace!(
                "File {} acquired, expiration has already been disabled.",
                token.to_hyphenated()
            );
        }
        Ok(file.clone())
    }

    pub fn release_file(&mut self, token: UUID) -> Result<bool, Error> {
        // TODO: doc this function properly
        // note: the file won't get expired
        // assuming that Arc<Mutex<PendingFile>> won't be cloned during the execution of the function
        if let Some((file, dqkey)) = self.pending_files.get_mut(&token) {
            debug_assert!(dqkey.is_none());
            // assuming that all unused references are dropped before
            let ref_count = Arc::strong_count(file);
            if ref_count == 1 {
                *dqkey = Some(self.expirations.insert(token, EXPIRATION_INTERVAL));
                trace!("File {} released.", token.to_hyphenated());
                Ok(true)
            } else {
                trace!(
                    "File {} not released with {} references holden.",
                    token.to_hyphenated(),
                    ref_count
                );
                Ok(false)
            }
        } else {
            Err(Error::InvalidFileToken)
        }
    }

    /// Discard a file by removing it in the `pending_files` list
    ///
    /// Be sure to acquire_file before calling this.
    pub fn discard(&mut self, token: UUID) -> Result<(), Error> {
        let (_file, _dqkey) = self
            .pending_files
            .remove(&token)
            .ok_or(Error::InvalidFileToken)?;
        assert!(_dqkey.is_none());
        Ok(())
    }
}

#[derive(Default)]
pub struct State {
    file_queue: Arc<Mutex<FileQueue>>,
}

impl State {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn expire(&self) -> impl Future<Output = ()> {
        FileQueue::keep_expiring(self.file_queue.clone())
    }

    pub async fn start_upload(
        &self,
        name: String,
        size: usize,
        chunk_size: usize,
    ) -> io::Result<UUID> {
        let (file, path) = create_file(&name, Option::<String>::None).await?;
        // create_file is a async job which may take much time, so here to acquire the lock only after that
        Ok(self
            .file_queue
            .lock()
            .await
            .add_file(name, size, path, file, chunk_size))
    }

    pub async fn put_chunk(
        &self,
        file_token: UUID,
        chunk_index: usize,
        data: impl Stream<Item = io::Result<impl AsRef<[u8]>>> + Unpin,
    ) -> Result<usize, Error> {
        let result = {
            // drop file_queue lock immediately
            let _file = self.file_queue.lock().await.acquire_file(file_token)?;
            let mut file = _file.lock().await;
            // TODO: Does the lock/unlock sequence work as expected?
            match file.write_chunk(chunk_index, data).await {
                Err(Error::Io(e)) => {
                    // already an IO error here, so discarding the new one
                    let _ = file.cancel().await;
                    Err(Error::Io(e))
                }
                other => other,
            }
        };
        let mut file_queue = self.file_queue.lock().await;
        if let Err(Error::Io(ref _e)) = result {
            // The intenal file has been taken away and dropped. The pending file must be canceled.
            file_queue.discard(file_token)?;
        } else {
            // before calling release_file, the Arc<Mutex<PendingFile>> should be dropped
            file_queue.release_file(file_token)?;
        }
        result
    }

    pub async fn finish_upload(&self, file_token: UUID) -> Result<(), Error> {
        let file = {
            let mut file_queue = self.file_queue.lock().await;
            let file = file_queue.acquire_file(file_token)?;
            file_queue.discard(file_token)?;
            file
        };

        let mut locked_file = file.lock().await;
        locked_file.finish().await?;
        // make sure the file is finished
        Ok(())
    }

    // TODO: cancel upload

    pub async fn put_full(
        &self,
        name: String,
        size: Option<usize>,
        mut data: impl Stream<Item = io::Result<impl AsRef<[u8]>>> + Unpin,
    ) -> Result<usize, Error> {
        let (mut file, path) = create_file(name, Option::<String>::None).await?;
        let mut count = 0;
        while let Some(bytes) = data.next().await {
            let bytes = try_finally!(bytes, {
                let _ = remove_file(path).compat().await;
            });
            count += bytes.as_ref().len();
            if let Some(size) = size {
                if count > size {
                    // TODO: shutdown before remove?
                    let _ = remove_file(path).compat().await;
                    return Err(Error::DataNotFitIn(count));
                }
            }
            // TODO: it seems that write_all flushes by design, which may result in unbearable
            // performance penalty; even though there is no flush according to the code (?)
            file = write_all(file, bytes).compat().await?.0;
        }
        if let Some(size) = size {
            if count != size {
                let _ = remove_file(path).compat().await;
                return Err(Error::FileNotFilledUp(count));
            }
        }
        poll_fn(|| file.poll_sync_data())
            .compat()
            .await
            .map_err(|e| Error::from(e))?;
        let result = shutdown(file)
            .map(|_| count)
            .map_err(|e| e.into())
            .compat()
            .await;
        info!("Uploaded file: {:?}", path);
        result
    }
}
