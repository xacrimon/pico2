#![no_std]
#![no_main]

use defmt::println;
use embassy_executor::Spawner;
use embassy_rp::peripherals::UART0;
use embassy_rp::uart;
use embassy_rp::uart::UartTx;
use rbq::RbQueue;

static QUEUE: RbQueue<1024> = RbQueue::new();

#[defmt::global_logger]
struct Logger;

unsafe impl defmt::Logger for Logger {
    fn acquire() {}
    unsafe fn flush() {}
    unsafe fn release() {}

    unsafe fn write(buf: &[u8]) {
        critical_section::with(|cs| {
            let mut grant = QUEUE.grant_exact(buf.len(), cs).unwrap();
            grant.buf_mut().copy_from_slice(buf);
            grant.commit(buf.len(), cs);
            QUEUE.wake(cs);
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
        let grant = QUEUE.wait(|q, cs| q.read(cs).ok()).await;
        let size = grant.buf().len();
        tx.write(grant.buf()).await.unwrap();
        critical_section::with(|cs| grant.release(size, cs));
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    println!("Hello, world!");

    let p = embassy_rp::init(Default::default());

    let uart_tx = UartTx::new(p.UART0, p.PIN_0, p.DMA_CH0, uart::Config::default());
    spawner.spawn(send_queue_uart(uart_tx)).unwrap();
}
