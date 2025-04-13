use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::mem::{self, MaybeUninit};
use core::ptr::NonNull;
use core::{cmp, slice};

use critical_section::CriticalSection;
use embassy_sync::waitqueue::WakerRegistration;

use crate::Error;

#[derive(Debug)]
pub(crate) struct RbqBuffer<const N: usize> {
    buf: MaybeUninit<[u8; N]>,

    // where the next byte will be written
    write: usize,

    // where the next byte will be read
    last: usize,

    // when inverted, marks the last valid position in the high half of the buffer
    // when it is not fully filled.
    read: usize,

    // used by the writer to remember what bytes are allowed to be written to, but are not yet ready to be read from
    reserve: usize,

    read_in_progress: bool,
    write_in_progress: bool,

    pub(crate) waker: WakerRegistration,
}

#[derive(Debug)]
pub struct RbQueue<const N: usize> {
    inner: UnsafeCell<RbqBuffer<N>>,
}

unsafe impl<const N: usize> Sync for RbQueue<N> {}

impl<const N: usize> RbQueue<N> {
    unsafe fn inner_ptr(&self) -> NonNull<RbqBuffer<N>> {
        unsafe { NonNull::new_unchecked(self.inner.get()) }
    }

    #[allow(clippy::mut_from_ref)]
    pub(crate) unsafe fn inner_ref(&self) -> &mut RbqBuffer<N> {
        unsafe { &mut *self.inner.get() }
    }

    pub const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(RbqBuffer {
                buf: MaybeUninit::uninit(),
                write: 0,
                last: 0,
                read: 0,
                reserve: 0,
                read_in_progress: false,
                write_in_progress: false,
                waker: WakerRegistration::new(),
            }),
        }
    }

    pub fn grant_exact(&self, sz: usize, _cs: CriticalSection) -> Result<GrantWrite<N>, Error> {
        let inner = unsafe { self.inner_ref() };

        if inner.write_in_progress {
            return Err(Error::GrantInProgress);
        } else {
            inner.write_in_progress = true;
        }

        let max = N;
        let inverted = inner.write < inner.read;

        let start = match () {
            // inverted, room is still available
            _ if inverted && (inner.write + sz) < inner.read => inner.write,
            // inverted, no room is available
            _ if inverted && (inner.write + sz) >= inner.read => {
                inner.write_in_progress = false;
                return Err(Error::InsufficientSize);
            }
            // non inverted condition
            _ if !inverted && inner.write + sz <= max => inner.write,
            // not inverted, but need to invert
            _ if !inverted && inner.write + sz > max => {
                // note: we check sz < read, not <=, because
                // write must never == read in an inverted condition, since
                // we will then not be able to tell if we are inverted or not
                if sz < inner.read {
                    // invertible situation
                    0
                } else {
                    // not invertible, no space
                    inner.write_in_progress = false;
                    return Err(Error::InsufficientSize);
                }
            }
            _ => unreachable!(),
        };

        inner.reserve = start + sz;

        let start_of_buf_ptr = inner.buf.as_mut_ptr().cast::<u8>();
        let grant_slice = unsafe { slice::from_raw_parts_mut(start_of_buf_ptr.add(start), sz) };

        let ptr = unsafe { self.inner_ptr() };
        Ok(GrantWrite {
            rbq: ptr,
            buf: grant_slice.into(),
            pd: PhantomData,
        })
    }

    pub fn grant_max_remaining(&self, _cs: CriticalSection) -> Result<(), Error> {
        todo!()
    }

    pub fn read(&self, _cs: CriticalSection) -> Result<GrantRead<N>, Error> {
        let inner = unsafe { self.inner_ref() };

        if inner.read_in_progress {
            return Err(Error::GrantInProgress);
        } else {
            inner.read_in_progress = true;
        }

        // untangle the inversion by moving back read
        if (inner.read == inner.last) && (inner.write < inner.read) {
            inner.read = 0;
        }

        // either there's nothing to read, we're in normal form, or inverted
        let sz = match () {
            _ if inner.write == inner.read => return Err(Error::InsufficientSize),
            _ if inner.write > inner.read => inner.write - inner.read,
            _ if inner.write < inner.read => (N - inner.read) + inner.write,
            _ => unreachable!(),
        };

        let start_of_buf_ptr = inner.buf.as_mut_ptr().cast::<u8>();
        let grant_slice =
            unsafe { slice::from_raw_parts_mut(start_of_buf_ptr.add(inner.read), sz) };

        let ptr = unsafe { self.inner_ptr() };
        Ok(GrantRead {
            rbq: ptr,
            buf: grant_slice.into(),
            pd: PhantomData,
        })
    }

    pub fn split_read(&self, _cs: CriticalSection) -> Result<SplitGrantRead<N>, Error> {
        todo!()
    }
}

