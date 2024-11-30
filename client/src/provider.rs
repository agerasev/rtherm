use futures::{future::join_all, FutureExt};
use rtherm_common::{error::AnyError, merge_groups, Measurements};
use std::{error::Error, future::Future, pin::Pin};

pub trait Provider: Send {
    type Error: Error + Send;
    fn measure(
        &mut self,
    ) -> impl Future<Output = (Measurements<String>, Vec<Self::Error>)> + Send + '_;
}

trait DynProvider: Send {
    fn read_any(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = (Measurements<String>, Vec<AnyError>)> + Send + '_>>;
}

impl<P: Provider<Error: 'static>> DynProvider for P {
    fn read_any(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = (Measurements<String>, Vec<AnyError>)> + Send + '_>> {
        Box::pin(
            self.measure()
                .map(|(meas, errs)| (meas, errs.into_iter().map(AnyError::new).collect())),
        )
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
    fn measure(
        &mut self,
    ) -> impl Future<Output = (Measurements<String>, Vec<Self::Error>)> + Send + '_ {
        self.0.read_any()
    }
}

impl<P: Provider> Provider for Vec<P> {
    type Error = P::Error;
    async fn measure(&mut self) -> (Measurements<String>, Vec<Self::Error>) {
        let (meas, errors): (Vec<_>, Vec<_>) = join_all(self.iter_mut().map(|p| p.measure()))
            .await
            .into_iter()
            .unzip();
        (merge_groups(meas), errors.into_iter().flatten().collect())
    }
}
