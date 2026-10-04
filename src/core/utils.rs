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
    process::{Command, Stdio},
};

pub fn exit(code: i32) -> ! {
    std::process::exit(code)
}

pub fn print_installation_instructions() {
    println!("\nPlease install FFmpeg manually to use this CLI tool:");

    if cfg!(target_os = "windows") {
        println!("=== WINDOWS ===");
        println!("1. Download the FFmpeg official binaries from https://ffmpeg.org.");
        println!(
            "2. Extract the zip file and add the 'bin' folder path to your System Environment Variables (PATH)."
        );
        println!("3. Alternatively, run this command in a Terminal (as Administrator):");
        println!("   winget install Gnu.FFmpeg");
    } else if cfg!(target_os = "linux") {
        println!("=== LINUX / TERMUX ===");
        println!("• Ubuntu/Debian : sudo apt update && sudo apt install ffmpeg");
        println!("• Arch Linux    : sudo pacman -S ffmpeg");
        println!("• Android Termux: pkg install ffmpeg");
    } else {
        println!("• Please use your operating system's package manager to install FFmpeg.");
    }
    println!("\nRestart this application once the installation is complete.");
}

pub fn is_ffmpeg_available() -> bool {
    match Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
    {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    Hevc,
    Vp9,
    Vp8,
    Av1,
    Mpeg4,
    Mpeg2,
    Unknown,
}

impl VideoCodec {
    pub fn from_ffprobe(value: &str) -> Self {
        match value {
            "h264" => Self::H264,
            "hevc" => Self::Hevc,
            "vp9" => Self::Vp9,
            "vp8" => Self::Vp8,
            "av1" => Self::Av1,
            "mpeg4" => Self::Mpeg4,
            "mpeg2video" => Self::Mpeg2,
            _ => Self::Unknown,
        }
    }
}

pub fn detect_video_codec(input: &str) -> Result<VideoCodec, Box<dyn Error>> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(input)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe failed: {}", stderr.trim()).into());
    }

    let codec = String::from_utf8(output.stdout)?.trim().to_lowercase();

    if codec.is_empty() {
        return Err("no video stream found".into());
    }

    Ok(VideoCodec::from_ffprobe(&codec))
}

pub fn preflight_ffmpeg() -> Result<(), ()> {
    println!("[INFO] Checking for FFmpeg availability...");
    if is_ffmpeg_available() {
        println!("[INFO] FFmpeg found! Processing with the application...");
    } else {
        eprintln!("\n[ERROR] FFmpeg is not installed or not found in your system's PATH!");
        print_installation_instructions();
        return Err(());
    }

    Ok(())
}

pub fn preflight_codec(input: &str) -> Result<(), ()> {
    println!("[INFO] Checking for codec compability...");
    let codec_ffprobe = detect_video_codec(input).map_err(|_| ())?;
    if ![&VideoCodec::H264, &VideoCodec::Hevc, &VideoCodec::Mpeg4].contains(&&codec_ffprobe) {
        println!("[ERROR] Codec not supported; wait for the maintainer to update.");
        println!(
            "[INFO] Support Codec: {}",
            [&VideoCodec::H264, &VideoCodec::Hevc, &VideoCodec::Mpeg4]
                .iter()
                .map(|c| format!("{:?}", c))
                .collect::<Vec<_>>()
                .join(", ")
        );
        println!("[INFO] Current video Codec: {:?}", codec_ffprobe);
        return Err(());
    }
    println!("[INFO] Current video Codec: {:?}", codec_ffprobe);

    Ok(())
}
