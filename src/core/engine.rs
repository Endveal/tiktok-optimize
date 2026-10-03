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
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
};

pub const FAKE_BYTES: [u8; 8] = *b"ENDVEAL1";
pub const TARGET_SAMPLE_DENSITY: f64 = 400.0;

const COPY_BUF_SIZE: usize = 1024 * 1024;

#[derive(Debug, Default)]
pub struct VideoInfo {
    pub trak: Option<BoxHeader>,
    pub stsz: Option<BoxHeader>,
    pub stsc: Option<BoxHeader>,
    pub stco: Option<BoxHeader>,
    pub co64: Option<BoxHeader>,
    pub stts: Option<BoxHeader>,
    pub stsz_sample_size: u32,
    pub stsz_sample_count: u32,
    pub stts_entry_count: u32,
    pub stts_sample_count: u64,
    pub stts_fixed_delta: u32,
    pub stsc_entry_count: u32,
    pub stsc_last_desc_id: u32,
    pub chunk_count: u32,
    pub offset_is_co64: bool,
    pub stts_last_count: u32,
    pub stts_last_delta: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Modification {
    pub offset: u64,
    pub delta: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FourCC([u8; 4]);

impl FourCC {
    pub fn from_bytes(b: [u8; 4]) -> Self {
        Self(b)
    }
    pub fn as_str(&self) -> String {
        self.0
            .iter()
            .map(|&c| if c.is_ascii_graphic() { c as char } else { '.' })
            .collect()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BoxHeader {
    pub offset: u64,
    pub size: u64,
    pub header_size: u64,
    pub typ: FourCC,
}

impl BoxHeader {
    pub fn end(&self) -> u64 {
        self.offset + self.size
    }
    pub fn payload_start(&self) -> u64 {
        self.offset + self.header_size
    }
    pub fn payload_size(&self) -> u64 {
        self.size - self.header_size
    }
}

pub fn read_u32_at(file: &mut File, pos: u64) -> io::Result<u32> {
    file.seek(SeekFrom::Start(pos))?;
    let mut b = [0u8; 4];
    file.read_exact(&mut b)?;
    Ok(u32::from_be_bytes(b))
}

pub fn read_u64_at(file: &mut File, pos: u64) -> io::Result<u64> {
    file.seek(SeekFrom::Start(pos))?;
    let mut b = [0u8; 8];
    file.read_exact(&mut b)?;
    Ok(u64::from_be_bytes(b))
}

pub fn read_fourcc_at(file: &mut File, pos: u64) -> io::Result<FourCC> {
    file.seek(SeekFrom::Start(pos))?;
    let mut b = [0u8; 4];
    file.read_exact(&mut b)?;
    Ok(FourCC::from_bytes(b))
}

pub fn read_box_header(
    file: &mut File,
    offset: u64,
    container_end: u64,
) -> io::Result<Option<BoxHeader>> {
    if offset >= container_end {
        return Ok(None);
    }
    if container_end - offset < 8 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("truncated MP4 box header at {offset}"),
        ));
    }

    let size32 = read_u32_at(file, offset)?;
    let typ = read_fourcc_at(file, offset + 4)?;
    let (size, header_size) = if size32 == 1 {
        if container_end - offset < 16 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "truncated extended MP4 box header",
            ));
        }
        (read_u64_at(file, offset + 8)?, 16)
    } else if size32 == 0 {
        (container_end - offset, 8)
    } else {
        (size32 as u64, 8)
    };

    if size < header_size || offset + size > container_end {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "invalid box size for {} at {}: size={size}, header={header_size}, container_end={container_end}",
                typ.as_str(),
                offset
            ),
        ));
    }

    Ok(Some(BoxHeader {
        offset,
        size,
        header_size,
        typ,
    }))
}

pub fn is_container(typ: FourCC) -> bool {
    typ.0 == *b"moov"
        || typ.0 == *b"trak"
        || typ.0 == *b"mdia"
        || typ.0 == *b"minf"
        || typ.0 == *b"stbl"
}

