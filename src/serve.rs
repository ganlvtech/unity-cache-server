use std::io::ErrorKind;

use async_trait::async_trait;
use tokio::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{Error, HexString, Result, u32_to_be_hex_string, UnityFileGuid, UnityFileHash, UnityFileType};

#[async_trait]
pub trait Handler: Sync {
    type File: AsyncRead + Unpin;

    /// verify version: only 254 allowed
    async fn version(&self, version: u32) -> Result<u32> {
        if version == 254 {
            Ok(version)
        } else {
            Err(Error::WrongVersion(version))
        }
    }

    /// get a file from server
    async fn get(&self, t: UnityFileType, guid: &UnityFileGuid, hash: &UnityFileHash) -> Result<Option<(u64, Self::File)>>;

    /// start a transaction
    async fn start_transaction(&mut self, guid: UnityFileGuid, hash: UnityFileHash) -> Result<()>;

    /// commit the transaction: save the files
    async fn end_transaction(&mut self) -> Result<()>;

    /// cancel the transaction: remove the files
    /// do nothing if no transaction
    async fn cancel_transaction(&mut self) -> Result<()>;

    /// put file: write file to temporary directory, calculate file hash.
    async fn put<R: AsyncRead + Unpin + Send>(&mut self, t: UnityFileType, size: u64, reader: R) -> Result<()>;
}

pub async fn handle<R, W, H>(reader: &mut R, writer: &mut W, mut handler: H) -> Result<()>
    where
        R: AsyncRead + Unpin + Send,
        W: AsyncWrite + Unpin + ?Sized,
        H: Handler,
{
    let version = read_version(&mut *reader).await?;
    let response_version = handler.version(version).await?;
    let _ = writer.write(u32_to_be_hex_string(response_version).as_ref()).await?;
    writer.flush().await?;

    loop {
        match reader.read_u8().await {
            Err(e) => {
                if e.kind() != ErrorKind::UnexpectedEof {
                    return Err(Error::IoError(e));
                }
                break;
            }
            Ok(b) => match b {
                b'g' => {
                    let b2 = reader.read_u8().await?;
                    match UnityFileType::try_from_ext_char(b2) {
                        Ok(t) => {
                            let guid: UnityFileGuid = read_hex_string(&mut *reader).await?;
                            let hash: UnityFileHash = read_hex_string(&mut *reader).await?;
                            println!("get {} {} {}", t.to_ext(), guid.to_hex_string(), hash.to_hex_string());
                            match handler.get(t, &guid, &hash).await? {
                                None => {
                                    let _ = writer.write_u8(b'-').await?;
                                    let _ = writer.write_u8(t.to_ext_char()).await?;
                                    let _ = writer.write(guid.as_ref()).await?;
                                    let _ = writer.write(hash.as_ref()).await?;
                                    writer.flush().await?;
                                }
                                Some((size, mut r)) => {
                                    let _ = writer.write_u8(b'+').await?;
                                    let _ = writer.write_u8(t.to_ext_char()).await?;
                                    let _ = write_size_string(writer, size).await?;
                                    let _ = writer.write(guid.as_ref()).await?;
                                    let _ = writer.write(hash.as_ref()).await?;
                                    let _ = io::copy(&mut r, writer).await?;
                                    writer.flush().await?;
                                }
                            }
                        }
                        Err(_) => {
                            return Err(Error::UnknownFileTypeByte(b2));
                        }
                    }
                }
                b't' => {
                    match reader.read_u8().await? {
                        b's' => {
                            let guid: UnityFileGuid = read_hex_string(&mut *reader).await?;
                            let hash: UnityFileHash = read_hex_string(&mut *reader).await?;
                            println!("start_transaction {} {}", guid.to_hex_string(), hash.to_hex_string());
                            handler.start_transaction(guid, hash).await?;
                        }
                        b'e' => {
                            println!("end_transaction");
                            handler.end_transaction().await?;
                        }
                        b => {
                            return Err(Error::UnknownTransactionCommand(b));
                        }
                    }
                }
                b'p' => {
                    match reader.read_u8().await? {
                        b @ (b'a' | b'i' | b'r') => {
                            let t = UnityFileType::try_from_ext_char(b).expect("parse unity file type error");
                            let size = read_size_string(&mut *reader).await?;
                            println!("put {} {}", t.to_ext(), size);
                            handler.put(t, size, &mut *reader).await?;
                        }
                        b => {
                            return Err(Error::UnknownPushCommand(b));
                        }
                    }
                }
                b'q' => {
                    break;
                }
                b => {
                    return Err(Error::UnknownCommand(b));
                }
            }
        }
    }
    Ok(())
}

pub async fn read_version<R>(reader: &mut R) -> Result<u32>
    where
        R: AsyncRead + Unpin
{
    let mut buf = vec![0u8; 8];
    let n = reader.read(&mut buf).await?;
    if n == 0 {
        return Err(Error::ReadVersionError);
    }

    let n = if n == 1 {
        let n2 = reader.read(&mut buf[n..8]).await?;
        if n2 == 0 {
            return Err(Error::ReadVersionError);
        }
        if n + n2 > 8 {
            unreachable!("The buffer is 8 bytes len. So it cannot read more than 8 bytes");
        }
        n + n2
    } else {
        n
    };

    Ok(u32::from_str_radix(std::str::from_utf8(&buf[0..n])?, 16)?)
}

pub async fn read_hex_string<R, const N: usize>(reader: &mut R) -> Result<HexString<N>>
    where
        R: AsyncRead + Unpin
{
    let mut s = HexString::new();
    reader.read_exact(&mut s.0).await?;
    Ok(s)
}

pub async fn read_size_string<R>(reader: &mut R) -> Result<u64>
    where
        R: AsyncRead + Unpin
{
    const U64_STRING_LENGTH: usize = 16;

    let mut s = String::new();
    let n = reader.take(U64_STRING_LENGTH as u64).read_to_string(&mut s).await?;
    if n != U64_STRING_LENGTH {
        return Err(std::io::Error::from(ErrorKind::UnexpectedEof).into());
    }
    Ok(u64::from_str_radix(&s, 16)?)
}

pub async fn write_size_string<W>(writer: &mut W, v: u64) -> Result<()>
    where
        W: AsyncWrite + Unpin + ?Sized
{
    writer.write(format!("{:016x}", v).as_bytes()).await?;
    Ok(())
}

