use futures::compat::{Future01CompatExt, Stream01CompatExt};
use futures::{lock::Mutex, Future, Stream, StreamExt};
use tokio::fs::{remove_file, File, OpenOptions};
use tokio::io::write_all;
use tokio::prelude::{Async as Async01, Stream as Stream01};
use tokio::timer::{delay_queue::Key as DQKey, DelayQueue, Interval};
use uuid::Uuid as UUID;

use std::cmp::min;
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io;
use std::io::SeekFrom;
use std::ops::Drop;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
//use std::str::FromStr;

use crate::bitmap::BitMap;
use crate::opt::OPT;

static EXPIRATION_INTERVAL: Duration = Duration::from_secs(15);

#[allow(unused)]
async fn create_file(
    file_name: impl AsRef<OsStr>,
    ext_hint: Option<impl AsRef<OsStr>>,
) -> io::Result<(File, PathBuf)> {
    // TODO: Create temporary file for pending task and then rename it
    let path = PathBuf::from(file_name.as_ref());
    let stem = path.file_stem().unwrap_or(OsStr::new("UnnamedFile"));
    let ext = path.extension().or(ext_hint.as_ref().map(|i| i.as_ref()));

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
}

impl PendingFile {
    fn new(
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
        PendingFile {
            token,
            name,
            size,
            path,
            handle,
            chunk_size,
            chunks,
        }
    }

    fn cancel(&mut self) -> impl Future<Output = io::Result<()>> {
        // take the file out and drop it
        let _ = self.handle.take().expect("Take file out");
        // remove the file
        remove_file(self.path.clone()).compat()
    }

    async fn write_chunk(
        &mut self,
        chunk_number: usize,
        mut data: impl Stream<Item = io::Result<impl AsRef<[u8]>>> + Unpin,
    ) -> io::Result<usize> {
        // TODO: error out when the chunk has been filled
        let pos = self.chunk_size * chunk_number;
        if pos > self.size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid chunk number",
            ));
        }
        let file = self.handle.take().unwrap();
        // FIXME: Result? does not give file back
        let size = min(self.chunk_size, self.size - pos);
        let mut file = file.seek(SeekFrom::Start(pos as u64)).compat().await?.0;
        let mut count = 0;
        while let Some(bytes) = data.next().await {
            let bytes = bytes?;
            count += bytes.as_ref().len();
            if count > size {
                self.handle = Some(file);
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Data length too long",
                ));
            }
            // TODO: it seems that write_all flushes by design, which may result in unbearable
            // performance penalty
            file = write_all(file, bytes).compat().await?.0;
        }
        self.handle = Some(file);
        if count != size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Data length too short",
            ));
        }
        self.chunks.set_bit(chunk_number);
        Ok(size)
    }
}

