use crate::{
    board::{
        initialized::{Board, StaMode},
        wifi::sta::Sta,
    },
    states::{menu::AppMenu, TouchInputShaper, MENU_IDLE_DURATION, MIN_FRAME_TIME},
    timeout::Timeout,
    AppState,
};
use alloc::format;
use embassy_time::{Duration, Ticker};
use embedded_graphics::Drawable;
use embedded_menu::{items::NavigationItem, Menu};
use gui::{
    screens::{menu_style, screen::Screen},
    widgets::{battery_small::Battery, status_bar::StatusBar, wifi::WifiStateView},
};

#[derive(Clone, Copy)]
pub enum BatteryEvents {
    None,
    Back,
}

pub async fn battery_info_menu(board: &mut Board) -> AppState {
    let sta = if !board.config.known_networks.is_empty() {
        board.enable_wifi_sta(StaMode::OnDemand).await
    } else {
        board.wifi.stop_if().await;
        None
    };
    let mut exit_timer = Timeout::new(MENU_IDLE_DURATION);
    let mut menu_state = Default::default();

    let list_item = |label| NavigationItem::new(label, BatteryEvents::None);

    let mut items = heapless::Vec::<_, 5>::new();

    let mut ticker = Ticker::every(MIN_FRAME_TIME);
    let mut input = TouchInputShaper::new(&mut board.frontend);

    let mut load_sensor_data = Timeout::new(Duration::from_secs(1));
    let mut first = true;
    while !exit_timer.is_elapsed() {
        let is_touched = input.is_touched();
        if is_touched {
            exit_timer.reset();
        }

        if first || load_sensor_data.is_elapsed() {
            load_sensor_data.reset();
            first = false;

            items.clear();

            let mut sensor = board.battery_monitor.sensor().await;

            let voltage = unwrap!(sensor.fg.read_vcell().await.ok());
            unwrap!(items
                .push(list_item(format!("Voltage: {:>9}mV", voltage / 1000)))
                .ok());

            let current = unwrap!(sensor.fg.read_current().await.ok());
            unwrap!(items
                .push(list_item(format!("Current: {:>9}mA", current / 1000)))
                .ok());

            let capacity = unwrap!(sensor.fg.read_design_capacity().await.ok());
            unwrap!(items
                .push(list_item(format!("Nominal: {:>8}mAh", capacity / 1000)))
                .ok());

            let capacity = unwrap!(sensor.fg.read_reported_capacity().await.ok());
            unwrap!(items
                .push(list_item(format!("Capacity: {:>7}mAh", capacity / 1000)))
                .ok());

            let charge_cycles = unwrap!(sensor.fg.read_charge_cycles().await.ok());
            unwrap!(items
                .push(list_item(format!("Chg Cycles: {:>8}", charge_cycles)))
                .ok());
        }

        let mut menu_screen = Screen {
            content: Menu::with_style("Battery info", menu_style())
                .add_items(&mut items[..])
                .add_item(NavigationItem::new("Back", BatteryEvents::Back))
                .build_with_state(menu_state),

            status_bar: StatusBar {
                battery: Battery::with_style(
                    board.battery_monitor.battery_data(),
                    board.config.battery_style(),
                ),
                wifi: WifiStateView::new(sta.as_ref().map(Sta::connection_state)),
            },
        };

        if let Some(event) = menu_screen.content.interact(is_touched) {
            match event {
                BatteryEvents::None => {}
                BatteryEvents::Back => return AppState::Menu(AppMenu::DeviceInfo),
            };
        }

        let battery_data = board.battery_monitor.battery_data();

        if let Some(battery) = battery_data {
            if battery.is_low {
                return AppState::Shutdown;
            }
        }

        menu_screen.status_bar.update_battery_data(battery_data);
        if let Some(ref sta) = sta {
            menu_screen.status_bar.wifi.update(sta.connection_state());
        };

        unwrap!(board
            .display
            .frame(|display| {
                menu_screen.content.update(display);
                menu_screen.draw(display)
            })
            .await
            .ok());

        menu_state = menu_screen.content.state();

        ticker.next().await;
    }

    AppState::Menu(AppMenu::Main)
}