impl<const N: usize> Default for RbQueue<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[must_use]
#[derive(Debug)]
pub struct GrantWrite<'a, const N: usize> {
    rbq: NonNull<RbqBuffer<N>>,
    buf: NonNull<[u8]>,
    pd: PhantomData<&'a mut [u8]>,
}

unsafe impl<const N: usize> Send for GrantWrite<'_, N> {}

impl<const N: usize> GrantWrite<'_, N> {
    pub fn commit(mut self, used: usize, cs: CriticalSection) {
        unsafe {
            self.commit_inner(used, cs);
        }

        mem::forget(self);
    }

    pub fn buf(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.buf.as_ptr() as *const u8, self.buf.len()) }
    }

    pub fn buf_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.buf.as_ptr() as *mut u8, self.buf.len()) }
    }

    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn as_static_mut_buf(&mut self) -> &'static mut [u8] {
        unsafe { mem::transmute::<&mut [u8], &'static mut [u8]>(self.buf_mut()) }
    }

    pub(crate) unsafe fn commit_inner(&mut self, used: usize, _cs: CriticalSection) {
        let inner = unsafe { &mut *self.rbq.as_ptr() };

        // if there is no grant in progress, return early. This
        // generally means we are dropping the grant within a
        // wrapper structure
        if !inner.write_in_progress {
            return;
        }

        // saturate the grant commit
        let len = self.buf.len();
        let used = cmp::min(len, used);

        inner.reserve -= len - used;

        let max = N;
        let new_write = inner.reserve;

        match () {
            // We have already wrapped, but we are skipping some bytes at the end of the ring.
            // Mark `last` where the write pointer used to be to hold the line here
            _ if (new_write < inner.write) && (inner.write != max) => {
                inner.last = inner.write;
            }
            _ if new_write > inner.last => {
                // We're about to pass the last pointer, which was previously the artificial
                // end of the ring. Now that we've passed it, we can "unlock" the section
                // that was previously skipped.
                //
                // Since new_write is strictly larger than last, it is safe to move this as
                // the other thread will still be halted by the (about to be updated) write
                // value.
                inner.last = max;
            }
            // else: If new_write == last, either:
            // * last == max, so no need to write, OR
            // * If we write in the end chunk again, we'll update last to max next time
            // * If we write to the start chunk in a wrap, we'll update last when we
            //     move write backwards
            _ => {}
        }

        inner.write = new_write;
        inner.write_in_progress = false;
    }
}

impl<const N: usize> Drop for GrantWrite<'_, N> {
    fn drop(&mut self) {
        critical_section::with(|cs| unsafe { self.commit_inner(0, cs) });
    }
}

#[must_use]
#[derive(Debug)]
pub struct GrantRead<'a, const N: usize> {
    rbq: NonNull<RbqBuffer<N>>,
    buf: NonNull<[u8]>,
    pd: PhantomData<&'a mut [u8]>,
}

unsafe impl<const N: usize> Send for GrantRead<'_, N> {}

