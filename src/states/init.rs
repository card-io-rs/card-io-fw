use crate::{
    board::initialized::Board,
    states::{AppMenu, MIN_FRAME_TIME},
    AppState,
};
use embassy_time::{Duration, Instant, Ticker};
use embedded_graphics::Drawable;
use gui::{
    screens::init::StartupScreen,
    widgets::{battery_small::Battery, status_bar::StatusBar},
};

pub async fn initialize(board: &mut Board) -> AppState {
    const INIT_TIME: Duration = Duration::from_secs(4);
    const MENU_THRESHOLD: Duration = Duration::from_secs(2);

    let entered = Instant::now();
    let mut ticker = Ticker::every(MIN_FRAME_TIME);
    while let elapsed = entered.elapsed() && elapsed <= INIT_TIME {
        if !board.frontend.is_touched() {
            return if elapsed > MENU_THRESHOLD {
                AppState::Menu(AppMenu::Main)
            } else {
                AppState::Shutdown
            };
        }

        let battery_data = board.battery_monitor.battery_data();

        if let Some(battery) = battery_data {
            if battery.is_low {
                return AppState::Shutdown;
            }
        }

        let elapsed_secs = elapsed.as_millis() as u32;
        let max_secs = (INIT_TIME.as_millis() as u32).max(elapsed_secs);

        let max_progress = 255;
        let progress = (elapsed_secs * max_progress) / max_secs;

        let init_screen = StartupScreen {
            label: if elapsed > MENU_THRESHOLD {
                "Release to menu"
            } else {
                "Release to shutdown"
            },
            progress,
            max_progress,

            status_bar: StatusBar {
                battery: Battery::with_style(
                    battery_data,
                    board.config.battery_style(),
                ),
            },
        };

        board.display
            .frame(|display| init_screen.draw(display))
            .await
            .unwrap();

        ticker.next().await;
    }

    AppState::Measure
}
