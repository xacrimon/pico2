#![no_std]
#![feature(maybe_uninit_uninit_array, maybe_uninit_slice)]

mod book;
mod buffer;
mod grant;
mod wait;

pub use buffer::{Buffer, Ring};
pub use grant::{GrantRead, GrantWrite};
pub use wait::PollFn;

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

use {internal_unreachable as _unreachable, internal_unsafe_assert as _unsafe_assert};
