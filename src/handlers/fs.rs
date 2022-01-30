use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, BufReader, BufWriter};
use tokio::sync::Mutex;

use crate::{Error, Handler, Result, UnityFileGuid, UnityFileHash, UnityFileType};
use crate::handlers::Transaction;

#[derive(Debug)]
pub struct TempFile {
    path: Option<PathBuf>,
    writer: Option<BufWriter<File>>,
}

impl TempFile {
    pub async fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path_buf = PathBuf::from(path.as_ref());
        if let Some(parent) = path_buf.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let file = OpenOptions::new().write(true).create(true).truncate(true).open(path).await?;
        Ok(Self {
            path: Some(path_buf),
            writer: Some(BufWriter::new(file)),
        })
    }

    pub async fn move_to(mut self, to: impl AsRef<Path>) -> std::io::Result<()> {
        self.writer = None;
        if let Some(parent) = to.as_ref().parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::rename(self.path.take().unwrap(), to).await?;
        Ok(())
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        self.writer = None;
        if let Some(path) = self.path.take() {
            tokio::spawn(async move {
                tokio::fs::remove_file(&path).await.unwrap_or_else(|e| {
                    println!("remove temp file {} error {:?}", path.to_string_lossy(), e);
                });
            });
        }
    }
}

impl AsyncWrite for TempFile {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::result::Result<usize, std::io::Error>> {
        Pin::new(self.writer.as_mut().unwrap()).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(self.writer.as_mut().unwrap()).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(self.writer.as_mut().unwrap()).poll_shutdown(cx)
    }
}

#[derive(Debug)]
pub struct FileSystemHandler {
    /// Max file size for put
    /// 0 for no limit
    max_file_size: usize,
    transaction: Mutex<Option<Transaction<TempFile>>>,
    base_path: PathBuf,
    temp_path: PathBuf,
}

impl FileSystemHandler {
    pub fn new(base_path: PathBuf, temp_path: PathBuf) -> Self {
        Self {
            max_file_size: 0,
            transaction: Default::default(),
            base_path,
            temp_path,
        }
    }

    pub fn max_file_size(&self) -> usize {
        self.max_file_size
    }

    pub fn set_max_file_size(&mut self, max_file_size: usize) {
        self.max_file_size = max_file_size;
    }

    pub fn calc_filename(t: UnityFileType, guid: &UnityFileGuid, hash: &UnityFileHash) -> String {
        format!("{}-{}.{}", guid.to_hex_string(), hash.to_hex_string(), t.to_ext())
    }

    pub fn calc_filepath(&self, t: UnityFileType, guid: &UnityFileGuid, hash: &UnityFileHash) -> PathBuf {
        let filename = Self::calc_filename(t, guid, hash);
        let hash_dir = filename.get(0..2).expect("get file hash directory name failed");
        self.base_path.join(hash_dir).join(filename)
    }

    pub async fn new_tmp_file(&self) -> std::io::Result<TempFile> {
        let path = self.temp_path.join(uuid::Uuid::new_v4().to_string());
        TempFile::open(path).await
    }
}

impl Clone for FileSystemHandler {
    fn clone(&self) -> Self {
        Self {
            max_file_size: self.max_file_size.clone(),
            transaction: Default::default(),
            base_path: self.base_path.clone(),
            temp_path: self.temp_path.clone(),
        }
    }
}

#[async_trait]
impl Handler for FileSystemHandler {
    type File = BufReader<File>;

    async fn get(&self, t: UnityFileType, guid: &UnityFileGuid, hash: &UnityFileHash) -> Result<Option<(u64, Self::File)>> {
        let path = self.calc_filepath(t, guid, hash);
        let f = match File::open(&path).await {
            Ok(f) => f,
            Err(e) => return match e.kind() {
                ErrorKind::NotFound => Ok(None),
                e => Err(Error::IoError(std::io::Error::from(e))),
            },
        };
        let meta = f.metadata().await?;
        if !meta.is_file() {
            return Err(Error::HandlerError(format!("Path {} is not a file", path.to_string_lossy())));
        }
        let len = meta.len();
        Ok(Some((len, BufReader::new(f))))
    }

    async fn start_transaction(&mut self, guid: UnityFileGuid, hash: UnityFileHash) -> Result<()> {
        *self.transaction.lock().await = Some(Transaction::new(guid, hash));
        Ok(())
    }

    async fn end_transaction(&mut self) -> Result<()> {
        let transaction = {
            self.transaction.lock().await.take()
        };
        if let Some(mut transaction) = transaction {
            for (t, file) in transaction.files.take_all().into_iter() {
                let target_path = self.calc_filepath(t, &transaction.guid, &transaction.hash);
                file.move_to(target_path).await?;
            }
        }
        Ok(())
    }

    async fn cancel_transaction(&mut self) -> Result<()> {
        *self.transaction.lock().await = None;
        Ok(())
    }

    async fn put<R: AsyncRead + Unpin + Send>(&mut self, t: UnityFileType, size: u64, reader: R) -> Result<()> {
        if self.max_file_size != 0 {
            if size > self.max_file_size as u64 {
                return Err(Error::FileTooLarge {
                    max_size: self.max_file_size,
                    size: size as usize,
                });
            }
        }
        let mut temp_file = self.new_tmp_file().await?;
        let n = tokio::io::copy(&mut reader.take(size), &mut temp_file).await?;
        if n != size {
            return Err(Error::IoError(std::io::Error::from(std::io::ErrorKind::UnexpectedEof)));
        }
        if let Some(transaction) = &mut *self.transaction.lock().await {
            transaction.files.set(t, temp_file);
            Ok(())
        } else {
            Err(Error::NotInTransaction)
        }
    }
}