use core::panic::PanicInfo;

// note: one can place a breakpoint on `rust_begin_unwind` to catch panics before they enter this loop.
#[inline(never)]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
