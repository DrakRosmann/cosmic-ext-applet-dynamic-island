// SPDX-License-Identifier: GPL-3.0-only

//! The battery, from UPower: the island shows when it starts charging, as
//! phones do when they are plugged in.

use cosmic::iced::{Subscription, stream};
use futures::{SinkExt, StreamExt, channel::mpsc};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Battery {
    pub percentage: f64,
    pub charging: bool,
    pub full: bool,
    /// Seconds until full, when charging and known.
    pub time_to_full: Option<i64>,
}

#[zbus::proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower/devices/DisplayDevice"
)]
trait Device {
    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;
    #[zbus(property, name = "Type")]
    fn kind(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn time_to_full(&self) -> zbus::Result<i64>;
}

async fn read(device: &DeviceProxy<'_>) -> Option<Battery> {
    // 2: a battery; desktops have none.
    if !device.is_present().await.ok()? || device.kind().await.ok()? != 2 {
        return None;
    }
    // 1 charging, 4 full, 5 pending charge.
    let state = device.state().await.ok()?;
    let charging = matches!(state, 1 | 5);
    Some(Battery {
        percentage: device.percentage().await.ok()?,
        charging,
        full: state == 4,
        time_to_full: device
            .time_to_full()
            .await
            .ok()
            .filter(|seconds| charging && *seconds > 0),
    })
}

pub fn subscription() -> Subscription<Option<Battery>> {
    Subscription::run_with("battery", |_| {
        stream::channel(4, |mut output: mpsc::Sender<Option<Battery>>| async move {
            let Ok(conn) = zbus::Connection::system().await else {
                return futures::future::pending().await;
            };
            let Ok(device) = DeviceProxy::builder(&conn)
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
            else {
                return futures::future::pending().await;
            };
            let Ok(properties) = zbus::fdo::PropertiesProxy::builder(&conn)
                .destination("org.freedesktop.UPower")
                .and_then(|b| b.path("/org/freedesktop/UPower/devices/DisplayDevice"))
            else {
                return futures::future::pending().await;
            };
            let Ok(properties) = properties.build().await else {
                return futures::future::pending().await;
            };
            let Ok(mut changes) = properties.receive_properties_changed().await else {
                return futures::future::pending().await;
            };
            loop {
                if output.send(read(&device).await).await.is_err() {
                    return;
                }
                if changes.next().await.is_none() {
                    return futures::future::pending().await;
                }
            }
        })
    })
}
