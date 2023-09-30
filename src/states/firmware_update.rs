use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Instant, Timer};
use embedded_io::asynch::Read;
use reqwless::{request::Method, response::Status};
use ufmt::uwrite;

use crate::{
    board::{
        initialized::{Board, StaMode},
        ota::{Ota0Partition, Ota1Partition, OtaClient, OtaDataPartition},
        wait_for_connection,
    },
    human_readable::Throughput,
    states::{display_message, menu::AppMenu},
    AppState, SerialNumber,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, PartialEq)]
enum UpdateError {
    WifiNotEnabled,
    WifiNotConnected,
    InternalError,
    HttpConnectionFailed,
    HttpConnectionTimeout,
    HttpRequestTimeout,
    HttpRequestFailed,
    WriteError,
    DownloadFailed,
    DownloadTimeout,
    EraseFailed,
    ActivateFailed,
}

#[derive(Clone, Copy, PartialEq)]
enum UpdateResult {
    Success,
    AlreadyUpToDate,
    Failed(UpdateError),
}

pub async fn firmware_update(board: &mut Board) -> AppState {
    let update_result = do_update(board).await;

    let message = match update_result {
        UpdateResult::Success => "Update complete",
        UpdateResult::AlreadyUpToDate => "Already up to date",
        UpdateResult::Failed(e) => match e {
            UpdateError::WifiNotEnabled => "WiFi not enabled",
            UpdateError::WifiNotConnected => "Could not connect to WiFi",
            UpdateError::InternalError => "Update failed: internal error",
            UpdateError::HttpConnectionFailed => "Failed to connect to update server",
            UpdateError::HttpConnectionTimeout => "Connection to update server timed out",
            UpdateError::HttpRequestTimeout => "Update request timed out",
            UpdateError::HttpRequestFailed => "Failed to check for update",
            UpdateError::EraseFailed => "Failed to erase update partition",
            UpdateError::WriteError => "Failed to write update",
            UpdateError::DownloadFailed => "Failed to download update",
            UpdateError::DownloadTimeout => "Download timed out",
            UpdateError::ActivateFailed => "Failed to finalize update",
        },
    };

    display_message(board, message).await;

    if let UpdateResult::Success = update_result {
        AppState::Shutdown
    } else {
        AppState::Menu(AppMenu::Main)
    }
}

async fn do_update(board: &mut Board) -> UpdateResult {
    display_message(board, "Connecting to WiFi").await;

    let Some(sta) = board.enable_wifi_sta(StaMode::Enable).await else {
        return UpdateResult::Failed(UpdateError::WifiNotEnabled);
    };

    if !wait_for_connection(&sta, board).await {
        return UpdateResult::Failed(UpdateError::WifiNotConnected);
    }

    display_message(board, "Looking for updates").await;

    let Ok(mut client_resources) = sta.https_client_resources() else {
        return UpdateResult::Failed(UpdateError::InternalError);
    };
    let mut client = client_resources.client();

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
        return UpdateResult::Failed(UpdateError::InternalError);
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
            warn!("HTTP connect error: {}", e);
            return UpdateResult::Failed(UpdateError::HttpConnectionFailed);
        }
        Either::Second(_) => return UpdateResult::Failed(UpdateError::HttpConnectionTimeout),
    };

    let mut rx_buffer = [0; 512];
    let result = match select(request.send(&mut rx_buffer), Timer::after(READ_TIMEOUT)).await {
        Either::First(result) => result,
        _ => return UpdateResult::Failed(UpdateError::HttpRequestTimeout),
    };

    let response = match result {
        Ok(response) => match response.status {
            Status::Ok => response,
            Status::NotModified => return UpdateResult::AlreadyUpToDate,
            _ => {
                warn!("HTTP response error: {}", response.status);
                return UpdateResult::Failed(UpdateError::HttpRequestFailed);
            }
        },
        Err(e) => {
            warn!("HTTP response error: {}", e);
            return UpdateResult::Failed(UpdateError::HttpRequestFailed);
        }
    };

    let mut ota = match OtaClient::initialize(OtaDataPartition, Ota0Partition, Ota1Partition).await
    {
        Ok(ota) => ota,
        Err(e) => {
            warn!("Failed to initialize OTA: {}", e);
            return UpdateResult::Failed(UpdateError::InternalError);
        }
    };

    let size = response.content_length;
    let mut total = 0;
    let mut buffer = [0; 512];

    print_progress(board, &mut buffer, total, size, None).await;

    if let Err(e) = ota.erase().await {
        warn!("Failed to erase OTA: {}", e);
        return UpdateResult::Failed(UpdateError::EraseFailed);
    };

    let mut reader = response.body().reader();

    let started = Instant::now();
    let mut last_print = Instant::now();
    let mut received_1s = 0;
    loop {
        let received_buffer =
            match select(reader.read(&mut buffer), Timer::after(READ_TIMEOUT)).await {
                Either::First(result) => match result {
                    Ok(0) => break,
                    Ok(read) => &buffer[..read],
                    Err(e) => {
                        warn!("HTTP read error: {}", e);
                        return UpdateResult::Failed(UpdateError::DownloadFailed);
                    }
                },
                _ => return UpdateResult::Failed(UpdateError::DownloadTimeout),
            };

        if let Err(e) = ota.write(received_buffer).await {
            warn!("Failed to write OTA: {}", e);
            return UpdateResult::Failed(UpdateError::WriteError);
        }

        total += received_buffer.len();
        received_1s += received_buffer.len();

        let elapsed = last_print.elapsed();
        if elapsed.as_millis() > 500 {
            let speed = Throughput(received_1s, elapsed);
            let avg_speed = Throughput(total, started.elapsed());

            debug!(
                "got {}B in {}ms {}",
                received_1s,
                elapsed.as_millis(),
                speed
            );
            last_print = Instant::now();
            received_1s = 0;

            print_progress(board, &mut buffer, total, size, Some(avg_speed)).await;
        }
    }

    if let Err(e) = ota.activate().await {
        warn!("Failed to activate OTA: {}", e);
        return UpdateResult::Failed(UpdateError::ActivateFailed);
    }

    UpdateResult::Success
}

async fn print_progress(
    board: &mut Board,
    message: &mut [u8],
    current: usize,
    size: Option<usize>,
    speed: Option<Throughput>,
) {
    let mut message = slice_string::SliceString::new(message);
    if let Some(size) = size {
        let progress = current * 100 / size;
        unwrap!(uwrite!(message, "Downloading update: {}%", progress));
    } else {
        unwrap!(uwrite!(message, "Downloading update: {} bytes", current));
    }

    if let Some(speed) = speed {
        unwrap!(uwrite!(message, "\n{}", speed));
    }

    display_message(board, message.as_str()).await;
}
