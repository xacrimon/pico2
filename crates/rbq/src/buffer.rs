use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::mem::{self, MaybeUninit};
use core::ptr::NonNull;
use core::{cmp, slice};

use bitflags::bitflags;
use rp235x_hal::sio::{Spinlock, SpinlockValid};

use crate::Error;

bitflags! {
    #[derive(Debug, Clone, Copy)]
    struct Flags: u8 {
        const READ_IN_PROGRESS = 1 << 0;
        const WRITE_IN_PROGRESS = 1 << 1;
        const ALREADY_SPLIT = 1 << 2;
    }
}

#[derive(Debug)]
struct RbqBuffer<const N: usize> {
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

    flags: Flags,
}

#[derive(Debug)]
pub struct RbQueue<const N: usize, const S: usize> {
    inner: UnsafeCell<RbqBuffer<N>>,
}

unsafe impl<const N: usize, const S: usize> Sync for RbQueue<N, S> {}

impl<const N: usize, const S: usize> RbQueue<N, S>
where
    Spinlock<S>: SpinlockValid,
{
    unsafe fn inner_ptr(&self) -> NonNull<RbqBuffer<N>> {
        unsafe { NonNull::new_unchecked(self.inner.get()) }
    }

    #[allow(clippy::mut_from_ref)]
    unsafe fn inner_ref(&self) -> &mut RbqBuffer<N> {
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
                flags: Flags::empty(),
            }),
        }
    }

    pub fn grant_exact(&self, sz: usize, _guard: &Spinlock<S>) -> Result<GrantWrite<N, S>, Error> {
        let inner = unsafe { self.inner_ref() };

        if inner.flags.contains(Flags::WRITE_IN_PROGRESS) {
            return Err(Error::GrantInProgress);
        } else {
            inner.flags.insert(Flags::WRITE_IN_PROGRESS);
        }

        let max = N;
        let inverted = inner.write < inner.read;

        let start = match () {
            // inverted, room is still available
            _ if inverted && (inner.write + sz) < inner.read => inner.write,
            // inverted, no room is available
            _ if inverted && (inner.write + sz) >= inner.read => {
                inner.flags.remove(Flags::WRITE_IN_PROGRESS);
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
                    inner.flags.remove(Flags::WRITE_IN_PROGRESS);
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

    pub fn grant_max_remaining(&self, _guard: &Spinlock<S>) -> Result<(), Error> {
        todo!()
    }

    pub fn read(&self, _guard: &Spinlock<S>) -> Result<GrantRead<N, S>, Error> {
        let inner = unsafe { self.inner_ref() };

        if inner.flags.contains(Flags::READ_IN_PROGRESS) {
            return Err(Error::GrantInProgress);
        } else {
            inner.flags.insert(Flags::READ_IN_PROGRESS);
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

    pub fn split_read(&self, _guard: &Spinlock<S>) -> Result<SplitGrantRead<N, S>, Error> {
        todo!()
    }
}

impl<const N: usize, const S: usize> Default for RbQueue<N, S>
where
    Spinlock<S>: SpinlockValid,
{
    fn default() -> Self {
        Self::new()
    }
}

#[must_use]
#[derive(Debug)]
pub struct GrantWrite<'a, const N: usize, const S: usize> {
    rbq: NonNull<RbqBuffer<N>>,
    buf: NonNull<[u8]>,
    pd: PhantomData<&'a mut [u8]>,
}

unsafe impl<const N: usize, const S: usize> Send for GrantWrite<'_, N, S> {}

impl<const N: usize, const S: usize> GrantWrite<'_, N, S>
where
    Spinlock<S>: SpinlockValid,
{
    pub fn commit(mut self, used: usize) {
        unsafe {
            self.commit_inner(used);
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

    pub(crate) unsafe fn commit_inner(&mut self, used: usize) {
        let inner = unsafe { &mut *self.rbq.as_ptr() };

        // if there is no grant in progress, return early. This
        // generally means we are dropping the grant within a
        // wrapper structure
        if !inner.flags.contains(Flags::WRITE_IN_PROGRESS) {
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
        inner.flags.remove(Flags::WRITE_IN_PROGRESS);
    }
}

impl<const N: usize, const S: usize> Drop for GrantWrite<'_, N, S> {
    fn drop(&mut self) {
        panic!();
    }
}

#[must_use]
#[derive(Debug)]
pub struct GrantRead<'a, const N: usize, const S: usize> {
    rbq: NonNull<RbqBuffer<N>>,
    buf: NonNull<[u8]>,
    pd: PhantomData<&'a mut [u8]>,
}

unsafe impl<const N: usize, const S: usize> Send for GrantRead<'_, N, S> {}

impl<const N: usize, const S: usize> GrantRead<'_, N, S>
where
    Spinlock<S>: SpinlockValid,
{
    pub fn release(self, used: usize, _guard: &Spinlock<S>) {
        let used = cmp::min(self.buf.len(), used);
        unsafe {
            self.release_inner(used);
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

    pub(crate) unsafe fn release_inner(&self, used: usize) {
        let inner = unsafe { &mut *self.rbq.as_ptr() };

        // if there is no grant in progress, return early. This
        // generally means we are dropping the grant within a
        // wrapper structure
        if !inner.flags.contains(Flags::READ_IN_PROGRESS) {
            return;
        }

        // This should always be checked by the public interfaces
        debug_assert!(used <= self.buf.len());

        inner.read += used;
        inner.flags.remove(Flags::READ_IN_PROGRESS);
    }
}

impl<const N: usize, const S: usize> Drop for GrantRead<'_, N, S> {
    fn drop(&mut self) {
        panic!();
    }
}

#[must_use]
#[derive(Debug)]
pub struct SplitGrantRead<'a, const N: usize, const S: usize> {
    rbq: NonNull<RbqBuffer<N>>,
    buf1: NonNull<[u8]>,
    buf2: NonNull<[u8]>,
    pd: PhantomData<&'a mut [u8]>,
}

unsafe impl<const N: usize, const S: usize> Send for SplitGrantRead<'_, N, S> {}

impl<const N: usize, const S: usize> SplitGrantRead<'_, N, S>
where
    Spinlock<S>: SpinlockValid,
{
    pub fn release(self, used: usize, _guard: &Spinlock<S>) {
        let used = cmp::min(self.combined_len(), used);
        unsafe {
            self.release_inner(used);
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

    pub(crate) unsafe fn release_inner(&self, used: usize) {
        let inner = unsafe { &mut *self.rbq.as_ptr() };

        // if there is no grant in progress, return early. This
        // generally means we are dropping the grant within a
        // wrapper structure
        if !inner.flags.contains(Flags::READ_IN_PROGRESS) {
            return;
        }

        // This should always be checked by the public interfaces
        debug_assert!(used <= self.combined_len());

        if used <= self.buf1.len() {
            inner.read += used;
        } else {
            inner.read = used - self.buf1.len();
        }

        inner.flags.remove(Flags::READ_IN_PROGRESS);
    }

    pub fn combined_len(&self) -> usize {
        self.buf1.len() + self.buf2.len()
    }
}

impl<const N: usize, const S: usize> Drop for SplitGrantRead<'_, N, S> {
    fn drop(&mut self) {
        panic!();
    }
}
