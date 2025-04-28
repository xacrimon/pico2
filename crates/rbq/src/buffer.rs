use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::mem::MaybeUninit;

use critical_section::CriticalSection;
use embassy_sync::waitqueue::WakerRegistration;

use crate::Error;
use crate::book::Book;
use crate::grant::{GrantRead, GrantWrite, SplitGrantRead};

pub(crate) struct Dst<T: ?Sized> {
    pub(crate) book: Book,
    pub(crate) waker: WakerRegistration,
    pub(crate) buf: T,
}

pub struct Buffer<const N: usize> {
    dst: UnsafeCell<Dst<[MaybeUninit<u8>; N]>>,
}

impl<const N: usize> Buffer<N> {
    pub const fn new() -> Self {
        Self {
            dst: UnsafeCell::new(Dst {
                book: Book::new(),
                waker: WakerRegistration::new(),
                buf: MaybeUninit::uninit_array(),
            }),
        }
    }

    const fn dst(&self) -> *mut Dst<[MaybeUninit<u8>]> {
        let r1: &UnsafeCell<Dst<[MaybeUninit<u8>; N]>> = &self.dst;
        let r2: &UnsafeCell<Dst<[MaybeUninit<u8>]>> = r1;
        r2.get()
    }
}

unsafe impl<const N: usize> Send for Buffer<N> {}
unsafe impl<const N: usize> Sync for Buffer<N> {}

#[derive(Debug)]
pub struct Ring<'a> {
    dst: *mut Dst<[MaybeUninit<u8>]>,
    _marker: PhantomData<&'a ()>,
}

impl<'a> Ring<'a> {
    pub const fn new<const N: usize>(buffer: &'a Buffer<N>) -> Self {
        Self {
            dst: buffer.dst(),
            _marker: PhantomData,
        }
    }

    #[inline(never)]
    pub fn grant_exact(&self, cs: CriticalSection, size: usize) -> Result<GrantWrite, Error> {
        let dst = self._dst(cs);
        let capacity = dst.buf.len();
        let range = dst.book.acquire_write_exact(capacity, size)?;
        let grant = GrantWrite { ring: self, range };
        Ok(grant)
    }

    #[inline(never)]
    pub fn grant_max_remaining(&self, cs: CriticalSection) -> Result<GrantWrite, Error> {
        let dst = self._dst(cs);
        let capacity = dst.buf.len();
        let range = dst.book.acquire_write_remaining(capacity)?;
        let grant = GrantWrite { ring: self, range };
        Ok(grant)
    }

    #[inline(never)]
    pub fn read(&self, cs: CriticalSection) -> Result<GrantRead, Error> {
        let dst = self._dst(cs);
        let capacity = dst.buf.len();
        let range = dst.book.acquire_read(capacity)?;
        let grant = GrantRead { ring: self, range };
        Ok(grant)
    }

    #[inline(never)]
    pub fn split_read(&self, cs: CriticalSection) -> Result<SplitGrantRead, Error> {
        let dst = self._dst(cs);
        let capacity = dst.buf.len();
        let ranges = dst.book.acquire_read_split(capacity)?;
        let grant = SplitGrantRead { ring: self, ranges };
        Ok(grant)
    }
}

impl<'a> Ring<'a> {
    #[inline]
    pub(crate) fn _dst(&self, _cs: CriticalSection) -> &mut Dst<[MaybeUninit<u8>]> {
        unsafe { &mut *(self.dst) }
    }
}

unsafe impl Send for Ring<'_> {}
unsafe impl Sync for Ring<'_> {}