impl<const N: usize> GrantRead<'_, N> {
    pub fn release(self, used: usize, cs: CriticalSection) {
        let used = cmp::min(self.buf.len(), used);
        unsafe {
            self.release_inner(used, cs);
        }

        mem::forget(self);
    }

    pub(crate) fn shrink(&mut self, len: usize) {
        let mut new_buf: &mut [u8] = &mut [];
        core::mem::swap(&mut self.buf_mut(), &mut new_buf);
        let (new, _) = new_buf.split_at_mut(len);
        self.buf = new.into();
    }

    pub fn buf(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.buf.as_ptr() as *const u8, self.buf.len()) }
    }

    pub fn buf_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.buf.as_ptr() as *mut u8, self.buf.len()) }
    }

    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn as_static_buf(&self) -> &'static [u8] {
        unsafe { mem::transmute::<&[u8], &'static [u8]>(self.buf()) }
    }

    pub(crate) unsafe fn release_inner(&self, used: usize, _cs: CriticalSection) {
        let inner = unsafe { &mut *self.rbq.as_ptr() };

        // if there is no grant in progress, return early. This
        // generally means we are dropping the grant within a
        // wrapper structure
        if !inner.read_in_progress {
            return;
        }

        // This should always be checked by the public interfaces
        debug_assert!(used <= self.buf.len());

        inner.read += used;
        inner.read_in_progress = false;
    }
}

impl<const N: usize> Drop for GrantRead<'_, N> {
    fn drop(&mut self) {
        critical_section::with(|cs: CriticalSection<'_>| unsafe { self.release_inner(0, cs) });
    }
}

#[must_use]
#[derive(Debug)]
pub struct SplitGrantRead<'a, const N: usize> {
    rbq: NonNull<RbqBuffer<N>>,
    buf1: NonNull<[u8]>,
    buf2: NonNull<[u8]>,
    pd: PhantomData<&'a mut [u8]>,
}

unsafe impl<const N: usize> Send for SplitGrantRead<'_, N> {}

impl<const N: usize> SplitGrantRead<'_, N> {
    pub fn release(self, used: usize, cs: CriticalSection) {
        let used = cmp::min(self.combined_len(), used);
        unsafe {
            self.release_inner(used, cs);
        }

        mem::forget(self);
    }

    pub fn bufs(&self) -> (&[u8], &[u8]) {
        let buf1 =
            unsafe { slice::from_raw_parts(self.buf1.as_ptr() as *const u8, self.buf1.len()) };
        let buf2 =
            unsafe { slice::from_raw_parts(self.buf2.as_ptr() as *const u8, self.buf2.len()) };

        (buf1, buf2)
    }

    pub fn bufs_mut(&mut self) -> (&mut [u8], &mut [u8]) {
        let buf1 =
            unsafe { slice::from_raw_parts_mut(self.buf1.as_ptr() as *mut u8, self.buf1.len()) };
        let buf2 =
            unsafe { slice::from_raw_parts_mut(self.buf2.as_ptr() as *mut u8, self.buf2.len()) };

        (buf1, buf2)
    }

    pub(crate) unsafe fn release_inner(&self, used: usize, _cs: CriticalSection) {
        let inner = unsafe { &mut *self.rbq.as_ptr() };

        // if there is no grant in progress, return early. This
        // generally means we are dropping the grant within a
        // wrapper structure
        if !inner.read_in_progress {
            return;
        }

        // This should always be checked by the public interfaces
        debug_assert!(used <= self.combined_len());

        if used <= self.buf1.len() {
            inner.read += used;
        } else {
            inner.read = used - self.buf1.len();
        }

        inner.read_in_progress = false;
    }

    pub fn combined_len(&self) -> usize {
        self.buf1.len() + self.buf2.len()
    }
}

impl<const N: usize> Drop for SplitGrantRead<'_, N> {
    fn drop(&mut self) {
        critical_section::with(|cs: CriticalSection<'_>| unsafe { self.release_inner(0, cs) });
    }
}
