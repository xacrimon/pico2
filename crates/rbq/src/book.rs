use core::num::NonZeroUsize;
use core::ops::Range;

use crate::{_unreachable, _unsafe_assert, Error};

#[derive(Debug, Clone, Copy)]
pub(super) struct GrantRange {
    start: usize,
    len: NonZeroUsize,
}

impl GrantRange {
    #[inline]
    fn from_range(range: Range<usize>) -> Self {
        _unsafe_assert!(range.start < range.end);

        let start = range.start;
        let len = range.end - range.start;

        unsafe {
            Self {
                start,
                len: NonZeroUsize::new_unchecked(len),
            }
        }
    }

    #[inline]
    pub(super) fn to_range(self) -> Range<usize> {
        self.start..(self.start + self.len.get())
    }

    #[inline]
    pub(super) fn to_len(self) -> usize {
        self.len.get()
    }
}

#[cold]
fn err_in_progress() -> Result<(), Error> {
    Err(Error::GrantInProgress)
}

#[derive(Debug, Clone)]
pub(super) struct Book {
    // where the next byte will be written
    write: usize,

    // where the next byte will be read
    last: usize,

    // when inverted, marks the last valid position in the high half of the buffer
    // when it is not fully filled.
    read: usize,

    // used by the writer to remember what bytes are allowed to be written to, but are not yet ready to be read from
    reserve: usize,

    // enforce spsc
    read_in_progress: bool,
    write_in_progress: bool,
}

impl Book {
    #[inline]
    fn sm_acq_write(&mut self) -> Result<(), Error> {
        if !self.write_in_progress {
            self.write_in_progress = true;
            Ok(())
        } else {
            err_in_progress()
        }
    }

    #[inline]
    fn sm_rel_write(&mut self) {
        _unsafe_assert!(self.write_in_progress);
        self.write_in_progress = false;
    }

    #[inline]
    fn sm_acq_read(&mut self) -> Result<(), Error> {
        if !self.read_in_progress {
            self.read_in_progress = true;
            Ok(())
        } else {
            err_in_progress()
        }
    }

    #[inline]
    fn sm_rel_read(&mut self) {
        _unsafe_assert!(self.read_in_progress);
        self.read_in_progress = false;
    }

    pub(super) const fn new() -> Self {
        Self {
            write: 0,
            last: 0,
            read: 0,
            reserve: 0,
            read_in_progress: false,
            write_in_progress: false,
        }
    }

    #[inline]
    pub(super) fn release_write(&mut self) {
        self.sm_rel_write();
    }

    #[inline]
    pub(super) fn release_read(&mut self) {
        self.sm_rel_read();
    }

    #[inline]
    pub(super) fn acquire_write_exact(
        &mut self,
        capacity: usize,
        size: usize,
    ) -> Result<GrantRange, Error> {
        self.sm_acq_write()?;

        let max = capacity - 1; // TODO: should it be minus 1?
        let inverted = self.write < self.read;

        let start = match () {
            // inverted, room is still available
            _ if inverted && (self.write + size) < self.read => self.write,
            // inverted, no room is available
            _ if inverted && (self.write + size) >= self.read => {
                self.sm_rel_write();
                return Err(Error::InsufficientSize);
            }
            // non inverted condition
            _ if !inverted && self.write + size <= max => self.write,
            // not inverted, but need to invert
            _ if !inverted && self.write + size > max => {
                // note: we check sz < read, not <=, because
                // write must never == read in an inverted condition, since
                // we will then not be able to tell if we are inverted or not
                if size < self.read {
                    // invertible situation
                    0
                } else {
                    // not invertible, no space
                    self.sm_rel_write();
                    return Err(Error::InsufficientSize);
                }
            }
            _ => _unreachable!(),
        };

        self.reserve = start + size;
        let grant_range = start..(start + size);
        Ok(GrantRange::from_range(grant_range))
    }

    #[inline]
    pub(super) fn commit_write_exact(&mut self, capacity: usize, size: usize, used: usize) {
        _unsafe_assert!(used <= size);

        // saturate the grant commit
        let len = size;
        //let used = cmp::min(len, used);

        self.reserve -= len - used;

        let max = capacity - 1; // TODO: should it be minus 1?
        let new_write = self.reserve;

        match () {
            // We have already wrapped, but we are skipping some bytes at the end of the ring.
            // Mark `last` where the write pointer used to be to hold the line here
            _ if (new_write < self.write) && (self.write != max) => {
                self.last = self.write;
            }
            _ if new_write > self.last => {
                // We're about to pass the last pointer, which was previously the artificial
                // end of the ring. Now that we've passed it, we can "unlock" the section
                // that was previously skipped.
                //
                // Since new_write is strictly larger than last, it is safe to move this as
                // the other thread will still be halted by the (about to be updated) write
                // value.
                self.last = max;
            }
            // else: If new_write == last, either:
            // * last == max, so no need to write, OR
            // * If we write in the end chunk again, we'll update last to max next time
            // * If we write to the start chunk in a wrap, we'll update last when we
            //     move write backwards
            _ => {}
        }

        self.write = new_write;
        self.sm_rel_write();
    }

    #[inline]
    pub(super) fn acquire_write_remaining(
        &mut self,
        _capacity: usize,
    ) -> Result<GrantRange, Error> {
        unimplemented!()
    }

    #[inline]
    pub(super) fn acquire_read(&mut self, capacity: usize) -> Result<GrantRange, Error> {
        self.sm_acq_read()?;

        // untangle the inversion by moving back read
        if (self.read == self.last) && (self.write < self.read) {
            self.read = 0;
        }

        // either there's nothing to read, we're in normal form, or inverted
        let sz = match () {
            _ if self.write == self.read => {
                self.sm_rel_read();
                return Err(Error::InsufficientSize);
            }
            _ if self.write > self.read => self.write - self.read,
            _ if self.write < self.read => (capacity - self.read) + self.write, // TODO: minus one capacity?
            _ => _unreachable!(),
        };

        let grant_range = self.read..(self.read + sz);
        Ok(GrantRange::from_range(grant_range))
    }

    #[inline]
    pub(super) fn commit_read(&mut self, size: usize, used: usize) {
        _unsafe_assert!(used <= size);
        self.read += used;
        self.sm_rel_read();
    }
}
