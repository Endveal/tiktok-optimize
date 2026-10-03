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

use std::{env, error::Error};

pub struct CliArgs {
    pub input: String,
    pub output: String,
}

fn print_usage(program: &str) {
    println!(
        "Tiktok video optimize - 1080p60fps, CLI version\n\nUsage:\n  {} <input> <output>",
        program
    );
}

fn print_version(program: &str) {
    println!("{} v{}", program, env!("CARGO_PKG_VERSION"));
}

pub fn parse_args() -> Result<Option<CliArgs>, Box<dyn Error>> {
    let argv: Vec<String> = env::args().collect();

    if argv.len() == 1 {
        print_usage(argv.first().map(String::as_str).unwrap_or("program"));
        return Ok(None);
    }

    let program = argv
        .first()
        .cloned()
        .unwrap_or_else(|| "program".to_owned());

    let mut positional = Vec::new();

    let mut i = 1usize;

    while i < argv.len() {
        match argv[i].as_str() {
            "-h" | "--help" => {
                print_usage(&program);
                return Ok(None);
            }
            "-v" | "--version" => {
                print_version(&program);
                return Ok(None);
            }
            "--" => {
                i += 1;

                while i < argv.len() {
                    positional.push(argv[i].clone());
                    i += 1;
                }
            }
            value if value.starts_with("--") => {
                return Err(format!("unknown option: {}", value).into());
            }
            value => {
                positional.push(value.to_owned());
                i += 1;
            }
        }
    }

    if positional.len() != 2 {
        print_usage(&program);
        return Ok(None);
    }

    Ok(Some(CliArgs {
        input: positional[0].clone(),
        output: positional[1].clone(),
    }))
}
