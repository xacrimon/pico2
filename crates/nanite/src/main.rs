//! This example test the RP Pico on board LED.
//!
//! It does not work with the RP Pico W board. See wifi_blinky.rs.

#![no_std]
#![no_main]

use core::cell::RefCell;

use critical_section::{CriticalSection, Mutex};
use defmt::println;
use embassy_executor::Spawner;
use embassy_rp::peripherals::UART0;
use embassy_rp::uart::{Blocking, BufferedInterruptHandler};
use embassy_rp::{bind_interrupts, uart};
use embassy_time::Timer;
use embedded_io_async::Write;
use rbq::RbQueue;

bind_interrupts!(struct Irqs {
    UART0_IRQ => BufferedInterruptHandler<UART0>;
});

static QUEUE: RbQueue<1024> = RbQueue::new();

fn enqueue_bytes(buf: &[u8], cs: CriticalSection) {
    let mut grant = QUEUE.grant_exact(buf.len(), cs).unwrap();
    grant.buf_mut().copy_from_slice(buf);
}

static ENCODER: Mutex<RefCell<defmt::Encoder>> = Mutex::new(RefCell::new(defmt::Encoder::new()));

#[defmt::global_logger]
struct Logger;

unsafe impl defmt::Logger for Logger {
    fn acquire() {
        critical_section::with(|cs| {
            let mut encoder = ENCODER.borrow_ref_mut(cs);
            encoder.start_frame(|buf| enqueue_bytes(buf, cs));
        });
    }

    unsafe fn flush() {}

    unsafe fn release() {
        critical_section::with(|cs| {
            let mut encoder = ENCODER.borrow_ref_mut(cs);
            encoder.end_frame(|buf| enqueue_bytes(buf, cs));
        });
    }

    unsafe fn write(bytes: &[u8]) {
        critical_section::with(|cs| {
            let mut encoder = ENCODER.borrow_ref_mut(cs);
            encoder.write(bytes, |buf| enqueue_bytes(buf, cs));
        });
    }
}

#[defmt::panic_handler]
fn defmt_panic() -> ! {
    loop {}
}

#[panic_handler]
fn core_panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[embassy_executor::task]
async fn send_queue_uart(uart: uart::Uart<'static, UART0, Blocking>) {
    let mut tx_buffer = [0u8; 16];
    let mut rx_buffer = [0u8; 16];
    let mut uart = uart.into_buffered(Irqs, &mut tx_buffer, &mut rx_buffer);

    loop {
        let Ok(grant) = critical_section::with(|cs| QUEUE.read(cs)) else {
            continue;
        };

        let n = uart.write(grant.buf()).await.unwrap();
        critical_section::with(|cs| grant.release(n, cs));
        Timer::after_millis(100).await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    println!("Hello, world!");

    let config = uart::Config::default();
    let uart =
        uart::Uart::new_with_rtscts_blocking(p.UART0, p.PIN_0, p.PIN_1, p.PIN_3, p.PIN_2, config);

    spawner.spawn(send_queue_uart(uart)).unwrap();
}
