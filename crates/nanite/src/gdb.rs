use defmt::{debug, error, info, unwrap};
use embassy_rp::peripherals::UART1;
use embassy_rp::uart;
use embassy_rp::uart::{Uart, UartRx, UartTx};
use futures_util::{FutureExt, select_biased};
use rbq::RbQueue;
use scopeguard::defer;

static TX: RbQueue<1024> = RbQueue::new();
static RX: RbQueue<1024> = RbQueue::new();

#[embassy_executor::task]
pub async fn bind_gdb_serial(uart: Uart<'static, UART1, uart::Async>) {
    let (mut tx, mut rx) = uart.split();
    defer! { info!("bind_gdb_serial stopping due to earlier failure..."); }

    select_biased! {
        _ = drive_gdb_serial_tx(&mut tx).fuse() => {
            error!("drive_gdb_serial_tx quit unexpectedly");
        }
        _ = drive_gdb_serial_rx(&mut rx).fuse() => {
            error!("drive_gdb_serial_rx quit unexpectedly");
        }
    }
}

async fn drive_gdb_serial_tx(uart: &mut UartTx<'static, UART1, uart::Async>) {
    loop {
        let grant = TX.wait(|q, cs| q.read(cs).ok()).await;
        let size = grant.buf().len();
        debug!(
            "processing chunk with size {} bytes in transmit buffer...",
            size
        );

        unwrap!(uart.write(grant.buf()).await);
        debug!("chunk transmitted, releasing grant...");
        critical_section::with(|cs| grant.release(size, cs));
    }
}

async fn drive_gdb_serial_rx(uart: &mut UartRx<'static, UART1, uart::Async>) {
    loop {
        let mut grant = RX.wait(|q, cs| q.grant_max_remaining(cs).ok()).await;
        let size = grant.buf().len();
        debug!("obtained grant with size {} in receive buffer", size);

        unwrap!(uart.read(grant.buf_mut()).await);
        debug!("read data into grant, committing write...");
        critical_section::with(|cs| grant.commit(size, cs));
    }
}
