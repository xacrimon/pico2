#![no_std]
#![no_main]

mod log;

use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_rp::peripherals::UART1;
use embassy_rp::uart::UartTx;
use embassy_rp::{bind_interrupts, uart};

bind_interrupts!(pub struct Irqs {
    UART1_IRQ => uart::BufferedInterruptHandler<UART1>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("starting...");

    info!("initializing HAL");
    let p = embassy_rp::init(Default::default());

    info!("starting log sink worker...");
    let uart_tx = UartTx::new(p.UART0, p.PIN_0, p.DMA_CH0, uart::Config::default());
    unwrap!(spawner.spawn(log::to_serial(uart_tx)));

    info!("startup sequence finished");
}
