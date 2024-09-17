use futures::FutureExt;
use rtherm_common::{error::AnyError, Measurement};
use std::{error::Error, future::Future, pin::Pin};

pub type ChannelId = String;

pub trait Recepient: Send {
    type Error: Error + Send;
    fn update(
        &mut self,
        channel_id: ChannelId,
        meas: Measurement,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + '_;
}

trait DynRecepient: Send {
    fn update_any(
        &mut self,
        channel_id: ChannelId,
        meas: Measurement,
    ) -> Pin<Box<dyn Future<Output = Result<(), AnyError>> + Send + '_>>;
}

impl<P: Recepient<Error: 'static>> DynRecepient for P {
    fn update_any(
        &mut self,
        channel_id: ChannelId,
        meas: Measurement,
    ) -> Pin<Box<dyn Future<Output = Result<(), AnyError>> + Send + '_>> {
        Box::pin(
            self.update(channel_id, meas)
                .map(|r| r.map_err(AnyError::new)),
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
    fn update(
        &mut self,
        channel_id: ChannelId,
        meas: Measurement,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + '_ {
        self.0.update_any(channel_id, meas)
    }
}

impl<R: Recepient> Recepient for Vec<R> {
    type Error = R::Error;
    async fn update(
        &mut self,
        channel_id: ChannelId,
        meas: Measurement,
    ) -> Result<(), Self::Error> {
        // FIXME: Return all errors
        let mut last_err = None;
        for r in self {
            if let Err(e) = r.update(channel_id.clone(), meas).await {
                last_err = Some(e);
            }
        }
        match last_err {
            None => Ok(()),
            Some(e) => Err(e),
        }
    }
}
