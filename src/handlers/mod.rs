pub use memory::MemoryHandler;
pub use nop::NopHandler;
pub use fs::FileSystemHandler;

use crate::{UnityFileGuid, UnityFileHash, UnityFileType};

mod nop;
mod memory;
mod fs;

#[derive(Debug)]
pub struct TransactionFiles<T>(Vec<Option<T>>);

impl<T> Default for TransactionFiles<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> TransactionFiles<T> {
    pub fn new() -> Self {
        Self((0..UnityFileType::LENGTH).into_iter().map(|_| None).collect())
    }

    pub fn take(&mut self, typ: UnityFileType) -> Option<T> {
        self.0.get_mut(typ.to_u8() as usize).unwrap().take()
    }

    pub fn take_all(&mut self) -> Vec<(UnityFileType, T)> {
        let mut result = Vec::with_capacity(self.0.len());
        for (i, item) in self.0.iter_mut().enumerate() {
            if item.is_some() {
                result.push((UnityFileType::try_from_u8(i as u8).unwrap(), item.take().unwrap()));
            }
        }
        result
    }

    pub fn set(&mut self, typ: UnityFileType, value: T) {
        *self.0.get_mut(typ.to_u8() as usize).unwrap() = Some(value);
    }

    pub fn get(&mut self, typ: UnityFileType) -> Option<&T> {
        self.0.get(typ.to_u8() as usize).unwrap().as_ref()
    }

    pub fn get_mut(&mut self, typ: UnityFileType) -> Option<&mut T> {
        self.0.get_mut(typ.to_u8() as usize).unwrap().as_mut()
    }
}

#[derive(Debug)]
pub struct Transaction<T> {
    guid: UnityFileGuid,
    hash: UnityFileHash,
    files: TransactionFiles<T>,
}

impl<T> Transaction<T> {
    pub fn new(guid: UnityFileGuid, hash: UnityFileHash) -> Self {
        Self {
            guid,
            hash,
            files: Default::default(),
        }
    }
}
