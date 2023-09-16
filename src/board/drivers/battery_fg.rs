use embassy_time::{Delay, Duration, Ticker};
use embedded_hal::digital::OutputPin;
use embedded_hal_async::{delay::DelayUs, i2c::I2c};
use max17055::Max17055;

use crate::{
    board::drivers::battery_monitor::SharedBatteryState, hal::gpio::RTCPinWithResistors,
    task_control::TaskControlToken, Shared,
};

#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct BatteryFgData {
    pub voltage: u16,
    pub percentage: u8,
}

pub struct BatteryFg<I2C, EN> {
    pub fg: Max17055<I2C>,
    pub enable: EN,
}

impl<I2C, EN> BatteryFg<I2C, EN>
where
    EN: OutputPin + RTCPinWithResistors,
    I2C: I2c,
{
    pub fn new(fg: Max17055<I2C>, enable: EN) -> Self {
        Self { fg, enable }
    }

    pub async fn enable<D: DelayUs>(&mut self, delay: &mut D) {
        unwrap!(self.enable.set_high().ok());
        delay.delay_ms(10).await;
        unwrap!(self.fg.load_initial_config_async(delay).await.ok());
    }

    pub async fn read_data(&mut self) -> Result<BatteryFgData, ()> {
        let voltage_uv = unwrap!(self.fg.read_vcell().await.ok());
        let percentage = unwrap!(self.fg.read_reported_soc().await.ok());

        Ok(BatteryFgData {
            voltage: (voltage_uv / 1000) as u16, // mV
            percentage,
        })
    }

    pub fn disable(&mut self) {
        // We want to keep the fuel gauge out of shutdown mode
        self.enable.rtcio_pad_hold(true);
        self.enable.rtcio_pullup(true);
    }
}

#[embassy_executor::task]
pub async fn monitor_task_fg(
    fuel_gauge: Shared<crate::board::BatteryFg>,
    battery_state: SharedBatteryState,
    mut task_control: TaskControlToken<()>,
) {
    task_control
        .run_cancellable(async {
            let mut timer = Ticker::every(Duration::from_secs(1));
            info!("Fuel gauge monitor started");

            fuel_gauge.lock().await.enable(&mut Delay).await;

            loop {
                let data = unwrap!(fuel_gauge.lock().await.read_data().await);

                {
                    let mut state = battery_state.lock().await;
                    state.data = Some(data);
                }
                debug!("Battery data: {:?}", data);

                timer.next().await;
            }
        })
        .await;

    fuel_gauge.lock().await.disable();
    info!("Monitor exited");
}
