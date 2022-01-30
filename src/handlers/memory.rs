use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::io;
use tokio::io::{AsyncRead, AsyncReadExt, BufReader};
use tokio::sync::Mutex;

use crate::{Error, Handler, Result, UnityFileGuid, UnityFileHash, UnityFileType};
use crate::handlers::Transaction;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct CacheKey {
    guid: UnityFileGuid,
    hash: UnityFileHash,
    r#type: UnityFileType,
}

impl CacheKey {
    pub fn new(guid: UnityFileGuid, hash: UnityFileHash, r#type: UnityFileType) -> Self {
        Self {
            guid,
            hash,
            r#type,
        }
    }
}

#[derive(Debug, Default)]
pub struct MemoryHandler {
    /// Max file size for put
    /// 0 for no limit
    max_file_size: usize,
    transaction: Mutex<Option<Transaction<Vec<u8>>>>,
    database: Arc<Mutex<HashMap<CacheKey, Bytes>>>,
}

impl Clone for MemoryHandler {
    fn clone(&self) -> Self {
        Self {
            max_file_size: self.max_file_size,
            transaction: Default::default(),
            database: self.database.clone(),
        }
    }
}

impl MemoryHandler {
    pub fn new() -> Self {
        Default::default()
    }
    pub fn max_file_size(&self) -> usize {
        self.max_file_size
    }
    pub fn set_max_file_size(&mut self, max_file_size: usize) {
        self.max_file_size = max_file_size;
    }
    pub async fn file_count(&self) -> usize {
        self.database.lock().await.len()
    }
}

#[async_trait]
impl Handler for MemoryHandler {
    type File = BufReader<Cursor<Bytes>>;

    async fn get(&self, t: UnityFileType, guid: &UnityFileGuid, hash: &UnityFileHash) -> Result<Option<(u64, Self::File)>> {
        match self.database.lock().await.get(&CacheKey::new(*guid, *hash, t)) {
            None => Ok(None),
            Some(v) => Ok(Some((v.len() as u64, BufReader::new(Cursor::new(v.clone()))))),
        }
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
            let mut guard = self.database.lock().await;
            for (t, file) in transaction.files.take_all() {
                guard.insert(CacheKey::new(transaction.guid.clone(), transaction.hash.clone(), t), Bytes::from(file));
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
        let mut buf = Vec::with_capacity(size as usize);
        let n = io::copy(&mut reader.take(size), &mut buf).await?;
        if n != size {
            return Err(Error::IoError(std::io::Error::from(std::io::ErrorKind::UnexpectedEof)));
        }
        if let Some(transaction) = &mut *self.transaction.lock().await {
            transaction.files.set(t, buf);
            Ok(())
        } else {
            Err(Error::NotInTransaction)
        }
    }
}