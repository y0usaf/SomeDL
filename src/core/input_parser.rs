use reqwest::blocking::Client;
use url::Url;

use crate::api::ytmusic::{self, SearchResult};

/// A resolved item ready for metadata fetching + download.
#[derive(Debug, Clone)]
pub struct SongItem {
    /// The YT video ID to download
    pub video_id: Option<String>,
    /// The video ID from the original URL (for strict_url_download)
    pub original_url_id: Option<String>,
    /// Text query to search with
    pub text_query: Option<String>,
    /// Pre-fetched info (from playlist/URL parse)
    pub prefetched: Option<PrefetchedInfo>,
    pub original_type: String,
}

#[derive(Debug, Clone)]
pub struct PrefetchedInfo {
    pub song_title: String,
    pub artist_name: String,
    pub artist_id: Option<String>,
    pub artist_all_names: Vec<String>,
    pub album_name: Option<String>,
    pub album_id: Option<String>,
    pub video_type: String,
    pub yt_url: String,
    pub lyrics_browse_id: Option<String>,
}

enum InputKind {
    Playlist(String),   // playlist_id
    Url(String),        // video_id
    Query(String),      // raw query string
}

fn classify(input: &str) -> InputKind {
    if let Ok(parsed) = Url::parse(input) {
        if parsed.scheme() == "https" || parsed.scheme() == "http" {
            // Check for playlist
            let list = parsed
                .query_pairs()
                .find(|(k, _)| k == "list")
                .map(|(_, v)| v.to_string());

            if let Some(playlist_id) = list {
                return InputKind::Playlist(playlist_id);
            }

            // Regular video URL: ?v=...
            let v = parsed
                .query_pairs()
                .find(|(k, _)| k == "v")
                .map(|(_, v)| v.to_string());

            if let Some(video_id) = v {
                return InputKind::Url(video_id);
            }

            // youtu.be/VIDEO_ID
            if parsed.host_str() == Some("youtu.be") {
                let video_id = parsed
                    .path_segments()
                    .and_then(|mut s| s.next())
                    .unwrap_or("")
                    .to_string();
                if !video_id.is_empty() {
                    return InputKind::Url(video_id);
                }
            }
        }
    }

    InputKind::Query(input.to_string())
}

/// Parse all user inputs into a flat list of SongItems.
pub fn generate_song_list(client: &Client, inputs: &[String]) -> Vec<SongItem> {
    let mut songs = Vec::new();

    for input in inputs {
        match classify(input) {
            InputKind::Playlist(playlist_id) => {
                log::info!("Input is playlist: {input}");
                match ytmusic::get_playlist(client, &playlist_id) {
                    Ok(tracks) => {
                        for track in tracks {
                            if let Some(item) = song_item_from_search_result(track, "playlist") {
                                songs.push(item);
                            }
                        }
                    }
                    Err(e) => log::error!("Failed to load playlist {playlist_id}: {e}"),
                }
            }

            InputKind::Url(video_id) => {
                log::info!("Input is URL: {input}");
                match ytmusic::get_watch_playlist(client, &video_id) {
                    Ok(track) => {
                        songs.push(SongItem {
                            video_id: Some(track.video_id.clone()),
                            original_url_id: Some(video_id.clone()),
                            text_query: None,
                            prefetched: Some(PrefetchedInfo {
                                song_title: track.title.clone(),
                                artist_name: track
                                    .artists
                                    .first()
                                    .map(|a| a.name.clone())
                                    .unwrap_or_default(),
                                artist_id: track.artists.first().and_then(|a| a.id.clone()),
                                artist_all_names: track
                                    .artists
                                    .iter()
                                    .map(|a| a.name.clone())
                                    .collect(),
                                album_name: track.album.name.clone(),
                                album_id: track.album.id.clone(),
                                video_type: track.video_type.clone(),
                                yt_url: format!(
                                    "https://music.youtube.com/watch?v={}",
                                    track.video_id
                                ),
                                lyrics_browse_id: track.lyrics_browse_id.clone(),
                            }),
                            original_type: track.video_type,
                        });
                    }
                    Err(e) => log::error!("Failed to load URL {input}: {e}"),
                }
            }

            InputKind::Query(query) => {
                log::info!("Input is query: {query}");
                songs.push(SongItem {
                    video_id: None,
                    original_url_id: None,
                    text_query: Some(query),
                    prefetched: None,
                    original_type: "Search query".to_string(),
                });
            }
        }
    }

    songs
}

fn song_item_from_search_result(result: SearchResult, original_type: &str) -> Option<SongItem> {
    Some(SongItem {
        original_url_id: Some(result.video_id.clone()),
        video_id: Some(result.video_id.clone()),
        text_query: None,
        prefetched: Some(PrefetchedInfo {
            song_title: result.title,
            artist_name: result.artists.first().map(|a| a.name.clone()).unwrap_or_default(),
            artist_id: result.artists.first().and_then(|a| a.id.clone()),
            artist_all_names: result.artists.iter().map(|a| a.name.clone()).collect(),
            album_name: result.album.name,
            album_id: result.album.id,
            video_type: result.video_type.clone(),
            yt_url: format!("https://music.youtube.com/watch?v={}", result.video_id),
            lyrics_browse_id: None,
        }),
        original_type: original_type.to_string(),
    })
}