pub fn find_video_info(
    file: &mut File,
    start: u64,
    end: u64,
    in_video_trak: bool,
    info: &mut VideoInfo,
) -> io::Result<()> {
    let mut pos = start;
    while pos < end {
        let Some(h) = read_box_header(file, pos, end)? else {
            break;
        };

        let now_in_video_trak =
            in_video_trak || h.typ == FourCC(*b"trak") && has_video_hdlr(file, &h)?;
        if h.typ == FourCC(*b"trak") && now_in_video_trak && info.trak.is_none() {
            info.trak = Some(h);
        }

        if now_in_video_trak {
            if h.typ.0 == *b"stsz" && info.stsz.is_none() {
                info.stsz = Some(h);
            } else if h.typ.0 == *b"stsc" && info.stsc.is_none() {
                info.stsc = Some(h);
            } else if h.typ.0 == *b"stco" && info.stco.is_none() {
                info.stco = Some(h);
            } else if h.typ.0 == *b"co64" && info.co64.is_none() {
                info.co64 = Some(h);
            } else if h.typ.0 == *b"stts" && info.stts.is_none() {
                info.stts = Some(h);
            }
        }

        if is_container(h.typ) && h.payload_size() > 0 {
            find_video_info(file, h.payload_start(), h.end(), now_in_video_trak, info)?;
        }

        pos = h.end();
    }
    Ok(())
}

pub fn has_video_hdlr(file: &mut File, trak: &BoxHeader) -> io::Result<bool> {
    let mut pos = trak.payload_start();
    while pos < trak.end() {
        let Some(h) = read_box_header(file, pos, trak.end())? else {
            break;
        };
        if h.typ == FourCC(*b"mdia") {
            let mut p = h.payload_start();
            while p < h.end() {
                let Some(c) = read_box_header(file, p, h.end())? else {
                    break;
                };
                if c.typ == FourCC(*b"hdlr") {
                    if c.payload_size() < 12 {
                        return Ok(false);
                    }
                    let handler = read_fourcc_at(file, c.payload_start() + 8)?;
                    return Ok(handler == FourCC(*b"vide"));
                }
                p = c.end();
            }
        }
        pos = h.end();
    }
    Ok(false)
}

pub fn get_video_timescale(file: &mut File, video_trak: &BoxHeader) -> io::Result<u32> {
    let mut pos = video_trak.payload_start();

    while pos < video_trak.end() {
        let Some(h) = read_box_header(file, pos, video_trak.end())? else {
            break;
        };

        if h.typ == FourCC(*b"mdia") {
            let mut p = h.payload_start();

            while p < h.end() {
                let Some(c) = read_box_header(file, p, h.end())? else {
                    break;
                };

                if c.typ == FourCC(*b"mdhd") {
                    if c.payload_size() < 20 {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid mdhd"));
                    }

                    let version = read_u32_at(file, c.payload_start())? >> 24;

                    let timescale_pos = if version == 0 {
                        c.payload_start() + 12
                    } else if version == 1 {
                        if c.payload_size() < 32 {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "invalid version-1 mdhd",
                            ));
                        }

                        c.payload_start() + 20
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("unsupported mdhd version: {version}"),
                        ));
                    };

                    let timescale = read_u32_at(file, timescale_pos)?;

                    if timescale == 0 {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "video mdhd timescale is zero",
                        ));
                    }

                    return Ok(timescale);
                }

                p = c.end();
            }
        }

        pos = h.end();
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "video mdhd not found",
    ))
}

pub fn parse_stts_info(file: &mut File, stts: BoxHeader) -> io::Result<(u32, u64, u32, u32, u32)> {
    let entry_count = read_u32_at(file, stts.payload_start() + 4)?;

    let first_count = read_u32_at(file, stts.payload_start() + 8)?;
    let first_delta = read_u32_at(file, stts.payload_start() + 12)?;
    if first_count == 0 || first_delta == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stts contains zero sample_count or zero delta",
        ));
    }

    let mut total = 0u64;
    let mut pos = stts.payload_start() + 8;
    for _ in 0..entry_count {
        let sample_count = read_u32_at(file, pos)? as u64;
        total = total.checked_add(sample_count).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "stts sample count overflow")
        })?;
        pos += 8;
    }

    let last_pos = stts.payload_start() + 8 + (entry_count as u64 - 1) * 8;
    let last_count = read_u32_at(file, last_pos)?;
    let last_delta = read_u32_at(file, last_pos + 4)?;
    if last_count == 0 || last_delta == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stts last entry has zero sample_count or zero delta",
        ));
    }

    Ok((entry_count, total, first_delta, last_count, last_delta))
}

