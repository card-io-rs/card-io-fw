use crate::{
    board::{
        hal::radio::Wifi,
        wifi::{as_static_mut, as_static_ref, net_task},
    },
    task_control::TaskController,
};
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_net::{Config, Stack, StackResources};
use embassy_time::{Duration, Timer};
use esp_wifi::{
    wifi::{WifiController, WifiDevice, WifiMode},
    EspWifiInitialization,
};

pub(super) struct StaState {
    init: EspWifiInitialization,
    controller: WifiController<'static>,
    stack: Stack<WifiDevice<'static>>,
    connection_task_control: TaskController<()>,
    net_task_control: TaskController<!>,
    started: bool,
}

impl StaState {
    pub(super) fn new(
        init: EspWifiInitialization,
        config: Config,
        wifi: &'static mut Wifi,
        resources: &'static mut StackResources<3>,
        random_seed: u64,
    ) -> Self {
        let (wifi_interface, controller) =
            esp_wifi::wifi::new_with_mode(&init, wifi, WifiMode::Sta);

        Self {
            init,
            controller,
            stack: Stack::new(wifi_interface, config, resources, random_seed),
            connection_task_control: TaskController::new(),
            net_task_control: TaskController::new(),
            started: false,
        }
    }

    pub(super) async fn deinit(mut self) -> EspWifiInitialization {
        self.stop().await;
        self.init
    }

    pub(super) async fn stop(&mut self) {
        if self.started {
            let _ = join(
                self.connection_task_control.stop_from_outside(),
                self.net_task_control.stop_from_outside(),
            )
            .await;
            self.started = false;
        }
    }

    pub(super) async fn start(&mut self) -> &mut Stack<WifiDevice<'static>> {
        if !self.started {
            let spawner = Spawner::for_current_executor().await;
            unsafe {
                spawner.must_spawn(sta_task(
                    as_static_mut(&mut self.controller),
                    as_static_ref(&self.connection_task_control),
                ));
                spawner.must_spawn(net_task(
                    as_static_ref(&self.stack),
                    as_static_ref(&self.net_task_control),
                ));
            }
            self.started = true;
        }

        &mut self.stack
    }

    pub(super) fn is_connected(&self) -> bool {
        false
    }
}

#[embassy_executor::task]
pub(super) async fn sta_task(
    controller: &'static mut WifiController<'static>,
    task_control: &'static TaskController<()>,
) {
    task_control
        .run_cancellable(async {
            let mut connect_idx = None::<usize>;

            loop {
                match controller.scan_n::<8>().await {
                    Ok((networks, _)) => {
                        for network in networks {
                            log::info!(
                                "Found network: {} (RSSI: {})",
                                network.ssid,
                                network.signal_strength
                            );
                        }
                    }
                    Err(err) => log::warn!("Scan failed: {err:?}"),
                }

                if connect_idx.is_none() {
                    Timer::after(Duration::from_secs(5)).await;
                }
            }
        })
        .await;
}
