use futures::FutureExt;
use rtherm_common::{error::AnyError, Measurement};
use std::{collections::HashMap, error::Error, future::Future, pin::Pin};

pub trait Provider: Send {
    type Error: Error + Send;
    fn read_all(
        &mut self,
    ) -> impl Future<Output = Result<HashMap<String, Measurement>, Self::Error>> + Send + '_;
}

#[allow(clippy::type_complexity)]
trait DynProvider: Send {
    fn read_all_any(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<HashMap<String, Measurement>, AnyError>> + Send + '_>>;
}

impl<P: Provider<Error: 'static>> DynProvider for P {
    fn read_all_any(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<HashMap<String, Measurement>, AnyError>> + Send + '_>>
    {
        Box::pin(self.read_all().map(|r| r.map_err(AnyError::new)))
    }
}

pub struct AnyProvider(Box<dyn DynProvider>);

impl AnyProvider {
    pub fn new<P: Provider<Error: 'static> + 'static>(provider: P) -> Self {
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

impl<P: Provider> Provider for Vec<P> {
    type Error = P::Error;
    async fn read_all(&mut self) -> Result<HashMap<String, Measurement>, Self::Error> {
        let mut measurements = HashMap::new();
        for p in self {
            // FIXME: Error on name collision
            measurements.extend(p.read_all().await?);
        }
        // FIXME: Return both measurements and errors
        Ok(measurements)
    }
}
