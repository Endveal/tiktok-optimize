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

mod core;
mod interface;

const FFMPEG_PATH: &str = "ffmpeg";
const TEMP_FILE_PREFIX: &str = "endveal_";

fn main() {
    let Some(args) = core::args_parser::parse_args().unwrap_or_else(|err| {
        eprint!("[ERROR] {}", err);
        core::utils::exit(0);
    }) else {
        core::utils::exit(0);
    };

    if core::utils::preflight_ffmpeg().is_err() {
        core::utils::exit(0);
    }

    if args.is_cmd {
        interface::cli::run_cli(args, TEMP_FILE_PREFIX, FFMPEG_PATH);
    } else {
        println!("[INFO] Mode API Akan Segera Datang...");
        println!("[INFO] Tunggu aja commit barunya :)");
    }
}
