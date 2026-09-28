// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Resource measurement, on the definition the whole project shares: peak memory
//! from `/proc/self/status` `VmHWM`, time from `CLOCK_MONOTONIC`.

use std::time::{Duration, Instant};

/// A wall-clock stopwatch.
#[derive(Debug)]
pub struct Stopwatch {
    started: Instant,
}

impl Stopwatch {
    pub fn start() -> Self {
        Self {
            started: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }
}

/// Peak resident set size in megabytes, or `None` where the kernel does not
/// report it.
pub fn peak_rss_mb() -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        let Some(rest) = line.strip_prefix("VmHWM:") else {
            continue;
        };
        let kilobytes: f64 = rest.split_whitespace().next()?.parse().ok()?;
        return Some(kilobytes / 1024.0);
    }
    None
}
