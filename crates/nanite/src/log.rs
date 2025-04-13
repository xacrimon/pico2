use defmt::{error, unwrap};
use embassy_rp::peripherals::UART0;
use embassy_rp::uart;
use embassy_rp::uart::UartTx;
use rbq::RbQueue;

static TX_QUEUE: RbQueue<1024> = RbQueue::new();

#[defmt::global_logger]
struct Logger;

unsafe impl defmt::Logger for Logger {
    fn acquire() {}
    unsafe fn flush() {}
    unsafe fn release() {}

    unsafe fn write(buf: &[u8]) {
        critical_section::with(|cs| {
            let Ok(mut grant) = TX_QUEUE.grant_exact(buf.len(), cs) else {
                return;
            };

            grant.buf_mut().copy_from_slice(buf);
            grant.commit(buf.len(), cs);
            TX_QUEUE.wake(cs);
        });
    }
}

#[defmt::panic_handler]
fn defmt_panic() -> ! {
    loop {}
}

#[panic_handler]
fn core_panic(info: &core::panic::PanicInfo) -> ! {
    error!("core panic: {:?}", info);
    loop {}
}

#[embassy_executor::task]
pub async fn to_serial(mut tx: UartTx<'static, UART0, uart::Async>) {
    loop {
        let grant = TX_QUEUE.wait(|q, cs| q.read(cs).ok()).await;
        let size = grant.buf().len();
        unwrap!(tx.write(grant.buf()).await);
        critical_section::with(|cs| grant.release(size, cs));
    }
}
