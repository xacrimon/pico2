#![no_std]
#![no_main]

mod gdb;
mod log;

use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_rp::peripherals::UART1;
use embassy_rp::uart::{Uart, UartTx};
use embassy_rp::{bind_interrupts, uart};

bind_interrupts!(pub struct Irqs {
    UART1_IRQ => uart::InterruptHandler<UART1>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("starting...");

    info!("initializing HAL");
    let p = embassy_rp::init(Default::default());

    info!("starting log sink worker using serial on pin 0...");
    let uart_tx = UartTx::new(p.UART0, p.PIN_0, p.DMA_CH0, uart::Config::default());
    unwrap!(spawner.spawn(log::to_serial(uart_tx)));

    info!("starting gdb io worker using serial on pin 4 and 5...");
    let uart = Uart::new(
        p.UART1,
        p.PIN_4,
        p.PIN_5,
        Irqs,
        p.DMA_CH1,
        p.DMA_CH2,
        uart::Config::default(),
    );
    unwrap!(spawner.spawn(gdb::bind_gdb_serial(uart)));

    info!("startup sequence finished");
}
