use futures::{executor::block_on, FutureExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::Infallible,
    fmt::Display,
    future::Future,
    io,
    ops::{Deref, DerefMut},
    path::PathBuf,
    pin::Pin,
};
use tokio::{
    fs::{try_exists, File},
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{RwLock, RwLockReadGuard, RwLockWriteGuard},
};

/// Persistent storage
pub trait Storage {
    type Error: Display;
    fn load(
        &mut self,
        name: String,
    ) -> impl Future<Output = Result<Option<Vec<u8>>, Self::Error>> + Send;
    fn store(
        &mut self,
        name: String,
        value: Vec<u8>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

pub type AnyError = Box<dyn Display + Send + 'static>;

/// Actually not persistent
#[derive(Clone, Default, Debug)]
pub struct MemStorage(HashMap<String, Vec<u8>>);

impl Storage for MemStorage {
    type Error = Infallible;
    async fn load(&mut self, name: String) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.0.get(&name).map(|v| v.to_owned()))
    }
    async fn store(&mut self, name: String, value: Vec<u8>) -> Result<(), Self::Error> {
        self.0.insert(name.to_string(), value.to_owned());
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct FileStorage {
    dir: PathBuf,
}

impl FileStorage {
    pub async fn new(path: impl Into<PathBuf>) -> Result<Self, io::Error> {
        let path = path.into();
        if try_exists(&path).await? {
            Ok(Self { dir: path })
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("No such directory: {:?}", path),
            ))
        }
    }
}

impl Storage for FileStorage {
    type Error = io::Error;
    async fn load(&mut self, name: String) -> Result<Option<Vec<u8>>, Self::Error> {
        let mut file = match File::open(self.dir.join(name)).await {
            Ok(f) => f,
            Err(e) => match e.kind() {
                io::ErrorKind::NotFound => return Ok(None),
                _ => return Err(e),
            },
        };
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await?;
        Ok(Some(buf))
    }
    async fn store(&mut self, name: String, value: Vec<u8>) -> Result<(), Self::Error> {
        let mut file = File::create(self.dir.join(name)).await?;
        file.write_all(&value).await?;
        Ok(())
    }
}

trait DynStorage: Send + Sync + 'static {
    fn load_dyn(
        &mut self,
        name: String,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Vec<u8>>, AnyError>> + Send + '_>>;

    fn store_dyn(
        &mut self,
        name: String,
        value: Vec<u8>,
    ) -> Pin<Box<dyn Future<Output = Result<(), AnyError>> + Send + '_>>;
}

impl<S: Storage<Error: Send + 'static> + Send + Sync + 'static> DynStorage for S {
    fn load_dyn(
        &mut self,
        name: String,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Vec<u8>>, AnyError>> + Send + '_>> {
        Box::pin(
            self.load(name)
                .map(|r| r.map_err(|e| Box::new(e) as AnyError)),
        )
    }
    fn store_dyn(
        &mut self,
        name: String,
        value: Vec<u8>,
    ) -> Pin<Box<dyn Future<Output = Result<(), AnyError>> + Send + '_>> {
        Box::pin(
            self.store(name, value)
                .map(|r| r.map_err(|e| Box::new(e) as AnyError)),
        )
    }
}

pub struct AnyStorage(Box<dyn DynStorage>);

impl AnyStorage {
    pub fn new<S: Storage<Error: Send + 'static> + Send + Sync + 'static>(storage: S) -> Self {
        Self(Box::new(storage))
    }
}

impl Storage for AnyStorage {
    type Error = AnyError;
    async fn load(&mut self, name: String) -> Result<Option<Vec<u8>>, Self::Error> {
        self.0.load_dyn(name).await
    }
    async fn store(&mut self, name: String, value: Vec<u8>) -> Result<(), Self::Error> {
        self.0.store_dyn(name, value).await
    }
}

pub struct Stored<T: Serialize + for<'de> Deserialize<'de>, S: Storage> {
    name: String,
    storage: S,
    value: T,
}

impl<T: Serialize + for<'de> Deserialize<'de>, S: Storage> Stored<T, S> {
    pub async fn load_or(name: String, mut storage: S, value: T) -> Self {
        let value = match storage.load(name.clone()).await {
            Ok(Some(data)) => match serde_json::from_slice(&data) {
                Ok(state) => state,
                Err(e) => {
                    log::error!("Error deserializing value: {e}");
                    value
                }
            },
            Ok(None) => value,
            Err(e) => {
                log::error!("Error reading from storage: {}", e);
                value
            }
        };
        Self {
            name,
            storage,
            value,
        }
    }

    pub async fn load_or_default(name: String, storage: S) -> Self
    where
        T: Default,
    {
        Self::load_or(name, storage, T::default()).await
    }

    pub async fn dump(&mut self) {
        match serde_json::to_vec(&self.value) {
            Ok(data) => {
                if let Err(e) = self.storage.store(self.name.clone(), data).await {
                    log::error!("Error writing to storage: {}", e);
                }
            }
            Err(e) => log::error!("Error serializing value: {e}"),
        }
    }
}

pub struct StoredLock<T: Serialize + for<'de> Deserialize<'de>, S: Storage>(RwLock<Stored<T, S>>);

impl<T: Serialize + for<'de> Deserialize<'de>, S: Storage> StoredLock<T, S> {
    pub fn new(inner: Stored<T, S>) -> Self {
        Self(RwLock::new(inner))
    }

    pub async fn read(&self) -> RwLockReadGuard<T> {
        RwLockReadGuard::map(self.0.read().await, |s| &s.value)
    }
    pub async fn write(&self) -> StoredLockWriteGuard<T, S> {
        StoredLockWriteGuard {
            inner: self.0.write().await,
            dumped: false,
        }
    }
}

pub struct StoredLockWriteGuard<'a, T: Serialize + for<'de> Deserialize<'de>, S: Storage> {
    inner: RwLockWriteGuard<'a, Stored<T, S>>,
    dumped: bool,
}

impl<'a, T: Serialize + for<'de> Deserialize<'de>, S: Storage> Deref
    for StoredLockWriteGuard<'a, T, S>
{
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.inner.value
    }
}
impl<'a, T: Serialize + for<'de> Deserialize<'de>, S: Storage> DerefMut
    for StoredLockWriteGuard<'a, T, S>
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner.value
    }
}

impl<'a, T: Serialize + for<'de> Deserialize<'de>, S: Storage> Drop
    for StoredLockWriteGuard<'a, T, S>
{
    fn drop(&mut self) {
        if !self.dumped {
            log::warn!("StoredLockWriteGuard should be dropped with async_drop");
            block_on(self.inner.dump());
        }
    }
}
impl<'a, T: Serialize + for<'de> Deserialize<'de>, S: Storage> StoredLockWriteGuard<'a, T, S> {
    pub async fn async_drop(mut self) {
        self.inner.dump().await;
        self.dumped = true;
    }
}
