diesel::table! {
    measurements (channel, time) {
        channel -> VarChar,
        value -> Float,
        time -> Time,
    }
}
