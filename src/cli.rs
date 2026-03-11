use clap::Parser;

use crate::config::Config;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(
    name = "somedl",
    version = VERSION,
    disable_version_flag = true,
    about = "Download songs from YouTube with metadata from MusicBrainz, Genius, and Deezer",
    long_about = r#"Download songs from YouTube by query, multiple queries, or playlist link.

 - Put all inputs in quotes, URLs as well: somedl "Artist - song"
 - Separate multiple inputs with spaces: somedl "Artist - song" "https://music.youtube..."
 - Different types of URLs and queries can be mixed.
 - Accepted URLs: YT-Music, YT, shortened youtu.be, YT playlists. Always include https://
 - For advanced configuration, run: somedl --generate-config"#
)]
pub struct Cli {
    /// Song queries (e.g., 'Artist - Song'), YouTube URLs or playlist URLs
    pub inputs: Vec<String>,

    /// Print version
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub version: bool,

    /// Generate a config file
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub generate_config: bool,

    /// Download format
    #[arg(
        short = 'f',
        long,
        value_name = "FORMAT",
        value_parser = ["best", "best/opus", "best/m4a", "opus", "m4a", "mp3", "vorbis", "flac"]
    )]
    pub format: Option<String>,

    /// Download to current directory, ignoring output template and dir from config
    #[arg(short = 'l', long, action = clap::ArgAction::SetTrue)]
    pub here: bool,

    /// Output directory
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<String>,

    /// Fetch metadata from YT search but download audio from the given URL
    #[arg(short = 'd', long, action = clap::ArgAction::SetTrue)]
    pub download_url_audio: bool,

    /// Verbose output (debug level)
    #[arg(short = 'v', long, action = clap::ArgAction::SetTrue)]
    pub verbose: bool,

    /// Quiet output (errors only)
    #[arg(short = 'q', long, action = clap::ArgAction::SetTrue)]
    pub quiet: bool,

    /// Always generate a download report
    #[arg(short = 'R', long, action = clap::ArgAction::SetTrue)]
    pub download_report: bool,

    /// Permanently disable download report generation
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub disable_report: bool,

    /// Skip yt-dlp download (debug only)
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub no_download: bool,

    /// Browser to pull cookies from for age-restricted content
    #[arg(long, value_name = "BROWSER")]
    pub cookies_from_browser: Option<String>,

    /// Path to cookies file for age-restricted content
    #[arg(long, value_name = "FILEPATH")]
    pub cookies: Option<String>,

    /// Skip MusicBrainz (no genre data will be added)
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub no_musicbrainz: bool,
}

/// Apply CLI overrides to config, return the final inputs list.
/// Returns None if no inputs were provided and nothing else to do.
pub fn apply_cli(cli: &Cli, config: &mut Config) -> Option<Vec<String>> {
    if cli.verbose {
        config.logging.level = "DEBUG".to_string();
    } else if cli.quiet {
        config.logging.level = "ERROR".to_string();
    }

    if cli.no_download {
        config.download.disable_download = true;
    }
    if cli.download_url_audio {
        config.download.strict_url_download = true;
    }
    if let Some(cookies) = &cli.cookies {
        config.download.cookies_path = cookies.clone();
    } else if let Some(browser) = &cli.cookies_from_browser {
        config.download.cookies_from_browser = browser.clone();
    }
    if cli.no_musicbrainz {
        config.api.musicbrainz = false;
    }
    if cli.download_report {
        config.logging.download_report = 1;
    }
    if cli.disable_report {
        config.logging.download_report = 1_000_000;
        let mut saved = config.clone();
        saved.logging.download_report = 1_000_000;
        let _ = crate::config::save_config(&saved);
        println!("Download reports have been disabled.");
    }
    if cli.here {
        config.download.output = "{artist} - {song}".to_string();
        config.download.output_dir = ".".to_string();
    }
    if let Some(fmt) = &cli.format {
        config.download.format = fmt.clone();
    }
    if let Some(out) = &cli.output {
        config.download.output_dir = out.clone();
    }

    if cli.inputs.is_empty() {
        None
    } else {
        Some(cli.inputs.clone())
    }
}
