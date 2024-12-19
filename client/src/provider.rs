use futures::{future::join_all, FutureExt};
use rtherm_common::{error::AnyError, merge_groups, Measurements};
use std::{error::Error, future::Future, pin::Pin};

type MeasurementsAndErrors<E> = (Measurements<String>, Vec<E>);

pub trait Provider: Send {
    type Error: Error + Send;
    fn measure(&mut self) -> impl Future<Output = MeasurementsAndErrors<Self::Error>> + Send + '_;
}

trait DynProvider: Send {
    #[allow(clippy::type_complexity)]
    fn read_any(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = MeasurementsAndErrors<AnyError>> + Send + '_>>;
}

impl<P: Provider<Error: 'static>> DynProvider for P {
    fn read_any(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = MeasurementsAndErrors<AnyError>> + Send + '_>> {
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
    fn measure(&mut self) -> impl Future<Output = MeasurementsAndErrors<Self::Error>> + Send + '_ {
        self.0.read_any()
    }
}

impl<P: Provider> Provider for Vec<P> {
    type Error = P::Error;
    async fn measure(&mut self) -> MeasurementsAndErrors<Self::Error> {
        let (meas, errors): (Vec<_>, Vec<_>) = join_all(self.iter_mut().map(|p| p.measure()))
            .await
            .into_iter()
            .unzip();
        (merge_groups(meas), errors.into_iter().flatten().collect())
    }
}
