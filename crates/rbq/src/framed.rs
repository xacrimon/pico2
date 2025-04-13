use core::cmp::min;

use critical_section::CriticalSection;
use defmt::debug_assert;

use crate::Error;
use crate::buffer::{GrantRead, GrantWrite, RbQueue};
use crate::vusize::{decode_usize, decoded_len, encode_usize_to_slice, encoded_len};

impl<const N: usize> RbQueue<N> {
    pub fn grant_frame(
        &mut self,
        max_sz: usize,
        cs: CriticalSection,
    ) -> Result<FrameGrantWrite<N>, Error> {
        let hdr_len = encoded_len(max_sz);
        Ok(FrameGrantWrite {
            grant: self.grant_exact(max_sz + hdr_len, cs)?,
            hdr_len: hdr_len as u8,
        })
    }

    pub fn read_frame(&mut self, cs: CriticalSection) -> Option<FrameGrantRead<N>> {
        // Get all available bytes. We never wrap a frame around,
        // so if a header is available, the whole frame will be.
        let mut grant = self.read(cs).ok()?;

        // Additionally, we never commit less than a full frame with
        // a header, so if we have ANY data, we'll have a full header
        // and frame. `Consumer::read` will return an Error when
        // there are 0 bytes available.

        // The header consists of a single usize, encoded in native
        // endianess order
        let frame_len = decode_usize(grant.buf());
        let hdr_len = decoded_len(grant.buf()[0]);
        let total_len = frame_len + hdr_len;
        let hdr_len = hdr_len as u8;

        debug_assert!(grant.buf().len() >= total_len);

        grant.shrink(total_len);

        Some(FrameGrantRead { grant, hdr_len })
    }
}

#[must_use]
#[derive(Debug)]
pub struct FrameGrantWrite<'a, const N: usize> {
    grant: GrantWrite<'a, N>,
    hdr_len: u8,
}

impl<const N: usize> FrameGrantWrite<'_, N> {
    pub fn commit(mut self, used: usize, cs: CriticalSection) {
        let total_len = self.set_header(used);

        // Commit the header + frame
        self.grant.commit(total_len, cs);
    }

    fn set_header(&mut self, used: usize) -> usize {
        // Saturate the commit size to the available frame size
        let grant_len = self.grant.buf().len();
        let hdr_len: usize = self.hdr_len.into();
        let frame_len = min(used, grant_len - hdr_len);
        let total_len = frame_len + hdr_len;

        // Write the actual frame length to the header
        encode_usize_to_slice(frame_len, hdr_len, &mut self.grant.buf_mut()[..hdr_len]);

        total_len
    }

    pub fn buf(&self) -> &[u8] {
        &self.grant.buf()[self.hdr_len.into()..]
    }

    pub fn buf_mut(&mut self) -> &mut [u8] {
        &mut self.grant.buf_mut()[self.hdr_len.into()..]
    }
}

#[must_use]
#[derive(Debug)]
pub struct FrameGrantRead<'a, const N: usize> {
    grant: GrantRead<'a, N>,
    hdr_len: u8,
}

impl<const N: usize> FrameGrantRead<'_, N> {
    pub fn release(self, cs: CriticalSection) {
        // For a read grant, we have already shrunk the grant
        // size down to the correct size
        let len = self.grant.buf().len();
        unsafe {
            self.grant.release_inner(len, cs);
        }
    }

    pub fn buf(&self) -> &[u8] {
        &self.grant.buf()[self.hdr_len.into()..]
    }

    pub fn buf_mut(&mut self) -> &mut [u8] {
        &mut self.grant.buf_mut()[self.hdr_len.into()..]
    }
}
