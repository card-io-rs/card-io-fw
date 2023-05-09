use crate::{
    board::{AdcDrdy, AdcReset, AdcSpi, DisplayInterface, DisplayReset, TouchDetect},
    display,
    frontend::Frontend,
    states::MIN_FRAME_TIME,
    AppState,
};
use embassy_time::Ticker;
use embedded_graphics::{pixelcolor::BinaryColor, prelude::*};
use gui::screens::{
    main_menu::{MainMenu, MainMenuEvents},
    MENU_STYLE,
};

pub async fn main_menu(
    display: &mut display::PoweredDisplay<'_, DisplayInterface<'_>, DisplayReset>,
    frontend: &mut Frontend<AdcSpi<'_>, AdcDrdy, AdcReset, TouchDetect>,
) -> AppState {
    let mut menu = MainMenu {}.create_menu_with_style(MENU_STYLE);

    let mut ticker = Ticker::every(MIN_FRAME_TIME);
    loop {
        display.clear(BinaryColor::Off).unwrap();

        if let Some(event) = menu.interact(frontend.is_touched()) {
            return match event {
                MainMenuEvents::Shutdown => AppState::Shutdown,
            };
        }

        menu.update(display);
        menu.draw(display).unwrap();

        display.flush().await.unwrap();

        ticker.next().await;
    }
}
