//! Reads the ZIP archive of a bundle from a [`BundleSource`] whose bytes may
//! still be arriving.
//!
//! Opening reads only the end of the file (the central directory); each
//! entry is read when asked for. The `zip` crate cannot do this: it reads
//! every entry's local header while opening, which would need bytes from all
//! over a file that is still downloading. Bundles hold stored or Deflate
//! entries, with or without ZIP64 records, and that is all this reads.

use std::collections::HashMap;
use std::io::Read;
use std::ops::Range;

use crate::source::BundleSource;
use crate::{BundleError, Result};

const LOCAL_HEADER: u32 = 0x0403_4b50;
const CENTRAL_HEADER: u32 = 0x0201_4b50;
const END_OF_DIRECTORY: u32 = 0x0605_4b50;
const ZIP64_END_OF_DIRECTORY: u32 = 0x0606_4b50;
const ZIP64_LOCATOR: u32 = 0x0706_4b50;
/// Fixed part of the end of central directory record.
const END_RECORD_LEN: u64 = 22;
/// A 16-bit field (or 32-bit size) saturated to say its value is in the
/// ZIP64 records.
const ZIP64_U16: u16 = u16::MAX;
const ZIP64_U32: u32 = u32::MAX;
/// How far from the end the end record can start: its fixed part plus the
/// longest comment.
const END_SEARCH: u64 = END_RECORD_LEN + u16::MAX as u64;

/// What the central directory says about one entry.
#[derive(Debug, Clone)]
struct Entry {
    /// Bytes from the entry's local header to the next entry (or the
    /// central directory): everything reading it needs.
    span: Range<u64>,
    method: u16,
    crc32: u32,
    compressed_size: u64,
    size: u64,
}

/// An open archive: its directory, and the bytes, whole or in pieces.
pub(crate) struct Archive {
    source: BundleSource,
    entries: HashMap<String, Entry>,
}

fn damaged(detail: impl std::fmt::Display) -> BundleError {
    BundleError::Corrupt(format!("the archive is damaged: {detail}"))
}

fn incomplete(missing: Vec<Range<u64>>) -> BundleError {
    BundleError::Incomplete { missing }
}

/// Little-endian fields of a record, bounds-checked.
struct Fields<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Fields<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| damaged("a record runs past its end"))?;
        let bytes = &self.bytes[self.position..end];
        self.position = end;
        Ok(bytes)
    }

    fn skip(&mut self, len: usize) -> Result<()> {
        self.take(len).map(|_| ())
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("2 bytes"),
        ))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("8 bytes"),
        ))
    }
}

fn read(source: &BundleSource, range: Range<u64>) -> Result<Vec<u8>> {
    source.read(range).map_err(incomplete)
}

