mod api;
mod cli;
mod config;
mod core;
mod utils;

use std::time::Instant;

use clap::Parser;
use reqwest::blocking::Client;

use crate::api::{deezer, genius, musicbrainz, ytmusic};
use crate::cli::{apply_cli, Cli};
use crate::config::{generate_config, load_config, Config};
use crate::core::{
    downloader::download_song,
    input_parser::{generate_song_list, SongItem},
    metadata::{write_metadata, TrackMetadata},
    report::DownloadReport,
};
use crate::utils::sanitize::file_already_exists;

fn main() {
    let cli = Cli::parse();

    // Print version if requested with no inputs
    if cli.version && cli.inputs.is_empty() {
        println!("somedl {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if cli.generate_config {
        if let Err(e) = generate_config() {
            eprintln!("Failed to generate config: {e}");
        }
        if cli.inputs.is_empty() {
            return;
        }
    }

    let mut config = load_config();

    let inputs = match apply_cli(&cli, &mut config) {
        Some(i) => i,
        None => {
            if !cli.version && !cli.generate_config && !cli.disable_report {
                eprintln!("No inputs provided. Run `somedl --help` for usage.");
            }
            return;
        }
    };

    // Init logger
    let log_level = match config.logging.level.to_uppercase().as_str() {
        "DEBUG" => log::LevelFilter::Debug,
        "WARNING" | "WARN" => log::LevelFilter::Warn,
        "ERROR" => log::LevelFilter::Error,
        _ => log::LevelFilter::Info,
    };
    env_logger::Builder::new()
        .filter_level(log_level)
        .format_timestamp(None)
        .format_target(false)
        .init();

    let client = ytmusic::make_client();
    let timer = Instant::now();

    log::debug!("Inputs: {inputs:?}");

    let songs = generate_song_list(&client, &inputs);
    if songs.is_empty() {
        eprintln!("No songs to process.");
        return;
    }

    let total = songs.len();
    let mut report = DownloadReport {
        succeeded: Vec::new(),
        failed: Vec::new(),
    };

    for (i, song) in songs.iter().enumerate() {
        println!("{}", "-".repeat(88));
        println!("Downloading song: {}/{total}", i + 1);
        println!();

        let label = song
            .prefetched
            .as_ref()
            .map(|p| format!("{} - {}", p.artist_name, p.song_title))
            .or_else(|| song.text_query.clone())
            .unwrap_or_else(|| "unknown".to_string());

        match process_song(&client, &config, song) {
            Some(meta) => {
                report
                    .succeeded
                    .push(format!("{} - {}", meta.artist, meta.title));
            }
            None => {
                log::warn!("Failed to download: {label}");
                report.failed.push(label);
            }
        }

        println!();
    }

    if total >= config.logging.download_report {
        report.print();
    }

    let elapsed = timer.elapsed();
    if elapsed.as_secs() < 60 {
        println!("TIME - The whole process took {:.1} seconds.", elapsed.as_secs_f64());
    } else {
        let mins = elapsed.as_secs() / 60;
        let secs = elapsed.as_secs() % 60;
        println!("TIME - The whole process took {mins} minutes and {secs} seconds.");
    }
}

fn process_song(client: &Client, config: &Config, song: &SongItem) -> Option<TrackMetadata> {
    let song_timer = Instant::now();

    // ── Step 1: resolve metadata ────────────────────────────────────────────
    let (video_id, artist_name, song_title, album_name, album_id, video_type, lyrics_browse_id, original_url_id) =
        resolve_song_info(client, config, song)?;

    // ── Step 2: duplicate check ─────────────────────────────────────────────
    if file_already_exists(config, &artist_name, &song_title) {
        log::warn!("Song already exists, skipping: {artist_name} - {song_title}");
        return None;
    }

    // ── Step 3: fetch album info from YouTube Music ─────────────────────────
    let album = match ytmusic::get_album(client, &album_id) {
        Ok(a) => a,
        Err(e) => {
            log::error!("Failed to fetch album {album_id}: {e}");
            return None;
        }
    };

    // Genius album check: if YouTube says "Single"/"EP", ask Genius for the real album
    let (final_album_name, final_album_id, final_album) =
        maybe_correct_album(client, config, &artist_name, &song_title, album_name, album_id, album);

    // Find track position
    let (track_pos, track_count) = find_track_position(&final_album, &song_title);

    let thumbnails = final_album.thumbnails.clone();
    let date = final_album.year.clone();

    // ── Step 4: lyrics ──────────────────────────────────────────────────────
    let lyrics = fetch_lyrics(client, config, &video_id, lyrics_browse_id);

    // ── Step 5: MusicBrainz genre ──────────────────────────────────────────
    let (mb_genre, mb_artist_id) = fetch_musicbrainz(client, config, &artist_name, &song_title);

    // ── Step 6: Deezer ISRC + label ───────────────────────────────────────
    let deezer = fetch_deezer(client, config, &artist_name, &final_album_name, &song_title);

    let isrc = deezer.as_ref().map(|d| d.isrc.clone()).filter(|s| !s.is_empty());
    let copyright = deezer
        .as_ref()
        .and_then(|d| d.label.as_ref())
        .filter(|_| !date.is_empty())
        .map(|label| format!("{date} {label}"));

    let mut meta = TrackMetadata {
        title: song_title.clone(),
        artist: artist_name.clone(),
        album: final_album_name.clone(),
        year: date.clone(),
        genre: mb_genre.unwrap_or_default(),
        track_pos,
        track_count,
        lyrics,
        isrc,
        copyright,
        musicbrainz_artist_id: mb_artist_id,
        source_url: format!("https://music.youtube.com/watch?v={video_id}"),
        album_art_url: thumbnails.last().map(|t| t.url.clone()),
        thumbnails,
    };

    // ── Step 7: download audio ─────────────────────────────────────────────
    let download_id = if config.download.strict_url_download {
        original_url_id.as_deref().unwrap_or(&video_id)
    } else {
        &video_id
    };

    let file_path = download_song(
        config,
        download_id,
        &artist_name,
        &song_title,
        &final_album_name,
        &date,
        track_pos,
        track_count,
    )?;

    // ── Step 8: download album art ─────────────────────────────────────────
    let art_bytes = meta
        .best_thumbnail_url()
        .and_then(|url| {
            log::debug!("Downloading album art...");
            ytmusic::download_album_art(client, url)
        });

    // ── Step 9: write metadata ─────────────────────────────────────────────
    if let Err(e) = write_metadata(config, &meta, &file_path, art_bytes.as_deref()) {
        log::error!("Failed to write metadata: {e:#}");
    }

    let elapsed = song_timer.elapsed();
    println!("TIME - Song download took {:.1} seconds.", elapsed.as_secs_f64());

    Some(meta)
}

/// Resolve the canonical video_id, artist, title, album_id for a SongItem.
fn resolve_song_info(
    client: &Client,
    config: &Config,
    song: &SongItem,
) -> Option<(String, String, String, String, String, String, Option<String>, Option<String>)> {
    // If we have a URL with pre-fetched good album info, use it directly
    if let Some(pre) = &song.prefetched {
        if let (Some(album_name), Some(album_id)) = (&pre.album_name, &pre.album_id) {
            if !config.download.always_search_by_query
                && pre.video_type == "MUSIC_VIDEO_TYPE_ATV"
            {
                let vid = song.video_id.as_deref().unwrap_or("").to_string();
                log::info!("Using pre-fetched metadata for {}", pre.song_title);
                return Some((
                    vid.clone(),
                    pre.artist_name.clone(),
                    pre.song_title.clone(),
                    album_name.clone(),
                    album_id.clone(),
                    pre.video_type.clone(),
                    pre.lyrics_browse_id.clone(),
                    song.original_url_id.clone(),
                ));
            }
        }
    }

    // Text query or fallback search
    let query = song.text_query.clone().or_else(|| {
        song.prefetched.as_ref().map(|p| {
            if !p.artist_name.is_empty() && !p.song_title.is_empty() {
                format!("{} - {}", p.artist_name, p.song_title)
            } else {
                p.song_title.clone()
            }
        })
    })?;

    log::info!("Searching by query: {query}");
    let results = ytmusic::search(client, &query).ok()?;

    for (i, r) in results.iter().enumerate().take(3) {
        let artist = r.artists.first().map(|a| a.name.as_str()).unwrap_or("-");
        log::debug!("YT result {i}: {artist} - {} | {:?}", r.title, r.album.name);
    }

    let first = results.into_iter().next()?;
    let album_id = first.album.id.clone()?;

    // Fetch watch playlist for lyrics browse ID
    let lyrics_browse_id = ytmusic::get_watch_playlist(client, &first.video_id)
        .ok()
        .and_then(|w| w.lyrics_browse_id);

    Some((
        first.video_id.clone(),
        first.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
        first.title.clone(),
        first.album.name.unwrap_or_default(),
        album_id,
        first.video_type.clone(),
        lyrics_browse_id,
        song.original_url_id.clone(),
    ))
}

fn maybe_correct_album(
    client: &Client,
    config: &Config,
    artist: &str,
    song_title: &str,
    mut album_name: String,
    mut album_id: String,
    mut album: ytmusic::Album,
) -> (String, String, ytmusic::Album) {
    if album.album_type != "Single" && album.album_type != "EP" {
        return (album_name, album_id, album);
    }

    if !config.api.genius_album_check || !config.api.genius {
        return (album_name, album_id, album);
    }

    log::debug!("Song listed as single/EP, consulting Genius for album name...");
    let genius_album = genius::get_album_by_song(
        client,
        artist,
        song_title,
        config.api.genius_use_official,
        &config.api.genius_token,
    );

    if let Some(guessed_name) = genius_album {
        log::debug!("Genius album guess: '{guessed_name}'");
        // Search YTM for this album
        let search_query = format!("{guessed_name} {artist}");
        if let Ok(results) = ytmusic::search(client, &search_query) {
            // For album search we'd normally use a different filter, but songs
            // results can still carry album info; here we try to match by name
            if let Some(r) = results.into_iter().find(|r| {
                r.artists
                    .first()
                    .map(|a| a.name.to_lowercase() == artist.to_lowercase())
                    .unwrap_or(false)
            }) {
                if let (Some(new_name), Some(new_id)) = (r.album.name, r.album.id) {
                    if new_name != album_name {
                        log::debug!("Correcting album: '{album_name}' -> '{new_name}'");
                        if let Ok(new_album) = ytmusic::get_album(client, &new_id) {
                            // Only use if the track is actually in this album
                            if new_album.tracks.iter().any(|t| t.title == song_title) {
                                album_name = new_name;
                                album_id = new_id;
                                album = new_album;
                            }
                        }
                    }
                }
            }
        }
    }

    (album_name, album_id, album)
}

fn find_track_position(album: &ytmusic::Album, song_title: &str) -> (Option<u32>, Option<u32>) {
    let track_count = if album.track_count > 0 {
        Some(album.track_count)
    } else {
        None
    };

    for (i, track) in album.tracks.iter().enumerate() {
        if track.title == song_title {
            let pos = track.track_number.unwrap_or((i + 1) as u32);
            return (Some(pos), track_count);
        }
    }

    // Not found in track list
    (None, track_count)
}

fn fetch_lyrics(
    client: &Client,
    config: &Config,
    video_id: &str,
    browse_id: Option<String>,
) -> Option<String> {
    if !config.metadata.lyrics {
        return None;
    }

    let lyrics_id = browse_id.or_else(|| {
        ytmusic::get_watch_playlist(client, video_id)
            .ok()
            .and_then(|w| w.lyrics_browse_id)
    })?;

    match ytmusic::get_lyrics(client, &lyrics_id) {
        Ok(l) if !l.lyrics.is_empty() => {
            log::info!("Got lyrics from YT API");
            Some(l.lyrics)
        }
        Ok(_) => {
            log::warn!("No lyrics available from YT API");
            None
        }
        Err(e) => {
            log::warn!("Failed to fetch lyrics: {e}");
            None
        }
    }
}

fn fetch_musicbrainz(
    client: &Client,
    config: &Config,
    artist: &str,
    song: &str,
) -> (Option<String>, Option<String>) {
    if !config.api.musicbrainz || !config.metadata.genre {
        return (None, None);
    }

    log::info!("Fetching MusicBrainz data...");
    let mb_resp = musicbrainz::get_song_by_name(client, artist, Some(song), config.api.max_retry);

    let (mb_id, _mb_name) = match mb_resp
        .as_ref()
        .and_then(|r| musicbrainz::extract_artist_info(r))
    {
        Some(info) => info,
        None => {
            log::warn!("MusicBrainz: no artist info found for {artist}");
            return (None, None);
        }
    };

    // Rate limit: MusicBrainz allows ~1 req/sec
    std::thread::sleep(std::time::Duration::from_secs(1));

    let artist_resp =
        musicbrainz::get_artist_by_mbid(client, &mb_id, config.api.max_retry);

    let genre = artist_resp
        .as_ref()
        .and_then(|r| musicbrainz::extract_top_genre(r));

    if let Some(ref g) = genre {
        log::debug!("Genre from MusicBrainz: {g}");
    } else {
        log::warn!("No genre found on MusicBrainz for this artist");
    }

    (genre, Some(mb_id))
}

fn fetch_deezer(
    client: &Client,
    config: &Config,
    artist: &str,
    album: &str,
    song: &str,
) -> Option<deezer::DeezerData> {
    if !config.api.deezer || (!config.metadata.isrc && !config.metadata.copyright) {
        return None;
    }

    log::info!("Fetching Deezer data...");
    match deezer::get_album_data(client, artist, album, song) {
        Some(d) => Some(d),
        None => {
            log::warn!("Deezer returned no results for {artist} - {song}");
            None
        }
    }
}
