use core::future::Future;
use core::pin::{Pin, pin};
use core::task::{Context, Poll};

use critical_section::CriticalSection;

use crate::buffer::RbQueue;

impl<const N: usize> RbQueue<N> {
    pub fn wake(&self, _cs: CriticalSection) {
        let inner = unsafe { self.inner_ref() };
        inner.waker.wake();
    }

    pub fn wait<'a, F, T>(&'a self, op: F) -> RbQueueFuture<'a, F, N>
    where
        F: Fn(&RbQueue<N>, CriticalSection) -> Option<T>,
    {
        RbQueueFuture { queue: self, op }
    }
}

pub struct RbQueueFuture<'a, F, const N: usize> {
    queue: &'a RbQueue<N>,
    op: F,
}

impl<T, F, const N: usize> Future for RbQueueFuture<'_, F, N>
where
    F: Fn(&RbQueue<N>, CriticalSection) -> Option<T>,
{
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        critical_section::with(|cs| {
            let fut = pin!(self);
            if let Some(result) = (fut.op)(fut.queue, cs) {
                return Poll::Ready(result);
            }

            let inner = unsafe { fut.queue.inner_ref() };
            inner.waker.register(cx.waker());
            Poll::Pending
        })
    }
}
