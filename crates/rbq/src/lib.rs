#![no_std]
#![feature(non_null_from_ref)]
//#![deny(missing_docs)]
//#![deny(warnings)]

pub mod book;
mod buffer;
mod framed;
mod vusize;
mod wait;

pub use buffer::{GrantRead, GrantWrite, RbQueue, SplitGrantRead};
pub use framed::{FrameGrantRead, FrameGrantWrite};

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
