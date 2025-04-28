use defmt::{error, unwrap};
use embassy_rp::peripherals::UART0;
use embassy_rp::uart;
use embassy_rp::uart::UartTx;

static TX_BUF: rbq::Buffer<1024> = rbq::Buffer::new();
static TX_QUEUE: rbq::Ring<'static> = rbq::Ring::new(&TX_BUF);

#[defmt::global_logger]
struct Logger;

unsafe impl defmt::Logger for Logger {
    fn acquire() {}
    unsafe fn flush() {}
    unsafe fn release() {}

    unsafe fn write(buf: &[u8]) {
        critical_section::with(|cs| {
            let Ok(mut grant) = TX_QUEUE.grant_exact(cs, buf.len()) else {
                return;
            };

            grant.buf_mut().copy_from_slice(buf);
            grant.commit(cs, buf.len());
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
        let grant = TX_QUEUE.poll(|q, cs| q.read(cs).ok()).await;
        let size = grant.buf().len();
        unwrap!(tx.write(grant.buf()).await);
        critical_section::with(|cs| grant.commit(cs, size));
    }
}
