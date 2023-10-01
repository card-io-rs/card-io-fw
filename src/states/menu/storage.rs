use crate::{
    board::{config::Config, initialized::Board, storage::FileSystem},
    human_readable::BinarySize,
    states::{
        display_message,
        menu::{AppMenu, MenuScreen},
    },
    AppState,
};
use embedded_io::asynch::{Read, Write};
use embedded_menu::{
    items::{NavigationItem, Select},
    SelectValue,
};
use gui::screens::create_menu;
use norfs::storable::{LoadError, Loadable, Storable};
use ufmt::uwrite;

use super::AppMenuBuilder;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, SelectValue)]
pub enum MeasurementAction {
    Ask = 0,
    Auto = 1,
    Store = 2,
    Upload = 3,
    Discard = 4,
}

impl Loadable for MeasurementAction {
    async fn load<R: Read>(reader: &mut R) -> Result<Self, LoadError<R::Error>> {
        let data = match u8::load(reader).await? {
            0 => Self::Ask,
            1 => Self::Auto,
            2 => Self::Store,
            3 => Self::Upload,
            4 => Self::Discard,
            _ => return Err(LoadError::InvalidValue),
        };

        Ok(data)
    }
}

impl Storable for MeasurementAction {
    async fn store<W: Write>(&self, writer: &mut W) -> Result<(), W::Error> {
        writer.write_all(&[*self as u8]).await
    }
}

#[derive(Clone, Copy)]
pub enum StorageMenuEvents {
    ChangeMeasurementAction(MeasurementAction),
    Format,
    Upload,
    Nothing,
    Back,
}

pub async fn storage_menu(board: &mut Board) -> AppState {
    let result = StorageMenu
        .display(board)
        .await
        .unwrap_or(AppState::Shutdown);

    board.save_config().await;

    result
}

struct StorageMenu;

impl MenuScreen for StorageMenu {
    type Event = StorageMenuEvents;
    type Result = AppState;

    async fn menu(&mut self, board: &mut Board) -> impl AppMenuBuilder<Self::Event> {
        // needs to be separate because the item is of a different type
        let mut used_item = heapless::Vec::<_, 1>::new();
        if let Some(storage) = board.storage.as_mut() {
            if let Ok(used) = storage.used_bytes().await {
                let mut used_str = heapless::String::<32>::new();
                unwrap!(uwrite!(
                    &mut used_str,
                    "{}/{}",
                    BinarySize(used),
                    BinarySize(storage.capacity())
                )
                .ok());

                unwrap!(used_item
                    .push(
                        NavigationItem::new("Used", StorageMenuEvents::Nothing)
                            .with_marker(used_str)
                    )
                    .ok());
            }
        }

        let mut items = heapless::Vec::<_, 2>::new();
        unwrap!(items
            .push(NavigationItem::new(
                "Format storage",
                StorageMenuEvents::Format,
            ))
            .ok());

        if board.can_enable_wifi()
            && !board.config.known_networks.is_empty()
            && !board.config.backend_url.is_empty()
            && board.sta_has_work().await
        {
            unwrap!(items
                .push(NavigationItem::new(
                    "Upload data",
                    StorageMenuEvents::Upload
                ))
                .ok());
        }

        create_menu("Storage")
            .add_item(
                Select::new("New EKG", board.config.measurement_action)
                    .with_value_converter(StorageMenuEvents::ChangeMeasurementAction)
                    .with_detail_text("What to do with new measurements"),
            )
            .add_items(used_item)
            .add_items(items)
            .add_item(NavigationItem::new("Back", StorageMenuEvents::Back))
    }

    async fn handle_event(
        &mut self,
        event: Self::Event,
        board: &mut Board,
    ) -> Option<Self::Result> {
        match event {
            StorageMenuEvents::ChangeMeasurementAction(action) => {
                debug!("Settings changed");

                board.config_changed = true;
                board.config.measurement_action = action;
            }
            StorageMenuEvents::Format => {
                info!("Format requested");
                display_message(board, "Formatting storage...").await;
                core::mem::drop(board.storage.take());
                FileSystem::format().await;
                board.storage = FileSystem::mount().await;

                // Prevent saving config changes
                *board.config = Config::default();
                board.config_changed = false;

                return Some(AppState::Menu(AppMenu::Main));
            }
            StorageMenuEvents::Upload => return Some(AppState::UploadStored(AppMenu::Storage)),
            StorageMenuEvents::Back => return Some(AppState::Menu(AppMenu::Main)),
            StorageMenuEvents::Nothing => {}
        }

        None
    }
}
