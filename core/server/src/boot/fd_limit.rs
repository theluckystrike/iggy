// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

//! The process's own `RLIMIT_NOFILE`, raised once at startup.

use nix::errno::Errno;
use nix::sys::resource::{Resource, getrlimit, setrlimit};
use thiserror::Error;

/// `OPEN_MAX` from `<sys/syslimits.h>`, which `libc` does not export. The
/// macOS hard limit is usually `RLIM_INFINITY`, and `setrlimit(2)` rejects
/// that as a soft `RLIMIT_NOFILE` with `EINVAL`, so the man page's recipe is
/// `min(OPEN_MAX, rlim_max)`.
#[cfg(target_vendor = "apple")]
const APPLE_OPEN_MAX: u64 = 10_240;

/// `RLIMIT_NOFILE` around [`raise_open_file_limit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenFileLimit {
    pub soft_before: u64,
    pub soft: u64,
    pub hard: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum OpenFileLimitError {
    #[error("cannot read RLIMIT_NOFILE: {0}")]
    Read(Errno),
    #[error(
        "cannot raise the RLIMIT_NOFILE soft limit from {soft} to {target} (hard {hard}): {errno}"
    )]
    Raise {
        errno: Errno,
        soft: u64,
        hard: u64,
        target: u64,
    },
}

/// Raise the soft `RLIMIT_NOFILE` to the hard limit (clamped on macOS).
/// A soft limit already at or above the target is left as it is.
///
/// # Errors
///
/// [`OpenFileLimitError::Read`] if the limit cannot be read, and
/// [`OpenFileLimitError::Raise`] if `setrlimit` rejects the new soft limit.
pub fn raise_open_file_limit() -> Result<OpenFileLimit, OpenFileLimitError> {
    let (soft_before, hard) =
        getrlimit(Resource::RLIMIT_NOFILE).map_err(OpenFileLimitError::Read)?;
    let target = soft_target(hard);
    if soft_before >= target {
        return Ok(OpenFileLimit {
            soft_before,
            soft: soft_before,
            hard,
        });
    }
    setrlimit(Resource::RLIMIT_NOFILE, target, hard).map_err(|errno| {
        OpenFileLimitError::Raise {
            errno,
            soft: soft_before,
            hard,
            target,
        }
    })?;
    Ok(OpenFileLimit {
        soft_before,
        soft: target,
        hard,
    })
}

#[cfg(target_vendor = "apple")]
const fn soft_target(hard: u64) -> u64 {
    if hard < APPLE_OPEN_MAX {
        hard
    } else {
        APPLE_OPEN_MAX
    }
}

#[cfg(not(target_vendor = "apple"))]
const fn soft_target(hard: u64) -> u64 {
    hard
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_process_limit_when_raising_should_leave_reported_soft_limit_in_effect() {
        let limit = raise_open_file_limit().expect("RLIMIT_NOFILE must be raisable in tests");

        let (soft, hard) = getrlimit(Resource::RLIMIT_NOFILE).expect("RLIMIT_NOFILE readable");
        assert_eq!(soft, limit.soft);
        assert_eq!(hard, limit.hard);
        assert!(limit.soft >= limit.soft_before);
        #[cfg(target_os = "linux")]
        assert_eq!(soft, hard);
    }
}
