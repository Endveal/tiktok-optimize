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
    error::Error,
    path::Path,
    process::{Command, Stdio},
};

pub fn ffmpeg_video_remux(
    ffmpeg: &str,
    input_path: &str,
    output_path: &Path,
) -> Result<(), Box<dyn Error>> {
    let args = vec![
        "-y".to_owned(),
        "-hide_banner".to_owned(),
        "-loglevel".to_owned(),
        "error".to_owned(),
        "-i".to_owned(),
        input_path.to_owned(),
        "-map".to_owned(),
        "0:v:0".to_owned(),
        "-map".to_owned(),
        "0:a:0?".to_owned(),
        "-c".to_owned(),
        "copy".to_owned(),
        "-map_metadata".to_owned(),
        "0".to_owned(),
        "-movflags".to_owned(),
        "+faststart".to_owned(),
        "-metadata:s:v:0".to_owned(),
        "handler_name=VideoHandler".to_owned(),
        "-metadata:s:a:0".to_owned(),
        "handler_name=SoundHandler".to_owned(),
        output_path.display().to_string(),
    ];

    print!("[DEBUG]");
    for arg in &args {
        print!(" {}", arg);
    }
    println!();

    let proc = Command::new(ffmpeg)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;

    if !proc.status.success() {
        let stderr = String::from_utf8_lossy(&proc.stderr);
        return Err(format!("Gagal menjalankan FFmpeg:\n{}", stderr).into());
    }

    Ok(())
}
