use core::mem;

use critical_section::CriticalSection;

use crate::book::GrantRange;
use crate::buffer::Ring;

enum Ref<'a, 'ring> {
    Write(&'a mut GrantWrite<'ring>),
    Read(&'a mut GrantRead<'ring>),
}

#[inline(never)]
fn drop_grant(ty: Ref) {
    critical_section::with(|cs| match ty {
        Ref::Write(grant) => grant.commit_internal(cs, 0),
        Ref::Read(grant) => grant.commit_internal(cs, 0),
    });
}

#[must_use]
#[derive(Debug)]
pub struct GrantWrite<'a> {
    pub(crate) ring: &'a Ring<'a>,
    pub(crate) range: GrantRange,
}

impl<'a> GrantWrite<'a> {
    #[inline]
    pub fn buf(&self) -> &[u8] {
        let range = self.range.to_range();
        unsafe { self.ring.view(range) }
    }

    #[inline]
    pub fn buf_mut(&mut self) -> &mut [u8] {
        let range = self.range.to_range();
        unsafe { self.ring.view_mut(range) }
    }

    #[inline]
    pub fn commit(mut self, cs: CriticalSection, used: usize) {
        self.commit_internal(cs, used);
        mem::forget(self);
    }

    #[inline(never)]
    fn commit_internal(&mut self, cs: CriticalSection, used: usize) {
        let dst = self.ring._dst(cs);

        if used == 0 {
            dst.book.release_write();
            return;
        }

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
    pub fn buf(&self) -> &[u8] {
        let range = self.range.to_range();
        unsafe { self.ring.view(range) }
    }

    #[inline]
    pub fn commit(mut self, cs: CriticalSection, used: usize) {
        self.commit_internal(cs, used);
        mem::forget(self);
    }

    #[inline(never)]
    fn commit_internal(&mut self, cs: CriticalSection, used: usize) {
        let dst = self.ring._dst(cs);

        if used == 0 {
            dst.book.release_read();
            return;
        }

        dst.book.commit_read(self.range.to_len(), used);

        dst.waker.wake()
    }
}

impl<'a> Drop for GrantRead<'a> {
    #[inline]
    fn drop(&mut self) {
        drop_grant(Ref::Read(self));
    }
}
