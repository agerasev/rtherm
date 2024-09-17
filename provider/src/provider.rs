use futures::FutureExt;
use rtherm_common::{error::AnyError, Measurement};
use std::{collections::HashMap, error::Error, future::Future, pin::Pin};

pub trait Provider {
    type Error: Error + 'static;
    fn read_all(
        &mut self,
    ) -> impl Future<Output = Result<HashMap<String, Measurement>, Self::Error>> + Send + '_;
}

#[allow(clippy::type_complexity)]
pub trait DynProvider {
    fn read_all_any(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<HashMap<String, Measurement>, AnyError>> + Send + '_>>;
}

impl<P: Provider> DynProvider for P {
    fn read_all_any(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<HashMap<String, Measurement>, AnyError>> + Send + '_>>
    {
        Box::pin(self.read_all().map(|r| r.map_err(AnyError::new)))
    }
}

pub struct AnyProvider(pub Box<dyn DynProvider>);

impl AnyProvider {
    pub fn new<P: Provider + 'static>(provider: P) -> Self {
        Self(Box::new(provider))
    }
}

impl Provider for AnyProvider {
    type Error = AnyError;
    fn read_all(
        &mut self,
    ) -> impl Future<Output = Result<HashMap<String, Measurement>, Self::Error>> + Send + '_ {
        self.0.read_all_any()
    }
}
