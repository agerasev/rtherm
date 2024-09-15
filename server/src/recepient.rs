use rtherm_common::Measurement;
use std::{fmt::Debug, future::Future};

pub type ChannelId = String;

pub trait Recepient {
    type Error: Debug;
    fn update(
        &mut self,
        channel_id: ChannelId,
        meas: Measurement,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
