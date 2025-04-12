#![no_std]
#![feature(non_null_from_ref)]
//#![deny(missing_docs)]
//#![deny(warnings)]

mod buffer;
mod framed;
mod vusize;

pub use buffer::{GrantRead, GrantWrite, RbQueue, SplitGrantRead};
pub use framed::{FrameGrantRead, FrameGrantWrite};

pub enum Error {
    GrantInProgress,
    InsufficientSize,
}