pub fn calculate_stts_duration(file: &mut File, stts: BoxHeader) -> io::Result<u64> {
    let entry_count = read_u32_at(file, stts.payload_start() + 4)?;

    if entry_count == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stts has zero entries",
        ));
    }

    let mut total_duration = 0u64;
    let mut pos = stts.payload_start() + 8;

    for _ in 0..entry_count {
        let sample_count = read_u32_at(file, pos)? as u64;
        let sample_delta = read_u32_at(file, pos + 4)? as u64;

        let duration = sample_count
            .checked_mul(sample_delta)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "stts duration overflow"))?;

        total_duration = total_duration.checked_add(duration).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "stts total duration overflow")
        })?;

        pos += 8;
    }

    Ok(total_duration)
}

pub fn parse_video_info(file: &mut File, info: &mut VideoInfo) -> io::Result<()> {
    let stsz = info
        .stsz
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "video stsz not found"))?;
    let stsc = info
        .stsc
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "video stsc not found"))?;
    let offset_box = if let Some(h) = info.stco {
        h
    } else if let Some(h) = info.co64 {
        h
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "video stco/co64 not found",
        ));
    };

    if stsz.payload_size() < 12 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "stsz too small"));
    }
    let sample_size = read_u32_at(file, stsz.payload_start() + 4)?;
    let sample_count = read_u32_at(file, stsz.payload_start() + 8)?;

    if stsc.payload_size() < 8 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "stsc too small"));
    }
    let entry_count = read_u32_at(file, stsc.payload_start() + 4)?;
    if entry_count == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stsc has zero entries",
        ));
    }
    let expected = 8u64 + entry_count as u64 * 12;
    if stsc.payload_size() < expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stsc entries exceed atom size",
        ));
    }
    let last_entry = stsc.payload_start() + 8 + (entry_count as u64 - 1) * 12;
    let last_desc_id = read_u32_at(file, last_entry + 8)?;

    if offset_box.payload_size() < 8 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stco/co64 too small",
        ));
    }
    let chunk_count = read_u32_at(file, offset_box.payload_start() + 4)?;
    let offset_entry_size = if offset_box.typ == FourCC(*b"co64") {
        8
    } else {
        4
    };
    let expected_offset_bytes = 8u64 + chunk_count as u64 * offset_entry_size;
    if offset_box.payload_size() < expected_offset_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stco/co64 entries exceed atom size",
        ));
    }

    info.stsz_sample_size = sample_size;
    info.stsz_sample_count = sample_count;
    info.stsc_entry_count = entry_count;
    info.stsc_last_desc_id = last_desc_id;
    info.chunk_count = chunk_count;
    info.offset_is_co64 = offset_box.typ == FourCC(*b"co64");
    Ok(())
}

pub fn checked_add_u64(a: u64, b: u64, what: &str) -> io::Result<u64> {
    a.checked_add(b)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("overflow in {what}")))
}

