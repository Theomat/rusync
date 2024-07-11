use colored::*;
use home::home_dir;
use indicatif::ProgressBar;
use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

pub(crate) struct Sync {
    pub(crate) name: String,
    pub(crate) locals: Vec<String>,
    pub(crate) remotes: Vec<(String, String)>,
}

const SYNC_FILE: &str = ".rusync";
const SEPARATOR: &str = "$RUSEP$";
const FILES_SEP: &str = "$FILES$";

fn paths_match(root: &String, sub_file: &String) -> bool {
    sub_file.starts_with(root)
}

fn remote_to_path(dist_path: &(String, String)) -> String {
    let mut out = String::new();
    out += &dist_path.0;
    out += ":";
    out += &dist_path.1;
    out
}

pub(crate) enum MatchingResult<'a> {
    Remote(Vec<String>),
    Local(Vec<&'a String>),
}

impl Sync {
    pub fn new(name: String) -> Sync {
        return Sync {
            name,
            locals: Vec::new(),
            remotes: Vec::new(),
        };
    }

    fn get_path() -> Option<PathBuf> {
        match home_dir() {
            Some(path) => Some(path.join(SYNC_FILE)),
            None => None,
        }
    }

    pub fn save_all(syncs: &Vec<Sync>) {
        match Self::get_path() {
            Some(path) => {
                let display = path.display();
                let mut file = match File::create(&path) {
                    Err(why) => panic!("couldn't open {}: {}", display, why),
                    Ok(file) => file,
                };
                let mut s = String::new();
                for sync in syncs {
                    s += SEPARATOR;
                    s += &sync.name;
                    s += FILES_SEP;
                    s += &sync.locals.join("\n");
                    if !sync.remotes.is_empty() {
                        s += "\n";
                        s += &sync
                            .remotes
                            .iter()
                            .map(remote_to_path)
                            .collect::<Vec<String>>()
                            .join("\n");
                    }
                }
                match &file.write_all(s.as_bytes()) {
                    Err(e) => panic!("couldn't save sync: {}", e),
                    Ok(_) => {}
                }
            }
            None => panic!("couldn't get home path"),
        }
    }

    pub fn load_all() -> Vec<Sync> {
        match home_dir() {
            Some(path) => {
                let path = path.join(SYNC_FILE);
                let display = path.display();
                let mut file = match File::open(&path) {
                    Err(_) => {
                        Self::save_all(&Vec::new());
                        File::open(&path).unwrap()
                    }
                    Ok(file) => file,
                };
                let mut s = String::new();
                match file.read_to_string(&mut s) {
                    Err(why) => panic!("couldn't read {}: {}", display, why),
                    Ok(_) => {
                        let mut out = Vec::new();
                        for el in s.split(SEPARATOR) {
                            match el.split_once(FILES_SEP) {
                                Some((name, files)) => {
                                    let mut new_sync = Sync::new(name.to_string());
                                    if files.contains("\n") && !files.is_empty() {
                                        new_sync.add_files(
                                            &files.split("\n").map(|x| x.to_string()).collect(),
                                        );
                                    }
                                    out.push(new_sync);
                                }
                                _ => {}
                            }
                        }
                        out
                    }
                }
            }
            None => Vec::new(),
        }
    }

    pub fn add_files(&mut self, files: &Vec<String>) -> Vec<bool> {
        let mut out = Vec::new();
        for file in files {
            match file.split_once(":") {
                Some(dist_path) => {
                    self.remotes
                        .push((dist_path.0.to_string(), dist_path.1.to_string()));
                    out.push(true)
                }
                None => {
                    let base = Path::new(file);
                    let true_path = base.canonicalize().unwrap_or(base.to_path_buf());
                    let a = true_path.to_str().unwrap();
                    self.locals.push(a.to_owned());
                    out.push(false)
                }
            }
        }
        out
    }

    pub fn remove_files(&mut self, files: &Vec<String>) -> Vec<(bool, bool)> {
        let mut out = Vec::new();
        for file in files {
            match file.split_once(":") {
                Some(dist_path) => {
                    let conv = (dist_path.0.to_string(), dist_path.1.to_string());
                    if self.remotes.contains(&conv) {
                        self.remotes.retain(|y| y != &conv);
                        out.push((true, true));
                    } else {
                        out.push((false, true));
                    }
                }
                None => {
                    if self.locals.contains(file) {
                        self.locals.retain(|y| y != file);
                        out.push((true, false));
                    } else {
                        out.push((false, false));
                    }
                }
            }
        }
        out
    }

    pub(crate) fn name_matches(&self, name: &String) -> bool {
        self.name.starts_with(name)
    }

    pub(crate) fn has_file_inside(&self, path: &str) -> bool {
        match path.split_once(":") {
            Some((qhost, qpath)) => self.remotes.iter().any(|(host, path)| {
                host == &qhost.to_string() && paths_match(&qpath.to_string(), path)
            }),
            None => self
                .locals
                .iter()
                .any(|file| paths_match(&path.to_string(), file)),
        }
    }

    pub(crate) fn matching_files(&self, path: &str) -> MatchingResult {
        match path.split_once(":") {
            Some((qhost, qpath)) => MatchingResult::Remote(
                self.remotes
                    .iter()
                    .filter(|(host, path)| {
                        host == &qhost.to_string() && paths_match(&qpath.to_string(), path)
                    })
                    .map(remote_to_path)
                    .collect(),
            ),
            None => MatchingResult::Local(
                self.locals
                    .iter()
                    .filter(|file| paths_match(&path.to_string(), file))
                    .collect(),
            ),
        }
    }

    pub fn sync(&self) {
        let bar = ProgressBar::new_spinner();
        bar.enable_steady_tick(Duration::from_millis(100));
        let dir = env::temp_dir();
        let dst = dir.join("file");

        let mut latest = 0;

        let mut targets = Vec::new();
        let mut others = Vec::new();

        for path in &self.locals {
            bar.set_message(format!("{} to update. checking {}", targets.len(), path));
            let mtime = get_mtime(&dst);
            if mtime > latest {
                latest = mtime;
                targets.append(&mut others);
                others.push(path);
            } else if mtime == latest {
                others.push(path);
            } else {
                targets.push(path);
            }
        }
        let remote_paths: Vec<String> = self.remotes.iter().map(remote_to_path).collect();

        for path in &remote_paths {
            bar.set_message(format!("{} to update. checking {}", targets.len(), path));

            match scp(path.to_string(), dst.to_str().expect("msg").to_string()) {
                Some(0) => {
                    let mtime = get_mtime(&dst);
                    if mtime > latest {
                        latest = mtime;
                        targets.append(&mut others);
                        others.push(&path);
                    } else if mtime == latest {
                        others.push(&path);
                    } else {
                        targets.push(&path);
                    }
                }
                _ => {}
            }
        }
        let source = others.first().unwrap();
        for target in &targets {
            bar.set_message(format!("updating {} ", target));
            scp(source.to_string(), target.to_string());
        }
        bar.finish_and_clear();
        if !targets.is_empty() {
            println!(
                "{} updated {} file{}",
                self.name.bright_green(),
                targets.len(),
                if targets.len() == 1 { "" } else { "s" }
            );
        }
    }
}

fn get_mtime(path: &PathBuf) -> i64 {
    match File::open(path) {
        Ok(f) => match File::metadata(&f) {
            Ok(x) => x.mtime(),
            _ => 0,
        },
        _ => 0,
    }
}

fn scp(src: String, dst: String) -> Option<i32> {
    Command::new("scp")
        .arg("-p")
        .arg(src)
        .arg(dst)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("failed to run scp")
        .code()
}
