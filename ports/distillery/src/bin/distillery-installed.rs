// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Installed Distillery configuration and Personae bootstrap boundary.
//!
//! This binary intentionally stops before starting a resident. The remaining
//! construction inputs are a mesh-owned store/retention policy and a
//! device-owned `HostConfig`/`ResidentSettings`; accepting defaults here would
//! make Distillery the unchosen scheduler and device-policy authority.

use std::path::PathBuf;

use distillery::InstalledAuthority;
use personae::ProfileId;

fn main() {
    if let Err(error) = run(std::env::args().skip(1).collect()) {
        eprintln!("distillery-installed: {error}");
        std::process::exit(2);
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let Some((command, rest)) = args.split_first() else {
        return Err(usage());
    };
    let options = Options::parse(rest)?;
    match command.as_str() {
        "configure" => {
            let profile = options.profile.ok_or_else(usage)?;
            let settings = InstalledAuthority::configure(&options.data_root, ProfileId(profile))
                .map_err(|error| error.to_string())?;
            println!(
                "configured Distillery at {} to use Personae profile `{}`",
                options.data_root.display(),
                settings.profile
            );
        }
        "inspect" => {
            if options.profile.is_some() {
                return Err("--profile only belongs to configure".into());
            }
            let authority = match options.vault_dir {
                Some(vault_dir) => InstalledAuthority::open_with(
                    &options.data_root,
                    &vault_dir,
                    personae::bootstrap::Unlock::from_env(),
                ),
                None => InstalledAuthority::open(&options.data_root),
            }
            .map_err(|error| error.to_string())?;
            println!(
                "Distillery profile: {}\nPersonae protection: {}\nProduct root: {}\n\
                 Resident start remains gated on caller-supplied mesh retention and device host settings.",
                authority.profile().0,
                authority.protection(),
                authority.data_root().display()
            );
        }
        _ => return Err(usage()),
    }
    Ok(())
}

struct Options {
    data_root: PathBuf,
    vault_dir: Option<PathBuf>,
    profile: Option<String>,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut data_root = None;
        let mut vault_dir = None;
        let mut profile = None;
        let mut values = args.iter();
        while let Some(option) = values.next() {
            match option.as_str() {
                "--data-root" => data_root = Some(PathBuf::from(next(&mut values, option)?)),
                "--vault-dir" => vault_dir = Some(PathBuf::from(next(&mut values, option)?)),
                "--profile" => profile = Some(next(&mut values, option)?.to_string()),
                _ => return Err(usage()),
            }
        }
        Ok(Self {
            data_root: data_root.ok_or_else(usage)?,
            vault_dir,
            profile,
        })
    }
}

fn next<'a>(
    values: &mut impl Iterator<Item = &'a String>,
    option: &str,
) -> Result<&'a str, String> {
    values
        .next()
        .map(String::as_str)
        .ok_or_else(|| format!("{option} needs a value"))
}

fn usage() -> String {
    "usage:\n  distillery-installed configure --data-root <path> --profile <personae-profile>\n  distillery-installed inspect --data-root <path> [--vault-dir <path>]".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configure_requires_an_explicit_data_root_and_profile() {
        assert!(Options::parse(&[]).is_err());
        assert!(Options::parse(&["--data-root".into(), "x".into()]).is_ok());
    }
}
