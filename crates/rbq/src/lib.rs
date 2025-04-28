#![no_std]
#![feature(non_null_from_ref)]
#![feature(maybe_uninit_as_bytes)]
#![feature(maybe_uninit_uninit_array, maybe_uninit_slice)]
//#![deny(missing_docs)]
//#![deny(warnings)]

mod book;
mod buffer;
mod grant;
mod vusize;
mod wait;

pub use buffer::{Buffer, Ring};
pub use grant::{GrantRead, GrantWrite, SplitGrantRead};
pub use wait::PollFn;
//pub use framed::{FrameGrantRead, FrameGrantWrite};

#[derive(defmt::Format)]
pub enum Error {
    GrantInProgress,
    InsufficientSize,
}

macro_rules! internal_unreachable {
    () => {{
        #[cfg(debug_assertions)]
        {
            unreachable!();
        }

        #[cfg(not(debug_assertions))]
        unsafe {
            core::hint::unreachable_unchecked();
        }
    }};
}

macro_rules! internal_unsafe_assert {
    ($cond:expr) => {{
        #[cfg(debug_assertions)]
        {
            assert!($cond);
        }

        #[cfg(not(debug_assertions))]
        unsafe {
            if !$cond {
                core::hint::unreachable_unchecked();
            }
        }
    }};
}

macro_rules! internal_cold {
    ($t:ty, $e:expr) => {{
        #[cold]
        fn _cold_fn() -> $t {
            $e
        }

        return _cold_fn();
    }};
}

use {
    internal_cold as _cold, internal_unreachable as _unreachable,
    internal_unsafe_assert as _unsafe_assert,
};
