// Copyright (C) 2026 Endveal Entertainment
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://gnu.org>.

use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    pub fn new(prefix: &str) -> Result<Self, Box<dyn Error>> {
        let base = env::temp_dir();
        let pid = std::process::id();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "system clock is before UNIX_EPOCH")?;

        let path = base.join(format!(
            "{}{}_{}_{}",
            prefix,
            pid,
            now.as_secs(),
            now.subsec_nanos()
        ));

        fs::create_dir(&path)?;

        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