impl Drop for PendingFile {
    fn drop(&mut self) {
        use std::fs::remove_file;
        // synchronously remove the file if it is not taked by `cancel`
        if let Some(_) = self.handle {
            debug!(
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

impl Drop for FileQueue {
    fn drop(&mut self) {
        println!("file_queue dropped!");
        info!("file_queue dropped!");
        unreachable!();
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
                    // FIXME: triggered during test
                    /*
                                            Finished dev [unoptimized + debuginfo] target(s) in 8.65s
                         Running `target/debug/intray`
                    [2019-06-05T15:37:57Z INFO  intray] Running at [::]:8080...
                    [2019-06-05T15:37:57Z DEBUG intray::state] Pending files expiration task starts with interval 15s
                    [2019-06-05T15:38:12Z DEBUG intray::state] 0 pending files expired at Instant { tv_sec: 45530, tv_nsec: 289342300 }
                    [2019-06-05T15:38:21Z DEBUG intray::api] Upload starts with UUID: 1cc99003-949e-4530-81ee-a8418bbca8a3
                    [2019-06-05T15:38:21Z INFO  tide_log] POST /upload/start 200 1ms
                    [2019-06-05T15:38:27Z DEBUG intray::state] 0 pending files expired at Instant { tv_sec: 45545, tv_nsec: 289342300 }
                    [2019-06-05T15:38:42Z DEBUG intray::state] 1 pending files expired at Instant { tv_sec: 45560, tv_nsec: 289342300 }
                    [2019-06-05T15:38:47Z DEBUG intray::api] Upload starts with UUID: 65ee2a12-34b3-4fa0-895b-3698e01d69dc
                    [2019-06-05T15:38:47Z INFO  tide_log] POST /upload/start 200 0ms
                    [2019-06-05T15:38:57Z DEBUG intray::state] 0 pending files expired at Instant { tv_sec: 45575, tv_nsec: 289342300 }
                    [2019-06-05T15:38:58Z INFO  tide_log] POST /upload/65ee2a12-34b3-4fa0-895b-3698e01d69dc/0 200 0ms
                    [2019-06-05T15:39:07Z INFO  tide_log] POST /upload/65ee2a12-34b3-4fa0-895b-3698e01d69dc/1 200 0ms
                    [2019-06-05T15:39:12Z DEBUG intray::state] 0 pending files expired at Instant { tv_sec: 45590, tv_nsec: 289342300 }
                    [2019-06-05T15:39:19Z INFO  tide_log] POST /upload/65ee2a12-34b3-4fa0-895b-3698e01d69dc/2 200 0ms
                    [2019-06-05T15:39:26Z INFO  tide_log] POST /upload/65ee2a12-34b3-4fa0-895b-3698e01d69dc/3 500 0ms
                    [2019-06-05T15:39:27Z ERROR intray::state] File not found when expiring, UUID: 65ee2a12-34b3-4fa0-895b-3698e01d69dc
                    thread 'tokio-runtime-worker-3' panicked at 'Take file out', src/libcore/option.rs:1036:5 // this got FIXED
                    note: Run with `RUST_BACKTRACE=1` environment variable to display a backtrace.
                    [2019-06-05T15:39:40Z DEBUG intray::api] Upload starts with UUID: 8ec970b4-2090-4e18-b5b8-dd53c66e6e9e
                    [2019-06-05T15:39:40Z INFO  tide_log] POST /upload/start 200 0ms
                    */
                    unreachable!(
                        "File not found when expiring, UUID: {}",
                        entry.get_ref().to_hyphenated()
                    );
                }
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

    pub async fn add_file(
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

    pub fn acquire_file(&mut self, token: UUID) -> Option<Arc<Mutex<PendingFile>>> {
        let (file, dqkey) = self.pending_files.get_mut(&token)?;
        // the contention won't make dqkey invalid
        if let Some(dqkey) = dqkey.take() {
            // if no others have disabled the expiration
            self.expirations.remove(&dqkey);
        }
        Some(file.clone())
        //self.file_queue.pending_files.
        // get arc mutex pending file
        // disable expirations
        // return pending file
        //self.file_queue.expirations.remove()
        // Note: Error when not found (possibly invalid token or expired/expiring)
    }

    pub fn release_file(&mut self, token: UUID) -> bool {
        // TODO: doc this function properly
        // assuming that Arc<Mutex<PendingFile>> won't be cloned during the execution of the function
        let (file, dqkey) = self
            .pending_files
            .get(&token)
            .expect("The token should always be valid when releasing a file");
        assert!(dqkey.is_none());
        // assuming that all unused references are dropped before
        if Arc::strong_count(file) == 1 {
            self.expirations.insert(token, EXPIRATION_INTERVAL);
            true
        } else {
            false
        }
    }
}

#[derive(Default)]
pub struct State {
    file_queue: Arc<Mutex<FileQueue>>, //expirations:
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
        let mut file_queue = self.file_queue.lock().await;
        Ok(file_queue
            .add_file(name, size, path, file, chunk_size)
            .await)
    }

    pub async fn put_chunk(
        &self,
        file_token: UUID,
        chunk_number: usize,
        data: impl Stream<Item = io::Result<impl AsRef<[u8]>>> + Unpin,
    ) -> io::Result<usize> {
        // drop file_queue lock immediately
        let result = {
            let _file = self.file_queue.lock().await.acquire_file(file_token).unwrap(/*FIXME: pass error out*/);
            let mut file = _file.lock().await;
            // TODO: Does the lock/unlock sequence work as expected?
            file.write_chunk(chunk_number, data).await
        };
        // before calling release_file, the Arc<Mutex<PendingFile>> should be dropped
        self.file_queue.lock().await.release_file(file_token);
        result
    }

    pub fn finish_upload(&self) {
        
    }

    // cancel upload
}
