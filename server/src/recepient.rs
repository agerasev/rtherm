use futures::FutureExt;
use rtherm_common::{error::AnyError, Measurements};
use std::{error::Error, future::Future, pin::Pin};

pub trait Recepient: Send {
    type Error: Error + Send;
    fn update(&mut self, meas: Measurements) -> impl Future<Output = Vec<Self::Error>> + Send + '_;
}

trait DynRecepient: Send {
    fn update_any(
        &mut self,
        meas: Measurements,
    ) -> Pin<Box<dyn Future<Output = Vec<AnyError>> + Send + '_>>;
}

impl<P: Recepient<Error: 'static>> DynRecepient for P {
    fn update_any(
        &mut self,
        meas: Measurements,
    ) -> Pin<Box<dyn Future<Output = Vec<AnyError>> + Send + '_>> {
        Box::pin(
            self.update(meas)
                .map(|errs| errs.into_iter().map(AnyError::new).collect()),
        )
    }
}

pub struct AnyRecepient(Box<dyn DynRecepient>);

impl AnyRecepient {
    pub fn new<P: Recepient<Error: 'static> + 'static>(provider: P) -> Self {
        Self(Box::new(provider))
    }
}

impl Recepient for AnyRecepient {
    type Error = AnyError;
    fn update(&mut self, meas: Measurements) -> impl Future<Output = Vec<Self::Error>> + Send + '_ {
        self.0.update_any(meas)
    }
}

impl<R: Recepient> Recepient for Vec<R> {
    type Error = R::Error;
    async fn update(&mut self, meas: Measurements) -> Vec<Self::Error> {
        let mut errors = Vec::new();
        for recepient in self {
            errors.extend(recepient.update(meas.clone()).await);
        }
        errors
    }
}
