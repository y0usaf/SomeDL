use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

use crate::config::Config;
use crate::utils::sanitize::generate_output_name;

/// Download audio for a YouTube video ID using yt-dlp.
/// Returns the path to the downloaded (and post-processed) file.
pub fn download_song(
    config: &Config,
    video_id: &str,
    artist: &str,
    song: &str,
    album: &str,
    date: &str,
    track_pos: Option<u32>,
    track_count: Option<u32>,
) -> Option<PathBuf> {
    if config.download.disable_download {
        log::warn!("Skipping download (--no-download is set)");
        return None;
    }

    let url = format!("https://music.youtube.com/watch?v={video_id}");
    let output_base = generate_output_name(config, artist, song, album, date, track_pos, track_count);
    let output_template = format!("{}.%(ext)s", output_base.display());

    let ext = &config.download.format;
    let quality = config.download.quality.to_string();

    let (yt_format, audio_format, needs_quality) = match ext.as_str() {
        "best" => ("bestaudio/best", "best", false),
        "best/opus" => ("bestaudio[ext=opus]/bestaudio/best", "best", false),
        "best/m4a" => ("bestaudio[ext=m4a]/bestaudio/best", "best", false),
        "opus" => ("bestaudio[ext=opus]/bestaudio/best", "opus", false),
        "m4a" => ("bestaudio[ext=m4a]/bestaudio/best", "m4a", false),
        "mp3" => ("bestaudio[ext=m4a]/bestaudio/best", "mp3", true),
        "vorbis" => ("bestaudio/best", "vorbis", true),
        "flac" => ("bestaudio/best", "flac", false),
        _ => ("bestaudio[ext=m4a]/bestaudio/best", "mp3", true),
    };

    let quiet = !log::log_enabled!(log::Level::Debug);

    let mut args: Vec<String> = vec![
        "--format".to_string(),
        yt_format.to_string(),
        "--extract-audio".to_string(),
        "--audio-format".to_string(),
        audio_format.to_string(),
        "--output".to_string(),
        output_template.clone(),
        "--no-playlist".to_string(),
        "--print".to_string(),
        "after_move:filepath".to_string(),
    ];

    if quiet {
        args.push("--quiet".to_string());
        args.push("--no-warnings".to_string());
    }

    if needs_quality {
        args.push("--audio-quality".to_string());
        args.push(quality);
    }

    if !config.download.cookies_from_browser.is_empty() {
        args.push("--cookies-from-browser".to_string());
        args.push(config.download.cookies_from_browser.clone());
    } else if !config.download.cookies_path.is_empty() {
        args.push("--cookies".to_string());
        args.push(config.download.cookies_path.clone());
    }

    args.push(url);

    log::info!("Running yt-dlp...");
    log::debug!("yt-dlp args: {args:?}");

    let output = Command::new("yt-dlp")
        .args(&args)
        .output()
        .context("Failed to run yt-dlp. Is it installed?");

    match output {
        Err(e) => {
            log::error!("{e}");
            None
        }
        Ok(out) => {
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                log::error!("yt-dlp failed: {stderr}");
                return None;
            }

            // yt-dlp prints the final filepath via --print after_move:filepath
            let stdout = String::from_utf8_lossy(&out.stdout);
            let filepath = stdout.lines().last().unwrap_or("").trim().to_string();

            if !filepath.is_empty() && std::path::Path::new(&filepath).exists() {
                log::debug!("Downloaded to: {filepath}");
                return Some(PathBuf::from(filepath));
            }

            // Fallback: glob for the output file
            let pattern = format!("{}.*", output_base.display());
            if let Ok(mut matches) = glob::glob(&pattern) {
                if let Some(Ok(p)) = matches.next() {
                    log::debug!("Found downloaded file via glob: {}", p.display());
                    return Some(p);
                }
            }

            log::error!("Could not determine downloaded file path");
            None
        }
    }
}
