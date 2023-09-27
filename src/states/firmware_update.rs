use embassy_futures::select::{select, Either};
use embassy_net::{dns::DnsSocket, tcp::client::TcpClient};
use embassy_time::{Duration, Timer};
use embedded_io::asynch::Read;
use reqwless::{client::HttpClient, request::Method, response::Status};
use ufmt::uwrite;

use crate::{
    board::{
        initialized::{Board, StaMode},
        wait_for_connection, HttpClientResources,
    },
    states::{display_message, menu::AppMenu},
    AppState, SerialNumber,
};

pub async fn firmware_update(board: &mut Board) -> AppState {
    do_update(board).await;

    AppState::Menu(AppMenu::Main)
}

async fn do_update(board: &mut Board) {
    display_message(board, "Connecting to WiFi").await;

    let Some(sta) = board.enable_wifi_sta(StaMode::Enable).await else {
        display_message(board, "Could not enable WiFi").await;
        return;
    };

    if !wait_for_connection(&sta, board).await {
        // If we do not have a network connection, save to file.
        return;
    }

    display_message(board, "Looking for updates").await;

    let mut resources = HttpClientResources::new_boxed();

    let client = TcpClient::new(sta.stack(), &resources.client_state);
    let dns = DnsSocket::new(sta.stack());

    let mut client = HttpClient::new(&client, &dns);

    const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

    let mut url = heapless::String::<128>::new();
    if uwrite!(
        &mut url,
        "{}/firmware/{}/{}/{}",
        board.config.backend_url.as_str(),
        env!("HW_VERSION"),
        SerialNumber::new(),
        env!("COMMIT_HASH")
    )
    .is_err()
    {
        error!("URL too long");
        return;
    }

    debug!("Looking for update at {}", url.as_str());

    let connect = select(
        client.request(Method::GET, &url),
        Timer::after(CONNECT_TIMEOUT),
    )
    .await;

    let mut request = match connect {
        Either::First(Ok(request)) => request,
        Either::First(Err(e)) => {
            display_message(board, "Connection failed").await;
            warn!("HTTP connect error: {}", e);
            return;
        }
        Either::Second(_) => {
            display_message(board, "Connection timeout").await;
            warn!("Conect timeout");
            return;
        }
    };

    let result = request.send(&mut resources.rx_buffer).await;

    let response = match result {
        Ok(response) => match response.status {
            Status::Ok => response,
            Status::NoContent => {
                display_message(board, "Already up to date").await;
                return;
            }
            _ => {
                display_message(board, "Failed to download update").await;
                warn!("HTTP response error: {}", response.status);
                return;
            }
        },
        Err(e) => {
            display_message(board, "Failed to download update").await;
            warn!("HTTP response error: {}", e);
            return;
        }
    };

    let size = response.content_length;

    // TODO: look up update partition, erase

    let mut current = 0;
    let mut message = heapless::String::<128>::new();

    let mut reader = response.body().reader();

    let mut buffer = [0; 1024];

    loop {
        let read = match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(read) => read,
            Err(e) => {
                display_message(board, "Failed to download update").await;
                warn!("HTTP read error: {}", e);
                return;
            }
        };

        // TODO write update

        current += read;

        message.clear();
        if let Some(size) = size {
            unwrap!(uwrite!(
                &mut message,
                "Downloading update: {}%",
                current * 100 / size
            ));
        } else {
            unwrap!(uwrite!(
                &mut message,
                "Downloading update: {} bytes",
                current
            ));
        }
        display_message(board, message.as_str()).await;
    }

    display_message(board, "Update complete").await;
}
