#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! edition = "2021"
//! license = "CC0-1.0"
//!
//! [dependencies]
//! camino = "1.1.2"
//! cargo_metadata = "0.15.2"
//! clap = { version = "4.1.4", features = ["derive"] }
//! color-eyre = "0.6.2"
//! eyre = "0.6.8"
//! fs-err = "2.9.0"
//! indexmap = { version = "1.9.2", features = ["serde-1"] }
//! itertools = "0.10.5"
//! serde = { version = "1.0.152", features = ["derive"] }
//! toml = "0.7.0"
//! ```

use std::collections::HashMap;

use camino::Utf8Path;
use cargo_metadata::{CargoOpt, Dependency, DependencyKind, MetadataCommand, Package};
use clap::Parser as _;
use eyre::{ensure, eyre};
use indexmap::IndexMap;
use itertools::Itertools as _;
use serde::Deserialize;

#[allow(clippy::enum_variant_names)]
#[derive(clap::Parser)]
enum Args {
    GenSpecs(ArgsGenSpecs),
    GenCommand(ArgsGenCommand),
    GenLicenseUrls(ArgsGenLicenseUrls),
}

#[derive(clap::Parser)]
struct ArgsGenSpecs {}

#[derive(clap::Parser)]
struct ArgsGenCommand {}

#[derive(clap::Parser)]
struct ArgsGenLicenseUrls {}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    match Args::parse() {
        Args::GenSpecs(args) => gen_specs(args),
        Args::GenCommand(args) => gen_command(args),
        Args::GenLicenseUrls(args) => gen_license_urls(args),
    }
}

fn gen_specs(ArgsGenSpecs {}: ArgsGenSpecs) -> eyre::Result<()> {
    let md = MetadataCommand::new()
        .features(CargoOpt::AllFeatures)
        .exec()?;
    let root_package = &md.root_package().ok_or_else(|| eyre!("no root package"))?;

    let specs = normal_crates_io_deps(root_package)?
        .map(|Dependency { name, req, .. }| (&**name, format!("{name}@{req}")))
        .collect();

    for spec in reorder(specs, &root_package.manifest_path)? {
        println!("{spec}");
    }
    Ok(())
}

fn gen_command(ArgsGenCommand {}: ArgsGenCommand) -> eyre::Result<()> {
    let md = MetadataCommand::new()
        .features(CargoOpt::AllFeatures)
        .exec()?;
    let root_package = &md.root_package().ok_or_else(|| eyre!("no root package"))?;

    let source_args = normal_crates_io_deps(root_package)?
        .map(
            |Dependency {
                 name,
                 req,
                 features,
                 ..
             }| {
                let spec = format!("{name}@{req}");
                let mut args = spec.clone();
                if !features.is_empty() {
                    args += " --features ";
                    args += &features.iter().map(|f| format!("{spec}/{f}")).join(",")
                }
                (&**name, args)
            },
        )
        .collect();

    println!(r"cargo add \");
    let mut package_args = reorder(source_args, &root_package.manifest_path)?.peekable();
    while let Some(package_args_) = package_args.next() {
        print!("  {package_args_}");
        if package_args.peek().is_some() {
            print!(r" \");
        }
        println!();
    }

    Ok(())
}

fn gen_license_urls(ArgsGenLicenseUrls {}: ArgsGenLicenseUrls) -> eyre::Result<()> {
    let md = MetadataCommand::new()
        .features(CargoOpt::AllFeatures)
        .exec()?;
    let root_package = &md.root_package().ok_or_else(|| eyre!("no root package"))?;

    let packages = md
        .packages
        .iter()
        .map(|p| ((&*p.name, p.version.to_string()), p))
        .collect::<HashMap<_, _>>();

    let urls = normal_crates_io_deps(root_package)?
        .map(|Dependency { name, req, .. }| {
            let version = req.to_string().trim_start_matches('=').to_owned();
            let Package {
                name,
                version,
                manifest_path,
                ..
            } = packages[&(&**name, version)];
            let manifest_dir = manifest_path.parent().unwrap();

            // proconioとnalgebraだけ暫定対応
            if name == "proconio" {
                let sha1 = read_git_sha1(manifest_dir)?;
                return Ok((
                    "proconio",
                    format!("https://github.com/statiolake/proconio-rs/tree/{sha1}"),
                ));
            }
            if name == "nalgebra" {
                // clarify.tomlを参照のこと
                return Ok((
                    "nalgebra",
                    format!("https://docs.rs/crate/nalgebra/{version}/source/Cargo.toml.orig"),
                ));
            }

            let url = format!("https://docs.rs/crate/{name}/{version}/source/");
            let url = if manifest_dir.join("LICENSE").exists() {
                format!("{url}LICENSE")
            } else {
                url
            };
            Ok((&**name, url))
        })
        .collect::<eyre::Result<_>>()?;

    for url in reorder(urls, &root_package.manifest_path)? {
        println!("{url}");
    }
    return Ok(());

    fn read_git_sha1(manifest_dir: &Utf8Path) -> eyre::Result<String> {
        let path = manifest_dir.join(".cargo_vcs_info.json");
        let json = &fs_err::read_to_string(path)?;
        let CargoVcsInfo { git: Git { sha1 } } = serde_json::from_str(json)?;
        return Ok(sha1);

        #[derive(Deserialize)]
        struct CargoVcsInfo {
            git: Git,
        }

        #[derive(Deserialize)]
        struct Git {
            sha1: String,
        }
    }
}

fn normal_crates_io_deps(
    root_package: &Package,
) -> eyre::Result<impl Iterator<Item = &Dependency>> {
    root_package
        .dependencies
        .iter()
        .filter(|Dependency { source, kind, .. }| {
            source.as_deref() == Some("registry+https://github.com/rust-lang/crates.io-index")
                && *kind == DependencyKind::Normal
        })
        .map(|dep| {
            ensure!(dep.uses_default_features, "not yet suppoorted");
            ensure!(!dep.optional, "not yet suppoorted");
            ensure!(dep.target.is_none(), "not yet suppoorted");
            ensure!(dep.rename.is_none(), "not yet suppoorted");
            Ok(dep)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(IntoIterator::into_iter)
}

fn reorder<'a, V: 'a>(
    items: HashMap<&'a str, V>,
    manifest_path: &Utf8Path,
) -> eyre::Result<impl Iterator<Item = V> + 'a> {
    let Manifest { dependencies } = toml::from_str(&fs_err::read_to_string(manifest_path)?)?;

    return Ok(items
        .into_iter()
        .sorted_by_key(move |(name, _)| {
            dependencies
                .keys()
                .enumerate()
                .find(|&(_, name_)| name_ == name)
                .map(|(i, _)| i)
        })
        .map(|(_, v)| v));

    #[derive(Deserialize)]
    struct Manifest {
        dependencies: IndexMap<String, toml::Value>,
    }
}