impl Archive {
    /// Read the central directory. Fails with [`BundleError::Incomplete`]
    /// while the end of the file has not arrived.
    pub(crate) fn open(source: BundleSource) -> Result<Self> {
        let len = source.len();
        if len < END_RECORD_LEN {
            return Err(BundleError::Corrupt(
                "not a Gaanim bundle (too short)".into(),
            ));
        }
        // Bundles have no archive comment, so the end record usually is the
        // last 22 bytes; only otherwise search the longest comment's reach.
        let find_end = |tail: &[u8]| {
            (0..=tail.len() - END_RECORD_LEN as usize)
                .rev()
                .find(|&at| {
                    tail[at..at + 4] == END_OF_DIRECTORY.to_le_bytes() && {
                        let comment = u16::from_le_bytes([tail[at + 20], tail[at + 21]]);
                        at + END_RECORD_LEN as usize + usize::from(comment) == tail.len()
                    }
                })
        };
        let mut tail_start = len - END_RECORD_LEN;
        let mut tail = read(&source, tail_start..len)?;
        let mut end_offset = find_end(&tail);
        if end_offset.is_none() {
            tail_start = len.saturating_sub(END_SEARCH);
            tail = read(&source, tail_start..len)?;
            end_offset = find_end(&tail);
        }
        let end_offset = end_offset.ok_or_else(|| {
            BundleError::Corrupt("not a Gaanim bundle (not a ZIP archive)".into())
        })?;
        let mut end = Fields::new(&tail[end_offset..]);
        end.skip(4 + 2 + 2 + 2)?;
        let mut count = u64::from(end.u16()?);
        let mut directory_size = u64::from(end.u32()?);
        let mut directory_start = u64::from(end.u32()?);
        let end_position = tail_start + end_offset as u64;

        if count == u64::from(ZIP64_U16)
            || directory_size == u64::from(ZIP64_U32)
            || directory_start == u64::from(ZIP64_U32)
        {
            let locator_start = end_position
                .checked_sub(20)
                .ok_or_else(|| damaged("the ZIP64 locator is missing"))?;
            let locator = read(&source, locator_start..end_position)?;
            let mut locator = Fields::new(&locator);
            if locator.u32()? != ZIP64_LOCATOR {
                return Err(damaged("the ZIP64 locator is missing"));
            }
            locator.skip(4)?;
            let record_start = locator.u64()?;
            let record_end = record_start
                .checked_add(56)
                .filter(|record_end| *record_end <= locator_start)
                .ok_or_else(|| damaged("the ZIP64 end record is out of range"))?;
            let record = read(&source, record_start..record_end)?;
            let mut record = Fields::new(&record);
            if record.u32()? != ZIP64_END_OF_DIRECTORY {
                return Err(damaged("the ZIP64 end record is missing"));
            }
            record.skip(8 + 2 + 2 + 4 + 4 + 8)?;
            count = record.u64()?;
            directory_size = record.u64()?;
            directory_start = record.u64()?;
        }
        let directory_end = directory_start
            .checked_add(directory_size)
            .filter(|directory_end| *directory_end <= end_position)
            .ok_or_else(|| damaged("the central directory is out of range"))?;
        let directory = read(&source, directory_start..directory_end)?;

        let mut fields = Fields::new(&directory);
        let mut listed = Vec::with_capacity(count.min(1 << 20) as usize);
        for _ in 0..count {
            if fields.u32()? != CENTRAL_HEADER {
                return Err(damaged("a central directory entry is malformed"));
            }
            fields.skip(2 + 2)?;
            let flags = fields.u16()?;
            let method = fields.u16()?;
            fields.skip(2 + 2)?;
            let crc32 = fields.u32()?;
            let mut compressed_size = u64::from(fields.u32()?);
            let mut size = u64::from(fields.u32()?);
            let name_len = usize::from(fields.u16()?);
            let extra_len = usize::from(fields.u16()?);
            let comment_len = usize::from(fields.u16()?);
            fields.skip(2 + 2 + 4)?;
            let mut header = u64::from(fields.u32()?);
            let name = std::str::from_utf8(fields.take(name_len)?)
                .map_err(|_| damaged("an entry name is not UTF-8"))?
                .to_owned();
            let mut extra = Fields::new(fields.take(extra_len)?);
            while extra.position + 4 <= extra.bytes.len() {
                let id = extra.u16()?;
                let len = usize::from(extra.u16()?);
                let mut field = Fields::new(extra.take(len)?);
                if id == 0x0001 {
                    // As the `zip` crate writes and reads them: a full
                    // block holds all three values, otherwise only the
                    // saturated ones are present.
                    if len >= 24 || size == u64::from(ZIP64_U32) {
                        size = field.u64()?;
                    }
                    if len >= 24 || compressed_size == u64::from(ZIP64_U32) {
                        compressed_size = field.u64()?;
                    }
                    if len >= 24 || header == u64::from(ZIP64_U32) {
                        header = field.u64()?;
                    }
                }
            }
            fields.skip(comment_len)?;
            if flags & 1 != 0 {
                return Err(BundleError::Unsupported(format!(
                    "entry {name} is encrypted"
                )));
            }
            if header >= directory_start {
                return Err(damaged(format!("entry {name} starts after the directory")));
            }
            listed.push((header, name, method, crc32, compressed_size, size));
        }

        listed.sort_by_key(|entry| entry.0);
        let ends: Vec<u64> = listed
            .iter()
            .skip(1)
            .map(|next| next.0)
            .chain([directory_start])
            .collect();
        let entries = listed
            .into_iter()
            .zip(ends)
            .map(
                |((header, name, method, crc32, compressed_size, size), end)| {
                    let entry = Entry {
                        span: header..end,
                        method,
                        crc32,
                        compressed_size,
                        size,
                    };
                    (name, entry)
                },
            )
            .collect();
        Ok(Self { source, entries })
    }

    pub(crate) fn source(&self) -> &BundleSource {
        &self.source
    }

    /// Bytes of the file that entry `name` needs, when it is in the archive.
    pub(crate) fn span(&self, name: &str) -> Option<Range<u64>> {
        self.entries.get(name).map(|entry| entry.span.clone())
    }

