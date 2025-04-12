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
use embassy_rp::uart;
use embassy_rp::uart::UartTx;
use embassy_time::Timer;
use rbq::RbQueue;

static QUEUE: RbQueue<1024> = RbQueue::new();

fn enqueue_bytes(buf: &[u8], cs: CriticalSection) {
    let mut grant = QUEUE.grant_exact(buf.len(), cs).unwrap();
    grant.buf_mut().copy_from_slice(buf);
    grant.commit(buf.len(), cs);
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
async fn send_queue_uart(mut tx: UartTx<'static, UART0, uart::Async>) {
    loop {
        let Ok(grant) = critical_section::with(|cs| QUEUE.read(cs)) else {
            continue;
        };

        let size = grant.buf().len();
        tx.write(grant.buf()).await.unwrap();
        critical_section::with(|cs| grant.release(size, cs));
        Timer::after_millis(10).await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    println!("Hello, world!");

    let uart_tx = UartTx::new(p.UART0, p.PIN_0, p.DMA_CH0, uart::Config::default());
    spawner.spawn(send_queue_uart(uart_tx)).unwrap();
}
