#![no_std]
#![no_main]

mod log;

use defmt::info;
use embassy_executor::Spawner;
use embassy_rp::uart;
use embassy_rp::uart::UartTx;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("starting...");

    let p = embassy_rp::init(Default::default());

    let uart_tx = UartTx::new(p.UART0, p.PIN_0, p.DMA_CH0, uart::Config::default());
    spawner.spawn(log::to_serial(uart_tx)).unwrap();
}
