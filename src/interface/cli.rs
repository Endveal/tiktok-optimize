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
    fs::{File, OpenOptions},
    io::{self, Write},
    path::Path,
};

pub fn run_cli(args: crate::core::args_parser::CliArgs, temp_file_prefix: &str, ffmpeg_path: &str) {
    let input_path = args.input.as_str();
    let output_path = args.output.as_str();

    println!("[INFO] input  : {input_path}");
    println!("[INFO] output : {output_path}");

    if !Path::new(&input_path).exists() {
        eprintln!("[ERROR] Input file not found: {input_path}");
        crate::core::utils::exit(0);
    }
    if input_path == output_path {
        eprintln!("[ERROR] The input and output must be different files");
        crate::core::utils::exit(0);
    }

    if crate::core::utils::preflight_codec(input_path).is_err() {
        crate::core::utils::exit(0);
    }

    let temp_guard =
        crate::core::temp_guard::TempDirGuard::new(temp_file_prefix).unwrap_or_else(|err| {
            eprintln!("[ERROR] {}", err);
            crate::core::utils::exit(0)
        });
    let temp_path = temp_guard.path().join("ffmpeg-remux.mp4");

    if let Err(err) = crate::core::ffmpeg::ffmpeg_video_remux(ffmpeg_path, input_path, &temp_path) {
        eprintln!("[ERROR] {}", err);
        crate::core::utils::exit(0);
    };

    let mut input = OpenOptions::new()
        .read(true)
        .open(temp_path)
        .unwrap_or_else(|err| {
            eprintln!("[ERROR] {}", err);
            crate::core::utils::exit(0)
        });
    let file_len = input.metadata().unwrap().len();
    if file_len < 16 {
        eprintln!("[ERROR] input is too small to be an MP4");
        crate::core::utils::exit(0);
    }

    let mut info = crate::core::engine::VideoInfo::default();

    crate::core::engine::find_video_info(&mut input, 0, file_len, false, &mut info).unwrap_or_else(
        |err| {
            eprintln!("[ERROR] {}", err);
            crate::core::utils::exit(0)
        },
    );
    crate::core::engine::parse_video_info(&mut input, &mut info).unwrap_or_else(|err| {
        eprintln!("[ERROR] {}", err);
        crate::core::utils::exit(0)
    });

    let video_trak = info
        .trak
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no video trak found"))
        .unwrap_or_else(|err| {
            eprintln!("[ERROR] {}", err);
            crate::core::utils::exit(0)
        });
    let stsz = info.stsz.unwrap();
    let stsc = info.stsc.unwrap();
    if info.stco.is_some() && info.co64.is_some() {
        eprintln!("[ERROR] video track contains both stco and co64; refusing ambiguous input");
        crate::core::utils::exit(0);
    }
    let offsets = info.stco.or(info.co64).unwrap();

    let (stts_entry_count, stts_samples, stts_fixed_delta, stts_last_count, stts_last_delta) =
        match info.stts {
            Some(h) => crate::core::engine::parse_stts_info(&mut input, h).unwrap_or_else(|err| {
                eprintln!("[ERROR] {}", err);
                crate::core::utils::exit(0)
            }),
            None => {
                eprintln!("[ERROR] video stts not found");
                crate::core::utils::exit(0);
            }
        };

    info.stts_entry_count = stts_entry_count;
    info.stts_sample_count = stts_samples;
    info.stts_fixed_delta = stts_fixed_delta;
    info.stts_last_count = stts_last_count;
    info.stts_last_delta = stts_last_delta;

    println!(
        "[INFO] video trak: offset={} size={}",
        video_trak.offset, video_trak.size
    );
    println!(
        "[INFO] stsz: offset={} size={} sample_size={} sample_count={}",
        stsz.offset, stsz.size, info.stsz_sample_size, info.stsz_sample_count
    );
    println!(
        "[INFO] stsc: offset={} size={} entries={} last_desc_id={} chunks={}",
        stsc.offset, stsc.size, info.stsc_entry_count, info.stsc_last_desc_id, info.chunk_count
    );
    println!(
        "[INFO] {}: offset={} size={} original_chunks={}",
        offsets.typ.as_str(),
        offsets.offset,
        offsets.size,
        info.chunk_count
    );
    println!(
        "[INFO] stts: entries={} samples={} fixed_delta={} (first original delta)",
        info.stts_entry_count, stts_samples, info.stts_fixed_delta
    );
    if stts_samples != info.stsz_sample_count as u64 {
        println!(
            "[WARN] input already has stts/stsz sample-count mismatch: stts={} stsz={}",
            stts_samples, info.stsz_sample_count
        );
    }
    if stts_samples != info.stsz_sample_count as u64 {
        eprintln!(
            "[ERROR] refusing to rebuild stts because input stts sample count does not match stsz sample count"
        );
        crate::core::utils::exit(0);
    }

    let stts_box = info.stts.unwrap();

    let video_timescale = crate::core::engine::get_video_timescale(&mut input, &video_trak)
        .unwrap_or_else(|err| {
            eprintln!("[ERROR] {}", err);
            crate::core::utils::exit(0)
        });

    let total_duration_ticks = crate::core::engine::calculate_stts_duration(&mut input, stts_box)
        .unwrap_or_else(|err| {
            eprintln!("[ERROR] {}", err);
            crate::core::utils::exit(0)
        });

    let duration_seconds = total_duration_ticks as f64 / video_timescale as f64;

    let real_sample_count = info.stsz_sample_count as u64;

    let target_samples_f64 = duration_seconds * crate::core::engine::TARGET_SAMPLE_DENSITY;

    let target_samples = target_samples_f64.round() as u64;

    let fake_count = target_samples.saturating_sub(real_sample_count);

    if fake_count > u32::MAX as u64 {
        eprintln!(
            "[ERROR] fake sample count too large for u32: {}",
            fake_count
        );
        crate::core::utils::exit(0);
    }

    let fake_samples = fake_count as u32;

    println!("[INFO] video timescale       : {}", video_timescale);
    println!("[INFO] duration ticks       : {}", total_duration_ticks);
    println!("[INFO] duration seconds     : {:.6}", duration_seconds);
    println!("[INFO] real samples         : {}", real_sample_count);
    println!("[INFO] target samples       : {}", target_samples);
    println!(
        "[INFO] target density       : {} samples/sec",
        crate::core::engine::TARGET_SAMPLE_DENSITY
    );
    println!("[INFO] fake samples needed  : {}", fake_samples);

    let (mods, final_len) =
        crate::core::engine::calculate_modifications(file_len, &info, fake_samples).unwrap_or_else(
            |err| {
                eprintln!("[ERROR] {}", err);
                crate::core::utils::exit(0)
            },
        );
    let metadata_growth: u64 = mods.iter().map(|m| m.delta).sum();
    let fake_mdat_header_size = 8u64;
    let fake_payload_offset = crate::core::engine::checked_add_u64(
        crate::core::engine::checked_add_u64(file_len, metadata_growth, "fake mdat start")
            .unwrap_or_else(|err| {
                eprintln!("[ERROR] {}", err);
                crate::core::utils::exit(0)
            }),
        fake_mdat_header_size,
        "fake mdat payload offset",
    )
    .unwrap_or_else(|err| {
        eprintln!("[ERROR] {}", err);
        crate::core::utils::exit(0)
    });

    println!("[INFO] metadata growth: +{} bytes", metadata_growth);
    for m in &mods {
        println!(
            "[DEBUG] insertion/growth at old file offset {}: +{} bytes",
            m.offset, m.delta
        );
    }
    println!(
        "[INFO] fake payload is at final file offset {} (8 bytes)",
        fake_payload_offset
    );
    println!("[INFO] output size will be {} bytes", final_len);

    let mut output = File::create(output_path).unwrap_or_else(|err| {
        eprintln!("[ERROR] {}", err);
        crate::core::utils::exit(0)
    });
    crate::core::engine::stream_top_level(
        &mut input,
        &mut output,
        file_len,
        &mods,
        &info,
        fake_payload_offset,
        fake_samples,
    )
    .unwrap_or_else(|err| {
        eprintln!("[ERROR] {}", err);
        crate::core::utils::exit(0)
    });

    output
        .write_all(&16u32.to_be_bytes())
        .unwrap_or_else(|err| {
            eprintln!("[ERROR] {}", err);
            crate::core::utils::exit(0)
        });
    output.write_all(b"mdat").unwrap_or_else(|err| {
        eprintln!("[ERROR] {}", err);
        crate::core::utils::exit(0)
    });
    output
        .write_all(&crate::core::engine::FAKE_BYTES)
        .unwrap_or_else(|err| {
            eprintln!("[ERROR] {}", err);
            crate::core::utils::exit(0)
        });
    output.flush().unwrap_or_else(|err| {
        eprintln!("[ERROR] {}", err);
        crate::core::utils::exit(0)
    });

    let actual_len = output
        .metadata()
        .unwrap_or_else(|err| {
            eprintln!("[ERROR] {}", err);
            crate::core::utils::exit(0)
        })
        .len();
    println!("[INFO] done: {} bytes written", actual_len);
    println!("[INFO] fake samples added: {}", fake_samples);
    println!(
        "[INFO] stsz new sample_count: {}",
        info.stsz_sample_count as u64 + fake_samples as u64
    );
    println!(
        "[INFO] fake chunks added to {}: {}",
        offsets.typ.as_str(),
        fake_samples
    );
    println!(
        "[INFO] stts rebuilt for real samples only: {} samples",
        stts_samples
    );
    println!(
        "[INFO] stts rule: samples 1..N-1 use delta={}, last sample uses delta=1",
        info.stts_fixed_delta
    );
    println!("[INFO] fake samples are NOT represented in stts");
    println!("[INFO] fake sample size: 8 bytes; all fake chunk offsets are identical");
    println!(
        "[INFO] stts atom growth: +{} bytes",
        mods.iter()
            .find(|m| info.stts.map(|x| x.offset == m.offset).unwrap_or(false))
            .map(|m| m.delta)
            .unwrap_or(0)
    );
    println!();
    println!("[WARN] This intentionally creates a non-conformant/experimental MP4 sample table:");
    println!(
        "       stts describes the original real samples only; fake samples are added to stsz/stsc/stco(or co64)."
    );
    println!("       Multiple fake chunks point to the same 8-byte payload.");
    println!(
        "       Some parsers may ignore the extra samples; others may reject the file or behave differently."
    );
}