pub fn calculate_modifications(
    file_len: u64,
    info: &VideoInfo,
    fake_samples: u32,
) -> io::Result<(Vec<Modification>, u64)> {
    let stsz = info.stsz.unwrap();
    let stsc = info.stsc.unwrap();
    let offset_box = info.stco.or(info.co64).unwrap();
    let n = fake_samples as u64;

    let stsz_delta = if info.stsz_sample_size == 0 {
        4 * n
    } else {
        4 * (info.stsz_sample_count as u64 + n)
    };
    let stsc_delta = 12;
    let offset_entry_size = if info.offset_is_co64 { 8 } else { 4 };
    let stco_delta = n * offset_entry_size;

    let stts_delta = if info.stts_last_count > 1 { 8 } else { 0 };

    let mut mods = vec![
        Modification {
            offset: stsc.offset,
            delta: stsc_delta,
        },
        Modification {
            offset: stsz.offset,
            delta: stsz_delta,
        },
        Modification {
            offset: offset_box.offset,
            delta: stco_delta,
        },
    ];

    if let Some(stts) = info.stts {
        if stts_delta != 0 {
            mods.push(Modification {
                offset: stts.offset,
                delta: stts_delta,
            });
        }
    }

    let metadata_growth = mods.iter().try_fold(0u64, |acc, m| {
        checked_add_u64(acc, m.delta, "metadata growth")
    })?;

    let final_len = checked_add_u64(
        checked_add_u64(file_len, metadata_growth, "final file size")?,
        16,
        "final file size",
    )?;

    if final_len > u32::MAX as u64 {
        for b in [stsc, stsz, offset_box] {
            if b.header_size == 8
                && checked_add_u64(
                    b.size,
                    mods.iter().find(|m| m.offset == b.offset).unwrap().delta,
                    "atom size",
                )
                .unwrap_or(u64::MAX)
                    > u32::MAX as u64
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "modified atom would exceed 32-bit MP4 box size",
                ));
            }
        }
    }

    Ok((mods, final_len))
}

pub fn sum_deltas_before(mods: &[Modification], pos: u64) -> u64 {
    mods.iter()
        .filter(|m| m.offset < pos)
        .map(|m| m.delta)
        .sum()
}

pub fn sum_deltas_in_range(mods: &[Modification], start: u64, end: u64) -> u64 {
    mods.iter()
        .filter(|m| m.offset >= start && m.offset < end)
        .map(|m| m.delta)
        .sum()
}

pub fn write_box_size<W: Write>(out: &mut W, h: &BoxHeader, new_size: u64) -> io::Result<()> {
    if h.header_size == 8 {
        if new_size > u32::MAX as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "box {} grew beyond 32-bit size but has no extended-size header",
                    h.typ.as_str()
                ),
            ));
        }
        out.write_all(&(new_size as u32).to_be_bytes())?;
        out.write_all(&h.typ.0)?;
    } else {
        out.write_all(&1u32.to_be_bytes())?;
        out.write_all(&h.typ.0)?;
        out.write_all(&new_size.to_be_bytes())?;
    }
    Ok(())
}

pub fn copy_range(input: &mut File, output: &mut File, start: u64, len: u64) -> io::Result<()> {
    input.seek(SeekFrom::Start(start))?;
    let mut left = len;
    let mut buf = vec![0u8; COPY_BUF_SIZE];
    while left > 0 {
        let want = (left as usize).min(buf.len());
        input.read_exact(&mut buf[..want])?;
        output.write_all(&buf[..want])?;
        left -= want as u64;
    }
    Ok(())
}

pub fn write_modified_stts(
    input: &mut File,
    output: &mut File,
    h: BoxHeader,
    info: &VideoInfo,
) -> io::Result<()> {
    let entry_count = info.stts_entry_count;
    let last_count = info.stts_last_count;
    let last_delta = info.stts_last_delta;

    if entry_count == 0 || last_count == 0 || last_delta == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "cannot rebuild empty/invalid stts",
        ));
    }

    let new_entry_count = if last_count > 1 {
        entry_count + 1
    } else {
        entry_count
    };
    let new_size = h.size + (new_entry_count as u64 - entry_count as u64) * 8;

    write_box_size(output, &h, new_size)?;
    copy_range(input, output, h.payload_start(), 4)?;
    output.write_all(&new_entry_count.to_be_bytes())?;

    copy_range(
        input,
        output,
        h.payload_start() + 8,
        (entry_count as u64 - 1) * 8,
    )?;

    if last_count > 1 {
        output.write_all(&(last_count - 1).to_be_bytes())?;
        output.write_all(&last_delta.to_be_bytes())?;
    }

    output.write_all(&1u32.to_be_bytes())?;
    output.write_all(&1u32.to_be_bytes())?;

    Ok(())
}

