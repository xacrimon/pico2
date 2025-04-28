use core::mem;

use critical_section::CriticalSection;

use crate::book::{GrantRange, SplitGrantRange};
use crate::buffer::Ring;

enum Ref<'a, 'ring> {
    Write(&'a mut GrantWrite<'ring>),
    Read(&'a mut GrantRead<'ring>),
    SplitRead(&'a mut SplitGrantRead<'ring>),
}

#[inline(never)]
fn drop_grant(ty: Ref) {
    critical_section::with(|cs| match ty {
        Ref::Write(grant) => grant.commit_internal(cs, 0),
        Ref::Read(grant) => grant.commit_internal(cs, 0),
        Ref::SplitRead(grant) => grant.commit_internal(cs, 0),
    });
}

// TODO: set value read to autocommit on drop

#[must_use]
#[derive(Debug)]
pub struct GrantWrite<'a> {
    pub(crate) ring: &'a Ring<'a>,
    pub(crate) range: GrantRange,
}

impl<'a> GrantWrite<'a> {
    #[inline]
    pub fn commit(mut self, cs: CriticalSection, used: usize) {
        self.commit_internal(cs, used);
        mem::forget(self);
    }

    #[inline]
    pub fn release(self, cs: CriticalSection) {
        self.ring._dst(cs).book.release_write();
        mem::forget(self);
    }

    #[inline(never)]
    fn commit_internal(&mut self, cs: CriticalSection, used: usize) {
        let dst = self.ring._dst(cs);
        let capacity = dst.buf.len();
        dst.book
            .commit_write_exact(capacity, self.range.to_len(), used);

        dst.waker.wake();
    }
}

impl<'a> Drop for GrantWrite<'a> {
    #[inline]
    fn drop(&mut self) {
        drop_grant(Ref::Write(self));
    }
}

#[must_use]
#[derive(Debug)]
pub struct GrantRead<'a> {
    pub(crate) ring: &'a Ring<'a>,
    pub(crate) range: GrantRange,
}

impl<'a> GrantRead<'a> {
    #[inline]
    pub fn commit(mut self, cs: CriticalSection, used: usize) {
        self.commit_internal(cs, used);
        mem::forget(self);
    }

    #[inline]
    pub fn release(self, cs: CriticalSection) {
        self.ring._dst(cs).book.release_read();
        mem::forget(self);
    }

    #[inline(never)]
    fn commit_internal(&mut self, cs: CriticalSection, used: usize) {
        let dst = self.ring._dst(cs);
        dst.waker.wake();
    }
}

impl<'a> Drop for GrantRead<'a> {
    #[inline]
    fn drop(&mut self) {
        drop_grant(Ref::Read(self));
    }
}

#[must_use]
#[derive(Debug)]
pub struct SplitGrantRead<'a> {
    pub(crate) ring: &'a Ring<'a>,
    pub(crate) ranges: SplitGrantRange,
}

impl<'a> SplitGrantRead<'a> {
    #[inline]
    pub fn commit(mut self, cs: CriticalSection, used: usize) {
        self.commit_internal(cs, used);
        mem::forget(self);
    }

    #[inline]
    pub fn release(self, cs: CriticalSection) {
        self.ring._dst(cs).book.release_read();
        mem::forget(self);
    }

    #[inline(never)]
    fn commit_internal(&mut self, cs: CriticalSection, used: usize) {
        let dst = self.ring._dst(cs);
        self.ring._dst(cs).waker.wake();
    }
}

impl<'a> Drop for SplitGrantRead<'a> {
    #[inline]
    fn drop(&mut self) {
        drop_grant(Ref::SplitRead(self));
    }
}
