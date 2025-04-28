use defmt::debug_assert;

use crate::_unreachable;

const USIZE_SIZE: usize = core::mem::size_of::<usize>();
const USIZE_SIZE_PLUS_ONE: usize = USIZE_SIZE + 1;

const fn max_size_header() -> u8 {
    // 64-bit: 0b0000_0000
    // 32-bit: 0b0001_0000
    // 16-bit: 0b0000_0100
    //  8-bit: 0b0000_0010
    ((1usize << USIZE_SIZE) & 0xFF) as u8
}

#[cfg(target_pointer_width = "64")]
pub fn encoded_len(value: usize) -> usize {
    match value.leading_zeros() {
        0..=7 => 9,
        8..=14 => 8,
        15..=21 => 7,
        22..=28 => 6,
        29..=35 => 5,
        36..=42 => 4,
        43..=49 => 3,
        50..=56 => 2,
        57..=64 => 1,
        _ => _unreachable!(),
    }
}

#[cfg(target_pointer_width = "32")]
pub fn encoded_len(value: usize) -> usize {
    match value.leading_zeros() {
        0..=3 => 5,
        4..=10 => 4,
        11..=17 => 3,
        18..=24 => 2,
        25..=32 => 1,
        _ => _unreachable!(),
    }
}

#[cfg(target_pointer_width = "16")]
pub fn encoded_len(value: usize) -> usize {
    match value.leading_zeros() {
        0..=1 => 3,
        2..=8 => 2,
        9..=16 => 1,
        _ => _unreachable!(),
    }
}

pub fn encode_usize_to_slice(value: usize, length: usize, slice: &mut [u8]) {
    debug_assert!(
        encoded_len(value) <= length,
        "Tried to encode to smaller than necessary length!",
    );
    debug_assert!(length <= slice.len(), "Not enough space to encode!",);
    debug_assert!(
        length <= USIZE_SIZE_PLUS_ONE,
        "Tried to encode larger than platform supports!",
    );

    let header_bytes = &mut slice[..length];

    if length >= USIZE_SIZE_PLUS_ONE {
        header_bytes[0] = max_size_header();
        header_bytes[1..USIZE_SIZE_PLUS_ONE].copy_from_slice(&value.to_le_bytes());
    } else {
        let encoded = (value << 1 | 1) << (length - 1);
        header_bytes.copy_from_slice(&encoded.to_le_bytes()[..length]);
    }
}

pub fn decoded_len(byte: u8) -> usize {
    byte.trailing_zeros() as usize + 1
}

pub fn decode_usize(input: &[u8]) -> usize {
    let length = decoded_len(input[0]);

    debug_assert!(input.len() >= length, "Not enough data to decode!",);
    debug_assert!(
        length <= USIZE_SIZE_PLUS_ONE,
        "Tried to decode data too large for this platform!",
    );

    let header_bytes = &input[..length];

    let mut encoded = [0u8; USIZE_SIZE];

    if length >= USIZE_SIZE_PLUS_ONE {
        encoded.copy_from_slice(&header_bytes[1..]);
        usize::from_le_bytes(encoded)
    } else {
        encoded[..length].copy_from_slice(header_bytes);
        usize::from_le_bytes(encoded) >> length
    }
}
