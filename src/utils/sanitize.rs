use regex::Regex;
use std::path::{Path, PathBuf};
use glob::glob;

use crate::config::Config;

pub fn sanitize(filename: &str) -> String {
    let re = Regex::new(r#"[<>:"/\\|?*\x00-\x1F]"#).unwrap();
    re.replace_all(filename, "_").to_string()
}

pub fn generate_output_name(
    config: &Config,
    artist: &str,
    song: &str,
    album: &str,
    date: &str,
    track_pos: Option<u32>,
    track_count: Option<u32>,
) -> PathBuf {
    let base = Path::new(&config.download.output_dir);
    let template = &config.download.output;

    let filled = template
        .replace("{artist}", &sanitize(artist))
        .replace("{song}", &sanitize(song))
        .replace("{album}", &sanitize(album))
        .replace("{year}", date)
        .replace("{track_pos}", &track_pos.map(|n| n.to_string()).unwrap_or_default())
        .replace("{track_count}", &track_count.map(|n| n.to_string()).unwrap_or_default());

    let path = base.join(filled);

    // Create parent directories if needed
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    path
}

pub fn file_already_exists(config: &Config, artist: &str, song: &str) -> bool {
    let base = Path::new(&config.download.output_dir);

    // Replace optional placeholders with glob wildcards
    let template = config.download.output.clone() + ".*";
    let re_optional = Regex::new(r"\{(year|album|track_pos|track_count)\}").unwrap();
    let glob_template = re_optional.replace_all(&template, "*");

    let filled = glob_template
        .replace("{artist}", &sanitize(artist))
        .replace("{song}", &sanitize(song));

    let pattern = base.join(&*filled);
    let pattern_str = pattern.to_string_lossy();

    glob(&pattern_str)
        .map(|mut g| g.next().is_some())
        .unwrap_or(false)
}
