// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

use crate::record::{OwnedRecord, Record};
use console::{style, Color, StyledObject};
use core::str;
use feo_log::Level;
use feo_time::SystemTime;
use std::sync::atomic::{self, AtomicUsize, Ordering};
use time::format_description::FormatItem;
use time::macros::format_description;

// TODO: Add monochrome support.

const TIMESTAMP_FORMAT: &[FormatItem<'static>] =
    format_description!("[hour]:[minute]:[second].[subsecond digits:3]");

static TARGET_SIZE: atomic::AtomicUsize = atomic::AtomicUsize::new(16);
static TGID_SIZE: atomic::AtomicUsize = atomic::AtomicUsize::new(4);
static TID_SIZE: atomic::AtomicUsize = atomic::AtomicUsize::new(4);

pub fn format<W: std::io::Write>(record: &Record, mut writer: W) -> Result<(), std::io::Error> {
    let timestamp = {
        let timestamp = record.timestamp;
        let timestamp = time::OffsetDateTime::from_unix_timestamp_nanos(
            timestamp
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as i128,
        )
        .unwrap();
        timestamp
            .format(TIMESTAMP_FORMAT)
            .expect("failed to format timestamp")
    };

    let level = {
        let level_color = match record.level {
            Level::Error => Color::Red,
            Level::Warn => Color::Yellow,
            Level::Info => Color::Green,
            Level::Debug => Color::Color256(243),
            Level::Trace => Color::White,
        };
        style(record.level).bold().fg(level_color)
    };

    let tgid = format_id(record.tgid, &TGID_SIZE, true);
    let tid = format_id(record.tid, &TID_SIZE, false);

    let message = unsafe { str::from_utf8_unchecked(record.args) };

    let target = {
        let target = record.target;
        TARGET_SIZE.fetch_max(target.len(), Ordering::Relaxed);
        let target_size = TARGET_SIZE.load(Ordering::Relaxed);
        let target_color = target.color();
        style(format!("{target:<s$}", s = target_size)).fg(target_color)
    };

    // Log location on trace level - otherwise just the message.
    if record.level == Level::Trace {
        let file = record.file.unwrap_or("file unknown");
        let file = style(file).fg(file.color());
        let line = record.line.unwrap_or(0);
        writeln!(
            writer,
            "{timestamp} {target} ({tgid} {tid}): {level:<5}: {file}:{line}: {message}",
        )
    } else {
        writeln!(
            writer,
            "{timestamp} {target} ({tgid} {tid}): {level:<5}: {message}"
        )
    }
}

pub fn format_owned<W: std::io::Write>(
    record: OwnedRecord,
    writer: W,
) -> Result<(), std::io::Error> {
    let record = Record {
        timestamp: record.timestamp,
        level: record.level,
        target: &record.target,
        file: record.file.as_deref(),
        line: record.line,
        tgid: record.tgid,
        tid: record.tid,
        args: record.args.as_bytes(),
    };
    format(&record, writer)
}

/// Generate a color of `self`.
trait HashColor {
    fn color(&self) -> Color;
}

impl HashColor for &str {
    fn color(&self) -> Color {
        let hash = self.bytes().fold(42u8, |c, x| c ^ x);
        Color::Color256(hash)
    }
}

impl HashColor for u32 {
    fn color(&self) -> Color {
        (*self as u64).color()
    }
}

impl HashColor for u64 {
    fn color(&self) -> Color {
        // Some colors are hard to read on (at least) dark terminals
        // and I consider some others as ugly ;-)
        let color = match *self as u8 {
            c @ 0..=1 => c + 2,
            c @ 16..=21 => c + 6,
            c @ 52..=55 | c @ 126..=129 => c + 4,
            c @ 163..=165 | c @ 200..=201 => c + 3,
            c @ 207 => c + 1,
            c @ 232..=240 => c + 9,
            c => c,
        };
        Color::Color256(color)
    }
}

/// Format `id` with a color based on the hash of `id`. Update `g` with the
/// maximum length of the formatted `id`.
fn format_id(id: u32, g: &AtomicUsize, align_left: bool) -> StyledObject<String> {
    let tgid_len = num_hex_digits(id);
    let color = id.color();
    g.fetch_max(tgid_len, Ordering::Relaxed);
    let len = g.load(Ordering::Relaxed);
    if align_left {
        style(format!("{:<l$x}", id, l = len)).fg(color)
    } else {
        style(format!("{:>l$x}", id, l = len)).fg(color)
    }
}

// Calculate the number of hex digits needed to represent `n`.
fn num_hex_digits(n: u32) -> usize {
    (1 + n.checked_ilog2().unwrap_or_default() / 4) as usize
}

#[cfg(test)]
mod test {
    use super::num_hex_digits;

    #[test]
    fn hex_digits() {
        assert_eq!(num_hex_digits(0), 1);
        assert_eq!(num_hex_digits(1), 1);
        assert_eq!(num_hex_digits(15), 1);
        assert_eq!(num_hex_digits(16), 2);
        assert_eq!(num_hex_digits(255), 2);
        assert_eq!(num_hex_digits(256), 3);
        assert_eq!(num_hex_digits(4095), 3);
    }
}
