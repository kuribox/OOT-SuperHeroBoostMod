//! IPS32 patches: the mod ships several (`ours.ips` per feature set) and the app merges them into one file,
//! because the emulator applies a single patch per file.
//!
//! Format: `IPS32`, then records of `offset: u32 (big endian)`, `size: u16 (big endian)`, `data[size]`, then
//! `EEOF`. A zero size means RLE, which we never produce and refuse to merge.

use crate::error::{Error, Result};

const HEADER: &[u8; 5] = b"IPS32";
const END: &[u8; 4] = b"EEOF";

/// A patch as (offset, bytes) records.
pub type Records = Vec<(u32, Vec<u8>)>;

pub fn read(blob: &[u8]) -> Result<Records> {
    if blob.len() < HEADER.len() || &blob[..HEADER.len()] != HEADER {
        return Err(Error::IpsFormat("no IPS32 header"));
    }
    let mut out = Records::new();
    let mut i = HEADER.len();
    while blob.len() >= i + 4 && &blob[i..i + 4] != END {
        if blob.len() < i + 6 {
            return Err(Error::IpsFormat("truncated record header"));
        }
        let offset = u32::from_be_bytes(blob[i..i + 4].try_into().unwrap());
        let size = u16::from_be_bytes([blob[i + 4], blob[i + 5]]) as usize;
        if size == 0 {
            return Err(Error::IpsFormat("RLE records are not supported"));
        }
        let end = i + 6 + size;
        if blob.len() < end {
            return Err(Error::IpsFormat("truncated record"));
        }
        out.push((offset, blob[i + 6..end].to_vec()));
        i = end;
    }
    if blob.len() < i + 4 || &blob[i..i + 4] != END {
        return Err(Error::IpsFormat("missing EEOF"));
    }
    Ok(out)
}

/// Merge several patches into one, refusing overlaps: two mods patching the same byte is an ambiguity the app
/// cannot resolve, so it reports it instead of silently picking one.
pub fn merge(blobs: &[&[u8]]) -> Result<Vec<u8>> {
    let mut taken: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    let mut records: Records = Vec::new();
    for (n, blob) in blobs.iter().enumerate() {
        for (offset, bytes) in read(blob)? {
            for k in offset..offset + bytes.len() as u32 {
                if let Some(owner) = taken.insert(k, n)
                    && owner != n
                {
                    return Err(Error::IpsOverlap(k as usize));
                }
            }
            records.push((offset, bytes));
        }
    }
    let mut out = Vec::with_capacity(records.iter().map(|(_, b)| b.len() + 6).sum::<usize>() + 9);
    out.extend_from_slice(HEADER);
    for (offset, bytes) in records {
        out.extend_from_slice(&offset.to_be_bytes());
        out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        out.extend_from_slice(&bytes);
    }
    out.extend_from_slice(END);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(records: &[(u32, &[u8])]) -> Vec<u8> {
        let mut out = HEADER.to_vec();
        for (offset, bytes) in records {
            out.extend_from_slice(&offset.to_be_bytes());
            out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
            out.extend_from_slice(bytes);
        }
        out.extend_from_slice(END);
        out
    }

    #[test]
    fn round_trip() {
        let blob = patch(&[(0x1000, &[1, 2, 3]), (0x2000, &[9])]);
        let records = read(&blob).unwrap();
        assert_eq!(records, vec![(0x1000, vec![1, 2, 3]), (0x2000, vec![9])]);
        // A single patch merges to itself.
        assert_eq!(merge(&[&blob]).unwrap(), blob);
    }

    #[test]
    fn merge_concatenates_disjoint_patches() {
        let a = patch(&[(0x10, &[1])]);
        let b = patch(&[(0x20, &[2, 3])]);
        let merged = read(&merge(&[&a, &b]).unwrap()).unwrap();
        assert_eq!(merged, vec![(0x10, vec![1]), (0x20, vec![2, 3])]);
    }

    #[test]
    fn overlaps_are_reported() {
        let a = patch(&[(0x10, &[1, 2])]);
        let b = patch(&[(0x11, &[3])]);
        assert!(matches!(merge(&[&a, &b]), Err(Error::IpsOverlap(0x11))));
    }

    #[test]
    fn bad_input_is_rejected() {
        assert!(matches!(read(b"IPS  "), Err(Error::IpsFormat(_))));
        assert!(matches!(read(b"IPS32\x00\x01"), Err(Error::IpsFormat(_))));
        let rle = patch(&[(0x10, &[1])]);
        let mut rle = rle;
        rle[9] = 0;
        rle[10] = 0;
        assert!(matches!(read(&rle), Err(Error::IpsFormat(_))));
    }
}
