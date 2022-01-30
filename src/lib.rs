use std::fmt::{Debug, Display, Formatter, Write};
use std::num::ParseIntError;
use std::str::Utf8Error;

pub use serve::{handle, Handler};

mod serve;
pub mod handlers;

// region Error

#[derive(Debug)]
pub enum Error {
    ReadVersionError,
    WrongVersion(u32),
    UnknownFileTypeByte(u8),
    UnknownFileTypeExt(String),
    UnknownTransactionCommand(u8),
    UnknownPushCommand(u8),
    UnknownCommand(u8),
    FileTooLarge {
        max_size: usize,
        size: usize,
    },
    NotInTransaction,
    Utf8Error(Utf8Error),
    ParseIntError(ParseIntError),
    IoError(std::io::Error),
    DecodeHexError(DecodeHexError),
    HandlerError(String),
    UnknownError,
}

impl From<ParseIntError> for Error {
    fn from(e: ParseIntError) -> Self {
        Error::ParseIntError(e)
    }
}

impl From<Utf8Error> for Error {
    fn from(e: Utf8Error) -> Self {
        Error::Utf8Error(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::IoError(e)
    }
}

impl From<DecodeHexError> for Error {
    fn from(e: DecodeHexError) -> Self {
        Error::DecodeHexError(e)
    }
}

impl From<()> for Error {
    fn from(_: ()) -> Self {
        Error::UnknownError
    }
}

pub type Result<T> = std::result::Result<T, Error>;

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ReadVersionError => write!(f, "read version error"),
            Error::WrongVersion(e) => write!(f, "wrong version: {:?}", e),
            Error::UnknownFileTypeByte(e) => write!(f, "unknown file type byte: {:?}", e),
            Error::UnknownFileTypeExt(e) => write!(f, "unknown file type ext: {:?}", e),
            Error::UnknownTransactionCommand(e) => write!(f, "unknown transaction command: {:?}", e),
            Error::UnknownPushCommand(e) => write!(f, "unknown push command: {:?}", e),
            Error::UnknownCommand(e) => write!(f, "unknown command: {:?}", e),
            Error::FileTooLarge { max_size, size } => write!(f, "file too large: {}. max size: {}. size", size, max_size),
            Error::NotInTransaction => write!(f, "not in transaction"),
            Error::Utf8Error(e) => write!(f, "utf8 error: {:?}", e),
            Error::ParseIntError(e) => write!(f, "parse int error: {:?}", e),
            Error::IoError(e) => write!(f, "io error: {:?}", e),
            Error::DecodeHexError(e) => write!(f, "decode hex error: {:?}", e),
            Error::HandlerError(e) => write!(f, "handler error: {:?}", e),
            Error::UnknownError => write!(f, "unknown error"),
        }
    }
}

impl std::error::Error for Error {}

// endregion

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum UnityFileType {
    Asset,
    Info,
    Resource,
}

impl UnityFileType {
    pub const LENGTH: usize = 3;

    pub fn to_u8(&self) -> u8 {
        match self {
            UnityFileType::Asset => 0,
            UnityFileType::Info => 1,
            UnityFileType::Resource => 2,
        }
    }

    pub fn try_from_u8(b: u8) -> std::result::Result<Self, ()> {
        Ok(match b {
            0 => UnityFileType::Asset,
            1 => UnityFileType::Info,
            2 => UnityFileType::Resource,
            _ => return Err(())
        })
    }

    pub fn to_ext(&self) -> &str {
        match self {
            UnityFileType::Asset => "bin",
            UnityFileType::Info => "info",
            UnityFileType::Resource => "resource",
        }
    }

    pub fn try_from_ext(s: &str) -> std::result::Result<Self, ()> {
        match s {
            "bin" => Ok(UnityFileType::Asset),
            "info" => Ok(UnityFileType::Info),
            "resource" => Ok(UnityFileType::Resource),
            _ => Err(())
        }
    }

    pub fn to_ext_char(&self) -> u8 {
        match self {
            UnityFileType::Asset => b'a',
            UnityFileType::Info => b'i',
            UnityFileType::Resource => b'r',
        }
    }

    pub fn try_from_ext_char(b: u8) -> std::result::Result<Self, ()> {
        match b {
            b'a' => Ok(UnityFileType::Asset),
            b'i' => Ok(UnityFileType::Info),
            b'r' => Ok(UnityFileType::Resource),
            _ => Err(())
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct HexString<const N: usize>(pub [u8; N]);

impl<const N: usize> HexString<N> {
    pub fn new() -> Self {
        HexString([0u8; N])
    }

    pub fn from_hex_string(s: String) -> std::result::Result<Self, DecodeHexError> {
        let mut result = Self::new();
        decode_hex(&s, &mut result.0)?;
        Ok(result)
    }

    pub fn to_hex_string(&self) -> String {
        encode_hex(&self.0)
    }
}

impl<const N: usize> AsRef<[u8]> for HexString<N> {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl<const N: usize> ToString for HexString<N> {
    fn to_string(&self) -> String {
        self.to_hex_string()
    }
}

pub const GUID_LENGTH: usize = 16;
pub const HASH_LENGTH: usize = 16;

pub type UnityFileGuid = HexString<GUID_LENGTH>;
pub type UnityFileHash = HexString<HASH_LENGTH>;

pub fn encode_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}

#[derive(Debug)]
pub enum DecodeHexError {
    LengthNotMatched {
        string_len: usize,
        buf_len: usize,
    },
    ParseIntError(ParseIntError),
}

impl Display for DecodeHexError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeHexError::LengthNotMatched { string_len, buf_len } => write!(f, "string length {} is not two times buf length {}", string_len, buf_len),
            DecodeHexError::ParseIntError(e) => write!(f, "parse int error: {:?}", e),
        }
    }
}

impl std::error::Error for DecodeHexError {}

pub fn decode_hex(s: &str, result: &mut [u8]) -> std::result::Result<(), DecodeHexError> {
    if result.len() * 2 != s.len() {
        return Err(DecodeHexError::LengthNotMatched {
            string_len: s.len(),
            buf_len: result.len(),
        });
    }

    for i in 0..result.len() {
        match u8::from_str_radix(&s[(i * 2)..(i * 2) + 2], 16) {
            Ok(b) => result[i] = b,
            Err(e) => return Err(DecodeHexError::ParseIntError(e)),
        }
    }
    Ok(())
}

pub fn u32_to_be_hex_string(n: u32) -> String {
    let arr = [
        ((n >> 24) & 0xff) as u8,
        ((n >> 16) & 0xff) as u8,
        ((n >> 8) & 0xff) as u8,
        ((n >> 0) & 0xff) as u8,
    ];
    encode_hex(&arr[..])
}