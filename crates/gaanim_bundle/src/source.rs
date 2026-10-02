//! The bytes of a bundle file, whole or arriving in pieces.
//!
//! The web player opens a bundle from the pieces it has downloaded (the end
//! of the file, which holds the archive's directory and the bundle's tables)
//! and adds the frame chunks as they arrive. Reading bytes that have not
//! arrived yet reports which ranges are missing instead of failing.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::{Arc, Mutex};

/// The bytes of a bundle file. Clones share the same pieces, so bytes added
/// through one clone are visible to a [`crate::Bundle`] opened from another.
#[derive(Clone, Debug)]
pub struct BundleSource {
    pieces: Arc<Mutex<Pieces>>,
}

#[derive(Debug)]
struct Pieces {
    len: u64,
    /// Downloaded pieces by start offset. Pieces may overlap.
    by_start: BTreeMap<u64, Arc<[u8]>>,
}

impl Pieces {
    /// The piece holding the byte at `offset`, with its start.
    fn covering(&self, offset: u64) -> Option<(u64, &Arc<[u8]>)> {
        self.by_start
            .range(..=offset)
            .rev()
            .find(|(start, bytes)| **start + bytes.len() as u64 > offset)
            .map(|(start, bytes)| (*start, bytes))
    }

    /// Start of the first piece after `offset`, or the end of the file.
    fn next_start(&self, offset: u64) -> u64 {
        self.by_start
            .range(offset + 1..)
            .next()
            .map_or(self.len, |(start, _)| *start)
    }
}

impl BundleSource {
    /// A file of `len` bytes of which nothing has arrived yet.
    pub fn new(len: u64) -> Self {
        Self {
            pieces: Arc::new(Mutex::new(Pieces {
                len,
                by_start: BTreeMap::new(),
            })),
        }
    }

    /// A file whose bytes are all here.
    pub fn whole(bytes: Arc<[u8]>) -> Self {
        let source = Self::new(bytes.len() as u64);
        source.insert(0, bytes);
        source
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Pieces> {
        self.pieces
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Length of the whole file.
    pub fn len(&self) -> u64 {
        self.lock().len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Add the bytes that start at `offset`. Bytes past the end of the file
    /// are dropped.
    pub fn insert(&self, offset: u64, bytes: Arc<[u8]>) {
        let mut pieces = self.lock();
        if offset >= pieces.len || bytes.is_empty() {
            return;
        }
        let bytes = if offset + bytes.len() as u64 > pieces.len {
            Arc::from(&bytes[..(pieces.len - offset) as usize])
        } else {
            bytes
        };
        let end = offset + bytes.len() as u64;
        if pieces
            .covering(offset)
            .is_some_and(|(start, piece)| start + piece.len() as u64 >= end)
        {
            return;
        }
        let longer = pieces
            .by_start
            .get(&offset)
            .is_some_and(|piece| piece.len() >= bytes.len());
        if !longer {
            pieces.by_start.insert(offset, bytes);
        }
    }

    /// The parts of `range` that have not arrived, in order.
    pub fn missing(&self, range: Range<u64>) -> Vec<Range<u64>> {
        let pieces = self.lock();
        let end = range.end.min(pieces.len);
        let mut missing = Vec::new();
        let mut offset = range.start;
        while offset < end {
            match pieces.covering(offset) {
                Some((start, piece)) => offset = start + piece.len() as u64,
                None => {
                    let gap_end = pieces.next_start(offset).min(end);
                    missing.push(offset..gap_end);
                    offset = gap_end;
                }
            }
        }
        missing
    }

    /// Whether every byte has arrived.
    pub fn is_complete(&self) -> bool {
        self.missing(0..self.len()).is_empty()
    }

    /// The bytes of `range`, or the parts of it still missing.
    pub(crate) fn read(&self, range: Range<u64>) -> Result<Vec<u8>, Vec<Range<u64>>> {
        let pieces = self.lock();
        if range.end > pieces.len {
            // Bytes past the end never arrive; the caller reports them.
            let len = pieces.len;
            drop(pieces);
            let mut missing = self.missing(range.start.min(len)..len);
            missing.push(range.start.max(len)..range.end);
            return Err(missing);
        }
        let mut bytes = Vec::with_capacity((range.end - range.start) as usize);
        let mut offset = range.start;
        while offset < range.end {
            let Some((start, piece)) = pieces.covering(offset) else {
                drop(pieces);
                return Err(self.missing(range));
            };
            let from = (offset - start) as usize;
            let to = ((range.end - start) as usize).min(piece.len());
            bytes.extend_from_slice(&piece[from..to]);
            offset = start + to as u64;
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(range: Range<u8>) -> Arc<[u8]> {
        range.collect::<Vec<u8>>().into()
    }

    #[test]
    fn reads_across_pieces_and_names_the_gaps() {
        let source = BundleSource::new(100);
        source.insert(10, bytes(10..30));
        source.insert(25, bytes(25..50));
        source.insert(80, bytes(80..100));

        assert_eq!(source.read(12..40).unwrap(), (12..40).collect::<Vec<u8>>());
        assert_eq!(source.missing(0..100), vec![0..10, 50..80]);
        assert_eq!(source.read(40..90).unwrap_err(), vec![50..80]);
        assert!(!source.is_complete());

        source.insert(0, bytes(0..10));
        source.insert(50, bytes(50..80));
        assert!(source.is_complete());
        assert_eq!(source.read(0..100).unwrap(), (0..100).collect::<Vec<u8>>());
    }

    #[test]
    fn a_whole_file_is_complete_and_bytes_past_its_end_are_dropped() {
        let source = BundleSource::whole(bytes(0..8));
        assert!(source.is_complete());
        source.insert(4, bytes(0..10));
        assert_eq!(source.read(0..8).unwrap(), (0..8).collect::<Vec<u8>>());
        assert_eq!(source.read(4..9).unwrap_err(), vec![8..9]);
        assert_eq!(
            BundleSource::new(8).read(4..9).unwrap_err(),
            vec![4..8, 8..9]
        );
    }
}
