use clap::{Args, Parser, Subcommand};
use colored::*;

use std::env;

mod sync;

use sync::{MatchingResult, Sync};

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Lists all syncs that will be synchronized
    Ls(FolderArgs),
    /// Display information about a sync
    Show(NameArgs),
    /// Creates a new synchrnoization and name it
    New(NameArgs),
    /// Delete a synchronization, files are kept
    Del(NameArgs),
    /// Add files to an existing synchronization
    Add(NameAndFileListArgs),
    /// Remove files to an existing synchronization
    Rm(NameAndFileListArgs),
}

#[derive(Args)]
struct FileListArgs {
    files: Vec<String>,
}

#[derive(Args)]
struct NameAndFileListArgs {
    name: String,
    files: Vec<String>,
}

#[derive(Args)]
struct NameArgs {
    name: String,
}

#[derive(Args)]
struct FolderArgs {
    folder: Option<String>,
}

fn select_by_name(syncs: &Vec<Sync>, name: &String, error: bool) -> Option<String> {
    match syncs
        .iter()
        .filter(|x| x.name_matches(name))
        .collect::<Vec<&Sync>>()
        .as_slice()
    {
        [] => {
            if error {
                print!("{} found no sync by the name {}", "error:".red(),  name.bright_green());
            }
            None
        }
        [x] => Some(x.name.clone()),
        v => {
            if error {
                println!(
                    "{} name {} is ambiguous found the following sync match:",
                    "error:".red(),
                    name.bright_green()
                );
                for x in v {
                    println!("\t{}", x.name.yellow());
                }
            }
            None
        }
    }
}

fn select_by_folder<'a>(syncs: &'a Vec<Sync>, path: &String) -> Vec<&'a Sync> {
    syncs.iter().filter(|x| x.has_file_inside(path)).collect()
}

fn current_dir() -> String {
    env::current_dir()
        .unwrap()
        .as_path()
        .canonicalize()
        .expect("failed to canonicalise path")
        .to_str()
        .expect("failed to convert to string")
        .to_string()
}

fn main() {
    let cli = Cli::parse();

    let mut syncs = Sync::load_all();

    match &cli.command {
        Some(cmd) => match &cmd {
            Commands::New(args) => match select_by_name(&syncs, &args.name, false) {
                Some(name) => {
                    println!(
                        "{} a sync with the name {} already exists",
                        "error:".red(),
                        name.bright_green()
                    );
                }
                None => {
                    let new_sync = Sync::new(args.name.clone());
                    syncs.push(new_sync);
                    Sync::save_all(&syncs);
                    println!("successfully created: {}", args.name.bright_green());
                }
            },
            Commands::Del(args) => match select_by_name(&syncs, &args.name, true) {
                Some(name) => {
                    syncs.retain(|x| x.name != name);
                    Sync::save_all(&syncs);
                    println!("successfully deleted: {}", name.bright_green());
                }
                None => {}
            },
            Commands::Show(args) => match select_by_name(&syncs, &args.name, true) {
                Some(name) => {
                    println!("name: {}", name.bright_green());
                    let sync = syncs.iter().find(|x| x.name == name).unwrap();
                    println!("local files ({}):", sync.locals.len());
                    for file in &sync.locals {
                        println!("\t{}", file.bright_yellow());
                    }
                    println!("remote files ({}):", sync.remotes.len());
                    for (host, path) in &sync.remotes {
                        println!("\t{}:{}", host.bright_blue(), path.bright_blue());
                    }
                }
                None => {}
            },
            Commands::Add(args) => {
                let out = select_by_name(&syncs, &args.name, true).and_then(|name| {
                    Some(
                        syncs
                            .iter_mut()
                            .find(|x| x.name == name)
                            .unwrap()
                            .add_files(&args.files),
                    )
                });
                match out {
                    Some(remotes) => {
                        Sync::save_all(&syncs);
                        println!("successfully added files to {}:", args.name.bright_green());
                        for (file, remote) in args.files.iter().zip(remotes.iter()) {
                            println!(
                                "\t{}",
                                if *remote {
                                    file.bright_blue()
                                } else {
                                    file.bright_yellow()
                                }
                            );
                        }
                    }
                    None => {}
                }
            }
            Commands::Rm(args) => { 
                let out = select_by_name(&syncs, &args.name, true).and_then(|name| {
                    Some(
                        syncs
                            .iter_mut()
                            .find(|x| x.name == name)
                            .unwrap()
                            .remove_files(&args.files),
                    )
                });
                match out {
                    Some(deleteds) => {
                        Sync::save_all(&syncs);
                        println!(
                            "successfully removed files from {}",
                            args.name.bright_green()
                        );
                        for (file, (_, remote)) in args
                            .files
                            .iter()
                            .zip(deleteds.iter())
                            .filter(|(_, (del, _))| *del)
                        {
                            println!(
                                "\t{}",
                                if *remote {
                                    file.bright_blue()
                                } else {
                                    file.bright_yellow()
                                }
                            );
                        }
                    }
                    None => {}
                }
            }
            Commands::Ls(args) => {
                let default = current_dir();
                let path = match &args.folder {
                    Some(x) => x,
                    None => &default.to_string(),
                };
                let selected = select_by_folder(&syncs, &path);
                if selected.is_empty() {
                    println!("found no sync in {}", path.bright_blue());
                } else {
                    println!("the following syncs are in {}:", path.bright_blue());
                    for sync in selected {
                        println!("name: {}", sync.name.bright_green());
                        println!("matching files:");
                        match sync.matching_files(&path) {
                            MatchingResult::Local(l) => {
                                for ele in l {
                                    println!("\t{}", ele.bright_yellow());
                                }
                            }
                            MatchingResult::Remote(l) => {
                                for ele in l {
                                    println!("\t{}", ele.bright_blue());
                                }
                            }
                        }
                    }
                }
            }
        },
        None =>  {
           let path = current_dir();
           let selected = select_by_folder(&syncs, &path);
           for sync in &selected {
            sync.sync();
           }
        },
    };
}
