use rtherm_common::{merge_groups, Measurements};
use std::{convert::Infallible, error::Error, future::Future, mem, ops::Deref};

pub trait Storage: Send {
    type Error: Error + Send;
    type Guard<'a>: StorageGuard<Error = Self::Error> + 'a
    where
        Self: 'a;

    fn store(
        &mut self,
        meas: Measurements,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + '_;

    fn load(&mut self) -> impl Future<Output = Result<Self::Guard<'_>, Self::Error>> + Send + '_;
}

pub trait StorageGuard: Deref<Target = Measurements> + Send {
    type Error: Error + Send;

    /// Remove data from storage
    fn remove(self) -> impl Future<Output = Result<Measurements, Self::Error>> + Send;
}

#[derive(Clone, Default, Debug)]
pub struct MemStorage {
    measurements: Measurements,
}

impl Storage for MemStorage {
    type Error = Infallible;
    type Guard<'a> = MemStorageGuard<'a>;

    async fn store(&mut self, meas: Measurements) -> Result<(), Self::Error> {
        self.measurements = merge_groups([mem::take(&mut self.measurements), meas]);
        Ok(())
    }

    async fn load(&mut self) -> Result<Self::Guard<'_>, Self::Error> {
        Ok(MemStorageGuard { storage: self })
    }
}

#[derive(Debug)]
pub struct MemStorageGuard<'a> {
    storage: &'a mut MemStorage,
}

impl Deref for MemStorageGuard<'_> {
    type Target = Measurements;
    fn deref(&self) -> &Self::Target {
        &self.storage.measurements
    }
}

impl<'a> StorageGuard for MemStorageGuard<'a> {
    type Error = Infallible;

    async fn remove(self) -> Result<Measurements, Self::Error> {
        Ok(mem::take(&mut self.storage.measurements))
    }
}
