use core::{
    mem::MaybeUninit,
    ptr::{self, addr_of_mut},
};

use crate::{
    board::{
        hal::{radio::Wifi, Rng},
        wifi::net_task,
    },
    task_control::{TaskControlToken, TaskController},
};
use alloc::rc::Rc;
use config_site::data::network::WifiNetwork;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_net::{Config, Stack, StackResources};
use embassy_sync::{
    blocking_mutex::raw::NoopRawMutex,
    mutex::{Mutex, MutexGuard},
};
use embassy_time::{Duration, Timer};
use embedded_hal_old::prelude::_embedded_hal_blocking_rng_Read;
use embedded_svc::wifi::{AccessPointInfo, ClientConfiguration, Configuration, Wifi as _};
use esp_wifi::{
    wifi::{WifiController, WifiDevice, WifiEvent, WifiMode},
    EspWifiInitialization,
};

const SCAN_RESULTS: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConnectionState {
    NotConnected,
    Connected,
}

#[derive(Clone)]
pub struct Sta {
    stack: Rc<Stack<WifiDevice<'static>>>,
    networks: Rc<Mutex<NoopRawMutex, heapless::Vec<AccessPointInfo, SCAN_RESULTS>>>,
}

impl Sta {
    pub fn connection_state(&self) -> ConnectionState {
        match self.stack.is_link_up() {
            true => ConnectionState::Connected,
            false => ConnectionState::NotConnected,
        }
    }

    pub async fn visible_networks(
        &self,
    ) -> MutexGuard<'_, NoopRawMutex, heapless::Vec<AccessPointInfo, SCAN_RESULTS>> {
        self.networks.lock().await
    }
}

pub(super) struct StaState {
    init: EspWifiInitialization,
    controller: Rc<Mutex<NoopRawMutex, WifiController<'static>>>,
    stack: Rc<Stack<WifiDevice<'static>>>,
    networks: Rc<Mutex<NoopRawMutex, heapless::Vec<AccessPointInfo, SCAN_RESULTS>>>,
    connection_task_control: TaskController<()>,
    net_task_control: TaskController<!>,
    started: bool,
}

impl StaState {
    pub(super) fn init(
        this: &mut MaybeUninit<Self>,
        init: EspWifiInitialization,
        config: Config,
        wifi: &'static mut Wifi,
        resources: &'static mut StackResources<3>,
        mut rng: Rng,
    ) {
        log::info!("Configuring STA");

        let this = this.as_mut_ptr();

        let (wifi_interface, controller) =
            esp_wifi::wifi::new_with_mode(&init, wifi, WifiMode::Sta).unwrap();

        let mut seed = [0; 8];
        rng.read(&mut seed).unwrap();

        unsafe {
            (*this).init = init;
            ptr::write(
                addr_of_mut!((*this).controller),
                Rc::new(Mutex::new(controller)),
            );
            ptr::write(
                addr_of_mut!((*this).stack),
                Rc::new(Stack::new(
                    wifi_interface,
                    config,
                    resources,
                    u64::from_le_bytes(seed),
                )),
            );
            ptr::write(
                addr_of_mut!((*this).networks),
                Rc::new(Mutex::new(heapless::Vec::new())),
            );
            ptr::write(
                addr_of_mut!((*this).connection_task_control),
                TaskController::new(),
            );
            ptr::write(
                addr_of_mut!((*this).net_task_control),
                TaskController::new(),
            );
            (*this).started = false;
        }
    }

    pub(super) fn unwrap(self) -> EspWifiInitialization {
        self.init
    }

    pub(super) async fn stop(&mut self) {
        if self.started {
            log::info!("Stopping STA");
            let _ = join(
                self.connection_task_control.stop_from_outside(),
                self.net_task_control.stop_from_outside(),
            )
            .await;

            if matches!(self.controller.lock().await.is_started(), Ok(true)) {
                self.controller.lock().await.stop().await.unwrap();
            }

            log::info!("Stopped STA");
            self.started = false;
        }
    }

    pub(super) async fn start(&mut self) -> Sta {
        if !self.started {
            log::info!("Starting STA");
            let spawner = Spawner::for_current_executor().await;

            log::info!("Starting STA task");
            spawner.must_spawn(sta_task(
                self.controller.clone(),
                self.networks.clone(),
                self.connection_task_control.token(),
            ));
            log::info!("Starting NET task");
            spawner.must_spawn(net_task(self.stack.clone(), self.net_task_control.token()));

            self.started = true;
        }

        Sta {
            stack: self.stack.clone(),
            networks: self.networks.clone(),
        }
    }
}

#[embassy_executor::task]
pub(super) async fn sta_task(
    controller: Rc<Mutex<NoopRawMutex, WifiController<'static>>>,
    networks: Rc<Mutex<NoopRawMutex, heapless::Vec<AccessPointInfo, SCAN_RESULTS>>>,
    mut task_control: TaskControlToken<()>,
) {
    task_control
        .run_cancellable(async {
            let known_networks = [];

            loop {
                if !matches!(controller.lock().await.is_started(), Ok(true)) {
                    log::info!("Starting wifi");
                    controller.lock().await.start().await.unwrap();
                    log::info!("Wifi started!");
                }

                let connect_to = 'select: loop {
                    match controller.lock().await.scan_n::<SCAN_RESULTS>().await {
                        Ok((mut visible_networks, network_count)) => {
                            log::info!("Found {network_count} access points");

                            // Sort by signal strength, descending
                            visible_networks
                                .sort_by(|a, b| b.signal_strength.cmp(&a.signal_strength));

                            let mut networks = networks.lock().await;

                            *networks = visible_networks;

                            if let Some(connect_to) =
                                select_visible_known_network(&known_networks, networks.as_slice())
                            {
                                break 'select connect_to;
                            }
                        }
                        Err(err) => log::warn!("Scan failed: {err:?}"),
                    }

                    Timer::after(Duration::from_secs(5)).await;
                };

                controller
                    .lock()
                    .await
                    .set_configuration(&Configuration::Client(ClientConfiguration {
                        ssid: connect_to.ssid.clone(),
                        password: connect_to.pass.clone(),
                        ..Default::default()
                    }))
                    .unwrap();

                log::info!("Connecting...");

                match controller.lock().await.connect().await {
                    Ok(_) => log::info!("Wifi connected!"),
                    Err(e) => {
                        log::warn!("Failed to connect to wifi: {e:?}");
                        Timer::after(Duration::from_millis(5000)).await
                    }
                }

                controller
                    .lock()
                    .await
                    .wait_for_event(WifiEvent::StaDisconnected)
                    .await;
            }
        })
        .await;
}

fn select_visible_known_network<'a>(
    known_networks: &'a [WifiNetwork],
    visible_networks: &[AccessPointInfo],
) -> Option<&'a WifiNetwork> {
    for network in visible_networks {
        if let Some(known_network) = known_networks.iter().find(|n| n.ssid == network.ssid) {
            return Some(known_network);
        }
    }

    None
}