pub fn write_modified_stsz(
    input: &mut File,
    output: &mut File,
    h: BoxHeader,
    info: &VideoInfo,
    fake_samples: u32,
) -> io::Result<()> {
    let n = fake_samples as u64;
    let new_sample_count = info
        .stsz_sample_count
        .checked_add(fake_samples)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "stsz sample_count overflow"))?;
    let new_size = if info.stsz_sample_size == 0 {
        h.size + 4 * n
    } else {
        h.size + 4 * (info.stsz_sample_count as u64 + n)
    };

    write_box_size(output, &h, new_size)?;

    copy_range(input, output, h.payload_start(), 4)?;

    output.write_all(&0u32.to_be_bytes())?;
    output.write_all(&new_sample_count.to_be_bytes())?;

    if info.stsz_sample_size == 0 {
        let table_start = h.payload_start() + 12;
        let table_len = info.stsz_sample_count as u64 * 4;
        copy_range(input, output, table_start, table_len)?;
    } else {
        for _ in 0..info.stsz_sample_count {
            output.write_all(&info.stsz_sample_size.to_be_bytes())?;
        }
    }

    for _ in 0..fake_samples {
        output.write_all(&8u32.to_be_bytes())?;
    }
    Ok(())
}

pub fn write_modified_stsc(
    input: &mut File,
    output: &mut File,
    h: BoxHeader,
    info: &VideoInfo,
) -> io::Result<()> {
    let new_entry_count = info
        .stsc_entry_count
        .checked_add(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "stsc entry_count overflow"))?;
    let new_size = h.size + 12;

    write_box_size(output, &h, new_size)?;
    copy_range(input, output, h.payload_start(), 4)?;
    output.write_all(&new_entry_count.to_be_bytes())?;
    copy_range(
        input,
        output,
        h.payload_start() + 8,
        info.stsc_entry_count as u64 * 12,
    )?;

    let first_fake_chunk = info
        .chunk_count
        .checked_add(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "chunk count overflow"))?;
    output.write_all(&first_fake_chunk.to_be_bytes())?;
    output.write_all(&1u32.to_be_bytes())?;
    output.write_all(&info.stsc_last_desc_id.to_be_bytes())?;
    Ok(())
}

pub fn write_modified_offsets(
    input: &mut File,
    output: &mut File,
    h: BoxHeader,
    info: &VideoInfo,
    mods: &[Modification],
    fake_payload_offset: u64,
    fake_samples: u32,
) -> io::Result<()> {
    let n = fake_samples as u32;
    let entry_size = if info.offset_is_co64 { 8u64 } else { 4u64 };
    let new_chunk_count = info
        .chunk_count
        .checked_add(n)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "chunk count overflow"))?;
    let new_size = h.size + (n as u64) * entry_size;

    write_box_size(output, &h, new_size)?;
    copy_range(input, output, h.payload_start(), 4)?;
    output.write_all(&new_chunk_count.to_be_bytes())?;

    let old_table_start = h.payload_start() + 8;
    for i in 0..info.chunk_count as u64 {
        let old_off = if info.offset_is_co64 {
            read_u64_at(input, old_table_start + i * 8)?
        } else {
            read_u32_at(input, old_table_start + i * 4)? as u64
        };
        let shifted = old_off
            .checked_add(sum_deltas_before(mods, old_off))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "chunk offset overflow"))?;

        if info.offset_is_co64 {
            output.write_all(&shifted.to_be_bytes())?;
        } else {
            if shifted > u32::MAX as u64 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "original stco offset 0x{old_off:x} becomes 0x{shifted:x}, use co64 instead"
                    ),
                ));
            }
            output.write_all(&(shifted as u32).to_be_bytes())?;
        }
    }

    for _ in 0..fake_samples {
        if info.offset_is_co64 {
            output.write_all(&fake_payload_offset.to_be_bytes())?;
        } else {
            if fake_payload_offset > u32::MAX as u64 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "fake payload offset exceeds stco 32-bit range; input needs co64",
                ));
            }
            output.write_all(&(fake_payload_offset as u32).to_be_bytes())?;
        }
    }
    Ok(())
}

