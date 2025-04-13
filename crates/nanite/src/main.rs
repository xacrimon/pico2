#![no_std]
#![no_main]

use defmt::{println,unwrap};
use embassy_executor::Spawner;
use embassy_rp::peripherals::UART0;
use embassy_rp::uart;
use embassy_rp::uart::UartTx;
use rbq::RbQueue;
use embassy_rp::peripherals::{DMA_CH0,DMA_CH1, PIO0};
use embassy_rp::bind_interrupts;
use embassy_rp::pio::{self, Pio};
use embassy_rp::gpio;
use rand::RngCore;
use embassy_rp::clocks::RoscRng;
use embedded_io_async::Write;
use static_cell::StaticCell;
use embassy_time::Timer;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => pio::InterruptHandler<PIO0>;
});

const WIFI_NETWORK: &str = "Floppy-IoT";
const WIFI_PASSWORD: &str = "c$WWUih1";

#[embassy_executor::task]
async fn cyw43_task(runner: cyw43::Runner<'static, gpio::Output<'static>, cyw43_pio::PioSpi<'static, PIO0, 0, DMA_CH1>>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}

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
    let mut rng = RoscRng;

    let uart_tx = UartTx::new(p.UART0, p.PIN_0, p.DMA_CH0, uart::Config::default());
    spawner.spawn(send_queue_uart(uart_tx)).unwrap();

    let fw = include_bytes!("../../../43439A0.bin");
    let clm = include_bytes!("../../../43439A0_clm.bin");

    let pwr = gpio::Output::new(p.PIN_23, gpio::Level::Low);
    let cs = gpio::Output::new(p.PIN_25, gpio::Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);
    let spi = cyw43_pio::PioSpi::new(
        &mut pio.common,
        pio.sm0,
        cyw43_pio::DEFAULT_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        p.PIN_24,
        p.PIN_29,
        p.DMA_CH1,
    );

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
    unwrap!(spawner.spawn(cyw43_task(runner)));

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    let config = embassy_net::Config::dhcpv4(Default::default());

    let seed = rng.next_u64();

    static RESOURCES: StaticCell<embassy_net::StackResources<3>> = StaticCell::new();
    let (stack, runner) = embassy_net::new(net_device, config, RESOURCES.init(embassy_net::StackResources::new()), seed);

    unwrap!(spawner.spawn(net_task(runner)));

    loop {
        match control
            .join(WIFI_NETWORK, cyw43::JoinOptions::new(WIFI_PASSWORD.as_bytes()))
            .await
        {
            Ok(_) => break,
            Err(err) => {
                println!("join failed with status={}", err.status);
            }
        }
    }

    println!("waiting for DHCP...");
    while !stack.is_config_up() {
        Timer::after_millis(100).await;
    }
    println!("DHCP is now up!");
}
