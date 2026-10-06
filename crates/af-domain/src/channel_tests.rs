use std::time::Duration;

use super::{ChannelTimeout, ChannelTimeoutError, MAX_CHANNEL_TIMEOUT_SECS};

#[test]
fn channel_timeout_accepts_only_the_supported_closed_range() {
    assert_eq!(ChannelTimeout::new(0), Err(ChannelTimeoutError));
    assert_eq!(
        ChannelTimeout::new(MAX_CHANNEL_TIMEOUT_SECS + 1),
        Err(ChannelTimeoutError)
    );

    let minimum = ChannelTimeout::new(1).unwrap();
    let maximum = ChannelTimeout::new(MAX_CHANNEL_TIMEOUT_SECS).unwrap();
    assert_eq!(minimum.seconds(), 1);
    assert_eq!(maximum.duration(), Duration::from_secs(900));
}
