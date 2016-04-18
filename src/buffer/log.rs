//! This module implements the disk logging system for buffers.
use std::path::PathBuf;
use std::io::{Read, Write};
use std::fs::{File, OpenOptions, DirBuilder};
use time::{Tm, Duration, now};
use rustc_serialize::json::{decode, encode};

use common::line::BufferLine;


/// Represents a handle for reading and writing to on-disk log files.
#[derive(Debug, Clone)]
pub struct BufferLog {
    dir: PathBuf,
    /// The last day's log we've read. This is stored as Tm, but any precision
    /// past days is ignored.
    next_read_day: Tm,
}

impl BufferLog {
    pub fn new(path: PathBuf) -> BufferLog {
        DirBuilder::new().recursive(true).create(&path).unwrap();
        BufferLog {
            dir: path,
            next_read_day: now() - Duration::days(1),
        }
    }

    /// Writes the given lines to today's log.
    pub fn write_lines(&mut self, lines: Vec<BufferLine>) {
        let path = self.file_for_day(&now());
        DirBuilder::new().recursive(true).create(&path.parent().unwrap()).unwrap();
        match OpenOptions::new().create(true).write(true).append(true).open(&path) {
            Err(e) => error!("Error opening log file for writing: {}", e),
            Ok(mut f) => {
                for line in lines {
                    let mut data = encode(&line).unwrap();
                    data.push('\n');
                    f.write_all(data.as_bytes()).expect("Failed writing to log file");
                }
            },
        }
    }

    /// Reads the lines for the given day.
    pub fn lines_for_day(&mut self, day: &Tm) -> Vec<BufferLine> {
        let path = self.file_for_day(day);
        trace!("Fetching lines from {}", path.display());
        let mut data = String::new();

        if let Ok(mut f) = File::open(&path) {
            if let Err(e) = f.read_to_string(&mut data) {
                error!("Error reading log file: {}", e);
                return vec![];
            }

            let lines = data.lines().flat_map(|l| {
                decode(l).ok()
            }).rev().collect();
            lines
        } else {
            vec![]
        }
    }

    /// Reads a batch of lines from the log files.
    ///
    /// This usually just reads an entire file of logs, but may vary.
    pub fn fetch_lines(&mut self) -> Vec<BufferLine> {
        let day = self.next_read_day.clone();
        let lines = self.lines_for_day(&day);
        self.next_read_day = (self.next_read_day - Duration::days(1)).to_local();
        trace!("Next day: {} Lines: {:?}", self.next_read_day.ctime(), lines);
        lines
    }

    fn file_for_day(&self, day: &Tm) -> PathBuf {
        let mut path = self.dir.clone();
        path.push(format!("{}", day.tm_year + 1900));
        path.push(format!("{}", day.tm_mon + 1));
        path.push(format!("{}", day.tm_mday));
        path
    }
}
