// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use feo_log::Level;
use feo_time::SystemTime;
use std::io::{self, Read};
use std::mem::size_of;
use std::time::Duration;

/// Log record that can be encoded. This is the borrowed version.
#[derive(Debug)]
pub struct Record<'a> {
    pub timestamp: SystemTime,
    pub level: Level,
    pub target: &'a str,
    pub file: Option<&'a str>,
    pub line: Option<u32>,
    pub tgid: u32,
    pub tid: u32,
    pub args: &'a [u8],
}

impl Record<'_> {
    /// Create a new record.
    #[allow(clippy::too_many_arguments)]
    pub fn new<'a>(
        timestamp: SystemTime,
        level: Level,
        target: &'a str,
        file: Option<&'a str>,
        line: Option<u32>,
        tgid: u32,
        tid: u32,
        args: &'a [u8],
    ) -> Record<'a> {
        Record {
            timestamp,
            level,
            target,
            file,
            line,
            tgid,
            tid,
            args,
        }
    }

    pub fn encoded_len(&self) -> usize {
        let mut len: usize = 0;

        len += size_of::<u64>() + size_of::<u32>(); // Timestamp
        len += size_of::<u8>(); // Level
        len += size_of::<u32>() + self.target.len(); // Target
        len += self
            .file
            .map_or(size_of::<u32>(), |f| size_of::<u32>() + f.len()); // File
        len += size_of::<u32>(); // Line
        len += size_of::<u32>(); // Tgid
        len += size_of::<u32>(); // Tid
        len += size_of::<u32>() + self.args.len(); // Args
        len
    }

    pub fn encode<W: io::Write>(&self, mut w: W) -> io::Result<usize> {
        let mut len = 0;
        // Timestamp
        let timestamp = self
            .timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();
        w.write_all(&timestamp.as_secs().to_be_bytes())?;
        w.write_all(&timestamp.subsec_nanos().to_be_bytes())?;
        len += size_of::<u64>() + size_of::<u32>();

        // Level
        w.write_all(&[self.level as u8])?; // Level
        len += size_of::<u8>();

        // Target
        w.write_all(&(self.target.len() as u32).to_be_bytes())?;
        w.write_all(self.target.as_bytes())?;
        len += size_of::<u32>() + self.target.len();

        // File
        if let Some(file) = &self.file {
            w.write_all(&(file.len() as u32).to_be_bytes())?;
            w.write_all(file.as_bytes())?;
            len += size_of::<u32>() + file.len();
        } else {
            w.write_all(&0u32.to_be_bytes())?;
            len += size_of::<u32>();
        }

        // Line
        if let Some(line) = self.line {
            w.write_all(&line.to_be_bytes())?;
        } else {
            w.write_all(&0u32.to_be_bytes())?;
        }
        len += size_of::<u32>();

        // Tgid
        w.write_all(&self.tgid.to_be_bytes())?;
        len += size_of::<u32>();

        // Tid
        w.write_all(&self.tid.to_be_bytes())?;
        len += size_of::<u32>();

        // Args
        w.write_all(&(self.args.len() as u32).to_be_bytes())?;
        w.write_all(self.args)?;
        len += size_of::<u32>() + self.args.len();

        // Add the length field
        Ok(len)
    }
}

/// Log record that can be decoded. This is the owned version.
#[derive(Debug)]
pub struct OwnedRecord {
    pub timestamp: SystemTime,
    pub level: Level,
    pub target: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub tgid: u32,
    pub tid: u32,
    pub args: String,
}