    /// Every entry name with the bytes it needs.
    pub(crate) fn spans(&self) -> impl Iterator<Item = (&str, Range<u64>)> {
        self.entries
            .iter()
            .map(|(name, entry)| (name.as_str(), entry.span.clone()))
    }

    /// The stored bytes of entry `name`, decompressed from the archive's
    /// own compression.
    pub(crate) fn read(&self, name: &str) -> Result<Vec<u8>> {
        let entry = self
            .entries
            .get(name)
            .ok_or_else(|| BundleError::Corrupt(format!("missing entry {name}")))?;
        let bytes = read(&self.source, entry.span.clone())?;
        let mut header = Fields::new(&bytes);
        if header.u32()? != LOCAL_HEADER {
            return Err(damaged(format!("entry {name} has no local header")));
        }
        header.skip(2 + 2 + 2 + 2 + 2 + 4 + 4 + 4)?;
        let name_len = usize::from(header.u16()?);
        let extra_len = usize::from(header.u16()?);
        header.skip(name_len + extra_len)?;
        let compressed_size = usize::try_from(entry.compressed_size)
            .map_err(|_| damaged(format!("entry {name} is too large")))?;
        let data = header.take(compressed_size)?;
        let limit = entry.size.min(1 << 32);
        let mut plain = Vec::with_capacity(limit.min(1 << 30) as usize);
        match entry.method {
            0 => plain.extend_from_slice(data),
            8 => {
                flate2::read::DeflateDecoder::new(data)
                    .take(limit + 1)
                    .read_to_end(&mut plain)
                    .map_err(|error| damaged(format!("entry {name}: {error}")))?;
            }
            method => {
                return Err(BundleError::Unsupported(format!(
                    "entry {name} uses ZIP compression method {method}"
                )));
            }
        }
        if plain.len() as u64 != entry.size {
            return Err(damaged(format!("entry {name} has the wrong size")));
        }
        if crc32fast::hash(&plain) != entry.crc32 {
            return Err(damaged(format!("entry {name} does not match its CRC")));
        }
        Ok(plain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Arc;

    fn archive(large: bool, comment: &str) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let deflated = zip::write::SimpleFileOptions::default().large_file(large);
        let stored = deflated.compression_method(zip::CompressionMethod::Stored);
        writer.start_file("frames/000000.bin", stored).unwrap();
        writer.write_all(&[7; 300]).unwrap();
        writer.start_file("manifest.json", deflated).unwrap();
        writer
            .write_all(b"{\"format\": \"gaanim-bundle\"}")
            .unwrap();
        writer.set_comment(comment);
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn reads_stored_and_deflated_entries_with_and_without_zip64() {
        for large in [false, true] {
            let bytes = archive(large, "a comment");
            let archive = Archive::open(BundleSource::whole(bytes.into())).unwrap();
            assert_eq!(archive.read("frames/000000.bin").unwrap(), vec![7; 300]);
            assert_eq!(
                archive.read("manifest.json").unwrap(),
                b"{\"format\": \"gaanim-bundle\"}"
            );
            assert!(matches!(
                archive.read("scene.bin"),
                Err(BundleError::Corrupt(_))
            ));
        }
    }

    #[test]
    fn opens_from_the_end_of_the_file_and_asks_for_each_entry() {
        let bytes = archive(true, "");
        let len = bytes.len() as u64;
        let source = BundleSource::new(len);
        assert!(matches!(
            Archive::open(source.clone()),
            Err(BundleError::Incomplete { .. })
        ));

        let tail = len - 300;
        source.insert(tail, Arc::from(&bytes[tail as usize..]));
        let archive = Archive::open(source.clone()).unwrap();
        let span = archive.span("frames/000000.bin").unwrap();
        assert_eq!(span.start, 0);
        match archive.read("frames/000000.bin") {
            Err(BundleError::Incomplete { missing }) => {
                assert_eq!(missing, vec![span.start..span.end.min(tail)]);
            }
            other => panic!("expected the frames to be missing: {other:?}"),
        }

        source.insert(0, Arc::from(&bytes[..tail as usize]));
        assert_eq!(archive.read("frames/000000.bin").unwrap(), vec![7; 300]);
    }

    #[test]
    fn a_flipped_byte_fails_the_crc() {
        let mut bytes = archive(false, "");
        let at = bytes.windows(3).position(|w| w == [7, 7, 7]).unwrap() + 10;
        bytes[at] ^= 1;
        let archive = Archive::open(BundleSource::whole(bytes.into())).unwrap();
        assert!(archive.read("frames/000000.bin").is_err());
    }
}
