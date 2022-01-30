use async_trait::async_trait;
use tokio::io;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::{Handler, Result, UnityFileGuid, UnityFileHash, UnityFileType};

#[derive(Debug, Default, Clone)]
pub struct NopHandler;

impl NopHandler {
    pub fn new() -> Self {
        Default::default()
    }
}

#[async_trait]
impl Handler for NopHandler {
    type File = tokio::io::Empty;

    async fn get(&self, _t: UnityFileType, _guid: &UnityFileGuid, _hash: &UnityFileHash) -> Result<Option<(u64, Self::File)>> { Ok(None) }

    async fn start_transaction(&mut self, _guid: UnityFileGuid, _hash: UnityFileHash) -> Result<()> { Ok(()) }

    async fn end_transaction(&mut self) -> Result<()> { Ok(()) }

    async fn cancel_transaction(&mut self) -> Result<()> { Ok(()) }

    async fn put<R: AsyncRead + Unpin + Send>(&mut self, _t: UnityFileType, size: u64, reader: R) -> Result<()> {
        io::copy(&mut reader.take(size), &mut io::sink()).await?;
        Ok(())
    }
}
