use core::pin::{Pin, pin};
use core::task::{Context, Poll};

use critical_section::CriticalSection;

use crate::buffer::Ring;

struct DynPollFn<'a, 'f, T> {
    ring: &'a Ring<'a>,
    op: &'f dyn Fn(&'a Ring, CriticalSection) -> Option<T>,
}

impl<'a, 'f, T> Future for DynPollFn<'a, 'f, T> {
    type Output = T;

    #[inline(never)]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        critical_section::with(|cs| {
            let fut = pin!(self);
            if let Some(result) = (fut.op)(fut.ring, cs) {
                return Poll::Ready(result);
            }

            fut.ring._dst(cs).waker.register(cx.waker());
            Poll::Pending
        })
    }
}

pub struct PollFn<'a, F> {
    ring: &'a Ring<'a>,
    op: F,
}

impl<'a, T, F> Future for PollFn<'a, F>
where
    F: Fn(&'a Ring, CriticalSection) -> Option<T>,
{
    type Output = T;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let fut = pin!(DynPollFn {
            ring: self.ring,
            op: &self.op,
        });

        fut.poll(cx)
    }
}

impl<'a> Ring<'a> {
    #[inline]
    pub fn poll<'b, F, T>(&'b self, op: F) -> PollFn<'b, F>
    where
        F: Fn(&'b Ring, CriticalSection) -> Option<T>,
    {
        PollFn { ring: self, op }
    }
}