impl OwnedRecord {
    pub fn decode(r: &[u8]) -> io::Result<OwnedRecord> {
        let mut r = io::Cursor::new(&r);

        // Timestamp
        let timestamp = {
            let timestamp_secs = read_u64_be(&mut r)?;
            let timestamp_nanos = read_u32_be(&mut r)?;
            SystemTime::UNIX_EPOCH
                .checked_add(Duration::new(timestamp_secs, timestamp_nanos))
                .unwrap()
        };

        // Level
        let level = match read_u8_be(&mut r)? {
            1 => Level::Error,
            2 => Level::Warn,
            3 => Level::Info,
            4 => Level::Debug,
            5 => Level::Trace,
            _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid level")),
        };

        // Target
        let target = {
            let target_len = read_u32_be(&mut r)? as usize;
            let mut buf = vec![0u8; target_len];
            r.read_exact(&mut buf)?;
            String::from_utf8(buf)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid target"))?
        };

        // File
        let file = {
            let file_len = read_u32_be(&mut r)? as usize;
            if file_len > 0 {
                let mut buf = vec![0u8; file_len];
                r.read_exact(&mut buf)?;
                Some(
                    String::from_utf8(buf)
                        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid file"))?,
                )
            } else {
                None
            }
        };

        // Line
        let line = match read_u32_be(&mut r)? {
            0 => None,
            n => Some(n),
        };

        let tgid = read_u32_be(&mut r)?;
        let tid = read_u32_be(&mut r)?;

        // Args
        let args = {
            let args_len = read_u32_be(&mut r)? as usize;
            let mut buf = vec![0u8; args_len];
            r.read_exact(&mut buf)?;
            String::from_utf8(buf)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid args"))?
        };

        Ok(OwnedRecord {
            timestamp,
            level,
            target,
            file,
            line,
            tgid,
            tid,
            args,
        })
    }
}

fn read_u8_be<R: io::Read>(mut r: R) -> io::Result<u8> {
    let mut buf = [0u8; size_of::<u8>()];
    r.read_exact(&mut buf)?;
    Ok(u8::from_be_bytes(buf))
}

fn read_u32_be<R: io::Read>(mut r: R) -> io::Result<u32> {
    let mut buf = [0u8; size_of::<u32>()];
    r.read_exact(&mut buf)?;
    Ok(u32::from_be_bytes(buf))
}

fn read_u64_be<R: io::Read>(mut r: R) -> io::Result<u64> {
    let mut buf = [0u8; size_of::<u64>()];
    r.read_exact(&mut buf)?;
    Ok(u64::from_be_bytes(buf))
}

#[cfg(test)]
mod test {
    use super::{read_u32_be, read_u64_be, Record};
    use crate::record::OwnedRecord;
    use std::io;

    #[test]
    fn read_u32_be_good() {
        let buf = [0x12, 0x34, 0x56, 0x78];
        assert_eq!(read_u32_be(io::Cursor::new(&buf)).unwrap(), 0x12345678);
    }

    #[test]
    fn read_u32_be_short() {
        let buf = [0x12, 0x34, 0x56];
        assert!(read_u32_be(io::Cursor::new(&buf)).is_err());
    }

    #[test]
    fn read_u64_be_good() {
        let buf = &mut [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0];
        assert_eq!(
            read_u64_be(io::Cursor::new(&buf)).unwrap(),
            0x123456789abcdef0
        );
    }

    #[test]
    fn read_u64_be_short() {
        let buf = &mut [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde];
        assert!(read_u64_be(io::Cursor::new(&buf)).is_err());
    }

    #[test]
    fn encode_decode() {
        fn do_it(record: Record) {
            let mut buf = Vec::new();
            record.encode(&mut buf).unwrap();

            let decoded = OwnedRecord::decode(&buf[..]).unwrap();
            assert_eq!(decoded.timestamp, record.timestamp);
            assert_eq!(decoded.level, record.level);
            assert_eq!(decoded.target, record.target);
            assert_eq!(decoded.file.as_deref(), record.file);
            assert_eq!(decoded.line, record.line);
            assert_eq!(decoded.args.as_bytes(), record.args);
        }

        // Full
        do_it(Record::new(
            feo_time::SystemTime::now(),
            feo_log::Level::Info,
            "target",
            Some("file"),
            Some(42),
            1,
            2,
            b"args",
        ));

        // Empty target
        do_it(Record::new(
            feo_time::SystemTime::now(),
            feo_log::Level::Info,
            "",
            Some("file"),
            Some(42),
            1,
            2,
            b"args",
        ));

        // Empty file
        do_it(Record::new(
            feo_time::SystemTime::now(),
            feo_log::Level::Info,
            "target",
            None,
            Some(42),
            1,
            2,
            b"args",
        ));

        // Empty line
        do_it(Record::new(
            feo_time::SystemTime::now(),
            feo_log::Level::Info,
            "target",
            Some("file"),
            None,
            1,
            2,
            b"args",
        ));

        // Empty args
        do_it(Record::new(
            feo_time::SystemTime::now(),
            feo_log::Level::Info,
            "target",
            Some("file"),
            Some(42),
            1,
            2,
            b"",
        ));
    }
}