pub fn write_shifted_offsets(
    input: &mut File,
    output: &mut File,
    h: BoxHeader,
    mods: &[Modification],
) -> io::Result<()> {
    copy_range(input, output, h.offset, h.header_size)?;

    let version_flags = read_u32_at(input, h.payload_start())?;
    output.write_all(&version_flags.to_be_bytes())?;

    let entry_count = read_u32_at(input, h.payload_start() + 4)?;
    output.write_all(&entry_count.to_be_bytes())?;

    let entry_size = if h.typ == FourCC(*b"co64") {
        8u64
    } else {
        4u64
    };
    let table_start = h.payload_start() + 8;

    for i in 0..entry_count as u64 {
        let old_off = if entry_size == 8 {
            read_u64_at(input, table_start + i * 8)?
        } else {
            read_u32_at(input, table_start + i * 4)? as u64
        };

        let shifted = old_off
            .checked_add(sum_deltas_before(mods, old_off))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "chunk offset overflow"))?;

        if entry_size == 8 {
            output.write_all(&shifted.to_be_bytes())?;
        } else {
            if shifted > u32::MAX as u64 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "original stco offset 0x{old_off:x} becomes 0x{shifted:x}, use co64 instead"
                    ),
                ));
            }
            output.write_all(&(shifted as u32).to_be_bytes())?;
        }
    }
    Ok(())
}

pub fn stream_transform_box(
    input: &mut File,
    output: &mut File,
    h: BoxHeader,
    mods: &[Modification],
    info: &VideoInfo,
    fake_payload_offset: u64,
    fake_samples: u32,
) -> io::Result<()> {
    let internal_delta = sum_deltas_in_range(mods, h.payload_start(), h.end());
    let is_target_stsz = info.stsz.map(|x| x.offset == h.offset).unwrap_or(false);
    let is_target_stsc = info.stsc.map(|x| x.offset == h.offset).unwrap_or(false);
    let is_target_offsets = info
        .stco
        .or(info.co64)
        .map(|x| x.offset == h.offset)
        .unwrap_or(false);
    let is_target_stts = info.stts.map(|x| x.offset == h.offset).unwrap_or(false);

    if is_target_stts {
        return write_modified_stts(input, output, h, info);
    }
    if is_target_stsz {
        return write_modified_stsz(input, output, h, info, fake_samples);
    }
    if is_target_stsc {
        return write_modified_stsc(input, output, h, info);
    }
    if is_target_offsets {
        return write_modified_offsets(
            input,
            output,
            h,
            info,
            mods,
            fake_payload_offset,
            fake_samples,
        );
    }
    if h.typ == FourCC(*b"stco") || h.typ == FourCC(*b"co64") {
        return write_shifted_offsets(input, output, h, mods);
    }

    let new_size = h.size + internal_delta;
    if new_size != h.size {
        write_box_size(output, &h, new_size)?;
    } else {
        copy_range(input, output, h.offset, h.header_size)?;
    }

    if is_container(h.typ) && h.payload_size() > 0 {
        let mut pos = h.payload_start();
        while pos < h.end() {
            let child = read_box_header(input, pos, h.end())?
                .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "missing child box"))?;
            stream_transform_box(
                input,
                output,
                child,
                mods,
                info,
                fake_payload_offset,
                fake_samples,
            )?;
            pos = child.end();
        }
    } else {
        copy_range(input, output, h.payload_start(), h.payload_size())?;
    }
    Ok(())
}

pub fn stream_top_level(
    input: &mut File,
    output: &mut File,
    file_len: u64,
    mods: &[Modification],
    info: &VideoInfo,
    fake_payload_offset: u64,
    fake_samples: u32,
) -> io::Result<()> {
    let mut pos = 0u64;
    while pos < file_len {
        let h = read_box_header(input, pos, file_len)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "missing top-level box"))?;
        stream_transform_box(
            input,
            output,
            h,
            mods,
            info,
            fake_payload_offset,
            fake_samples,
        )?;
        pos = h.end();
    }
    Ok(())
}
