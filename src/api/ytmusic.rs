/// YouTube Music unofficial API client.
///
/// All requests are unauthenticated (public) POST requests to the YouTube
/// internal browse/search/next endpoints.  The request structure mirrors
/// what `ytmusicapi` does under the hood.
use anyhow::{Context, Result};
use chrono::Utc;
use reqwest::blocking::Client;
use serde_json::{json, Value};

const YTM_BASE: &str = "https://music.youtube.com/youtubei/v1/";
const YTM_KEY: &str = "AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30";
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:88.0) Gecko/20100101 Firefox/88.0";

// Songs filter param = param1 + param2("songs") + param3(no ignore_spelling)
// param1="EgWKAQ", param2_songs="II", param3="AWoMEA4QChADEAQQCRAF"
const SONGS_FILTER_PARAM: &str = "EgWKAQIIAWoMEA4QChADEAQQCRAF";

#[derive(Debug, Clone)]
pub struct YtArtist {
    pub name: String,
    pub id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct YtAlbumRef {
    pub name: Option<String>,
    pub id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub video_id: String,
    pub title: String,
    pub artists: Vec<YtArtist>,
    pub album: YtAlbumRef,
    pub video_type: String,
}

#[derive(Debug, Clone)]
pub struct AlbumTrack {
    pub title: String,
    pub track_number: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct AlbumThumbnail {
    pub url: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct Album {
    pub title: String,
    pub album_type: String, // "Album", "Single", "EP", etc.
    pub year: String,
    pub track_count: u32,
    pub tracks: Vec<AlbumTrack>,
    pub thumbnails: Vec<AlbumThumbnail>,
}

#[derive(Debug, Clone)]
pub struct WatchTrack {
    pub video_id: String,
    pub title: String,
    pub artists: Vec<YtArtist>,
    pub album: YtAlbumRef,
    pub video_type: String,
    pub lyrics_browse_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Lyrics {
    pub lyrics: String,
    pub has_timestamps: bool,
}

fn make_context() -> Value {
    let version = format!("1.{}.01.00", Utc::now().format("%Y%m%d"));
    json!({
        "context": {
            "client": {
                "clientName": "WEB_REMIX",
                "clientVersion": version,
                "hl": "en",
                "gl": "US",
            },
            "user": {}
        }
    })
}

fn post(client: &Client, endpoint: &str, mut body: Value) -> Result<Value> {
    let url = format!("{YTM_BASE}{endpoint}?alt=json&key={YTM_KEY}");

    // Merge context into body
    if let (Some(obj), Some(ctx)) = (body.as_object_mut(), make_context().as_object()) {
        for (k, v) in ctx {
            obj.entry(k).or_insert_with(|| v.clone());
        }
    }

    let resp = client
        .post(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "*/*")
        .header("Content-Type", "application/json")
        .header("Origin", "https://music.youtube.com")
        .json(&body)
        .send()
        .context("YouTube Music API request failed")?;

    resp.json::<Value>().context("Failed to parse YouTube Music API response")
}

/// Safe JSON pointer helper (returns None instead of panicking)
fn ptr<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    v.pointer(path)
}

fn str_val<'a>(v: &'a Value, path: &str) -> Option<&'a str> {
    ptr(v, path)?.as_str()
}

fn parse_artists_from_runs(runs: &Value) -> Vec<YtArtist> {
    let arr = match runs.as_array() {
        Some(a) => a,
        None => return vec![],
    };

    arr.iter()
        .filter_map(|run| {
            let name = run.get("text")?.as_str()?;
            // Skip separator runs
            if name.trim() == "•" || name.trim() == "&" || name.trim() == "," {
                return None;
            }
            // Only runs that have a browseEndpoint pointing to a channel are artists
            let id = run
                .pointer("/navigationEndpoint/browseEndpoint/browseId")
                .and_then(|v| v.as_str())
                .filter(|id| id.starts_with("UC") || id.starts_with("MPLA"))
                .map(|s| s.to_string());

            // Accept runs with a channel browse ID, or the very first named run
            if id.is_some() || !name.contains(" • ") {
                Some(YtArtist {
                    name: name.to_string(),
                    id,
                })
            } else {
                None
            }
        })
        .collect()
}

fn parse_album_from_runs(runs: &Value) -> YtAlbumRef {
    let arr = match runs.as_array() {
        Some(a) => a,
        None => return YtAlbumRef { name: None, id: None },
    };

    for run in arr {
        if let Some(browse_id) = run
            .pointer("/navigationEndpoint/browseEndpoint/browseId")
            .and_then(|v| v.as_str())
        {
            if browse_id.starts_with("MPRE") {
                return YtAlbumRef {
                    name: run.get("text").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    id: Some(browse_id.to_string()),
                };
            }
        }
    }
    YtAlbumRef { name: None, id: None }
}

fn get_item_text<'a>(item: &'a Value, col_index: usize) -> Option<&'a str> {
    item.pointer(&format!(
        "/flexColumns/{col_index}/musicResponsiveListItemFlexColumnRenderer/text/runs/0/text"
    ))
    .and_then(|v| v.as_str())
}

fn get_item_runs<'a>(item: &'a Value, col_index: usize) -> Option<&'a Value> {
    item.pointer(&format!(
        "/flexColumns/{col_index}/musicResponsiveListItemFlexColumnRenderer/text/runs"
    ))
}

/// Search YouTube Music for songs matching `query`.
/// Returns up to `limit` results filtered to songs.
pub fn search(client: &Client, query: &str) -> Result<Vec<SearchResult>> {
    let body = json!({
        "query": query,
        "params": SONGS_FILTER_PARAM,
    });

    let resp = post(client, "search", body)?;

    // Navigate: contents > tabbedSearchResultsRenderer > tabs[0] > tabRenderer > content
    //         > sectionListRenderer > contents[0] > musicShelfRenderer > contents
    let shelf_contents = resp
        .pointer("/contents/tabbedSearchResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents")
        .and_then(|c| c.as_array())
        .and_then(|arr| {
            // Find the musicShelfRenderer
            arr.iter().find_map(|item| {
                item.get("musicShelfRenderer")
                    .and_then(|shelf| shelf.get("contents"))
                    .and_then(|c| c.as_array())
            })
        });

    let items = match shelf_contents {
        Some(items) => items,
        None => return Ok(vec![]),
    };

    let mut results = Vec::new();

    for item in items {
        let data = match item.get("musicResponsiveListItemRenderer") {
            Some(d) => d,
            None => continue,
        };

        let title = match get_item_text(data, 0) {
            Some(t) => t.to_string(),
            None => continue,
        };

        // video_id from overlay play button or first column run
        let video_id = data
            .pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/videoId")
            .or_else(|| data.pointer("/flexColumns/0/musicResponsiveListItemFlexColumnRenderer/text/runs/0/navigationEndpoint/watchEndpoint/videoId"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if video_id.is_empty() {
            continue;
        }

        let video_type = data
            .pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/watchEndpointMusicSupportedConfigs/watchEndpointMusicConfig/musicVideoType")
            .or_else(|| data.pointer("/flexColumns/0/musicResponsiveListItemFlexColumnRenderer/text/runs/0/navigationEndpoint/watchEndpoint/watchEndpointMusicSupportedConfigs/watchEndpointMusicConfig/musicVideoType"))
            .and_then(|v| v.as_str())
            .unwrap_or("MUSIC_VIDEO_TYPE_ATV")
            .to_string();

        let second_col_runs = get_item_runs(data, 1);
        let artists = second_col_runs
            .map(|r| parse_artists_from_runs(r))
            .unwrap_or_default();
        let album = second_col_runs
            .map(|r| parse_album_from_runs(r))
            .unwrap_or_else(|| YtAlbumRef { name: None, id: None });

        results.push(SearchResult {
            video_id,
            title,
            artists,
            album,
            video_type,
        });
    }

    Ok(results)
}

/// Get the watch playlist for a video (used to retrieve album/artist info and lyrics browse ID).
pub fn get_watch_playlist(client: &Client, video_id: &str) -> Result<WatchTrack> {
    let body = json!({
        "videoId": video_id,
        "playlistId": format!("RDAMVM{video_id}"),
        "enablePersistentPlaylistPanel": true,
        "isAudioOnly": true,
        "tunerSettingValue": "AUTOMIX_SETTING_NORMAL",
        "watchEndpointMusicSupportedConfigs": {
            "watchEndpointMusicConfig": {
                "hasPersistentPlaylistPanel": true,
                "musicVideoType": "MUSIC_VIDEO_TYPE_ATV"
            }
        }
    });

    let resp = post(client, "next", body)?;

    // Navigate to the tabs inside the watch next results
    let tabs = resp
        .pointer("/contents/singleColumnMusicWatchNextResultsRenderer/tabbedRenderer/watchNextTabbedResultsRenderer/tabs")
        .and_then(|t| t.as_array())
        .context("Could not find watch playlist tabs")?;

    // First tab: tabs[0]/tabRenderer/content/musicQueueRenderer/content/playlistPanelRenderer
    let playlist_contents = tabs
        .first()
        .and_then(|tab| {
            tab.pointer("/tabRenderer/content/musicQueueRenderer/content/playlistPanelRenderer/contents")
        })
        .and_then(|c| c.as_array())
        .context("Could not find playlist panel contents")?;

    fn extract_renderer(item: &Value) -> Option<&Value> {
        item.get("playlistPanelVideoWrapperRenderer")
            .and_then(|w| w.pointer("/primaryRenderer/playlistPanelVideoRenderer"))
            .or_else(|| item.get("playlistPanelVideoRenderer"))
    }

    // Prefer the track matching the requested video_id; fall back to first track
    let track_data = playlist_contents
        .iter()
        .find_map(|item| {
            let r = extract_renderer(item)?;
            if r.get("videoId").and_then(|v| v.as_str()) == Some(video_id) {
                Some(r)
            } else {
                None
            }
        })
        .or_else(|| playlist_contents.iter().find_map(|item| extract_renderer(item)))
        .context("Could not find track in watch playlist")?;

    let vid_id = track_data
        .get("videoId")
        .and_then(|v| v.as_str())
        .unwrap_or(video_id)
        .to_string();

    let title = track_data
        .pointer("/title/runs/0/text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let video_type = track_data
        .pointer("/navigationEndpoint/watchEndpoint/watchEndpointMusicSupportedConfigs/watchEndpointMusicConfig/musicVideoType")
        .and_then(|v| v.as_str())
        .unwrap_or("MUSIC_VIDEO_TYPE_ATV")
        .to_string();

    let long_byline = track_data.get("longBylineText");
    let artists = long_byline
        .and_then(|b| b.get("runs"))
        .map(|r| parse_artists_from_runs(r))
        .unwrap_or_default();
    let album = long_byline
        .and_then(|b| b.get("runs"))
        .map(|r| parse_album_from_runs(r))
        .unwrap_or_else(|| YtAlbumRef { name: None, id: None });

    // Lyrics browse ID is in tab index 1 (if not unselectable)
    let lyrics_browse_id = tabs.get(1).and_then(|tab| {
        // "unselectable" key means no lyrics available
        let renderer = tab.get("tabRenderer")?;
        if renderer.get("unselectable").is_some() {
            return None;
        }
        renderer
            .pointer("/endpoint/browseEndpoint/browseId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    });

    Ok(WatchTrack {
        video_id: vid_id,
        title,
        artists,
        album,
        video_type,
        lyrics_browse_id,
    })
}

/// Browse an album by its browseId (e.g. "MPREb_...").
pub fn get_album(client: &Client, browse_id: &str) -> Result<Album> {
    let body = json!({ "browseId": browse_id });
    let resp = post(client, "browse", body)?;

    // Try new 2024 format first (twoColumnBrowseResultsRenderer), then fall back to old
    let (album_type, year, track_count, thumbnails) =
        if let Some(header) = resp.pointer("/header/musicDetailHeaderRenderer") {
            parse_album_header_old(header)
        } else if let Some(header) = resp
            .pointer("/contents/twoColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/musicResponsiveHeaderRenderer")
        {
            parse_album_header_new(header)
        } else {
            // fallback empty
            ("Album".to_string(), String::new(), 0u32, vec![])
        };

    // Tracks are in singleColumnBrowseResultsRenderer or twoColumnBrowseResultsRenderer
    let track_items = resp
        .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/musicShelfRenderer/contents")
        .or_else(|| resp.pointer("/contents/twoColumnBrowseResultsRenderer/secondaryContents/sectionListRenderer/contents/0/musicShelfRenderer/contents"))
        .and_then(|c| c.as_array());

    let mut tracks = Vec::new();
    if let Some(items) = track_items {
        for (i, item) in items.iter().enumerate() {
            let data = match item.get("musicResponsiveListItemRenderer") {
                Some(d) => d,
                None => continue,
            };
            let title = get_item_text(data, 0).unwrap_or("").to_string();
            // Track number: try to get from the renderer, fallback to index+1
            let track_number = data
                .get("index")
                .and_then(|idx| idx.pointer("/runs/0/text"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u32>().ok())
                .or(Some((i + 1) as u32));

            tracks.push(AlbumTrack { title, track_number });
        }
    }

    let actual_track_count = if track_count > 0 {
        track_count
    } else {
        tracks.len() as u32
    };

    Ok(Album {
        title: str_val(&resp, "/header/musicDetailHeaderRenderer/title/runs/0/text")
            .or_else(|| str_val(&resp, "/contents/twoColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/musicResponsiveHeaderRenderer/title/runs/0/text"))
            .unwrap_or("")
            .to_string(),
        album_type,
        year,
        track_count: actual_track_count,
        tracks,
        thumbnails,
    })
}

fn parse_album_header_old(header: &Value) -> (String, String, u32, Vec<AlbumThumbnail>) {
    let album_type = header
        .pointer("/subtitle/runs/0/text")
        .and_then(|v| v.as_str())
        .unwrap_or("Album")
        .to_string();

    // subtitle runs: ["Album", " • ", "2023"]  or  ["Album", " • ", "Artist", " • ", "2023"]
    let subtitle_runs = header
        .pointer("/subtitle/runs")
        .and_then(|r| r.as_array());
    let year = subtitle_runs
        .and_then(|runs| runs.last())
        .and_then(|r| r.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let track_count = header
        .pointer("/secondSubtitle/runs/0/text")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    let thumbnails = parse_thumbnails(
        header.pointer("/thumbnail/croppedSquareThumbnailRenderer/thumbnail/thumbnails"),
    );

    (album_type, year, track_count, thumbnails)
}

fn parse_album_header_new(header: &Value) -> (String, String, u32, Vec<AlbumThumbnail>) {
    let album_type = header
        .pointer("/subtitle/runs/0/text")
        .and_then(|v| v.as_str())
        .unwrap_or("Album")
        .to_string();

    let subtitle_runs = header
        .pointer("/subtitle/runs")
        .and_then(|r| r.as_array());
    let year = subtitle_runs
        .and_then(|runs| runs.last())
        .and_then(|r| r.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let track_count = header
        .pointer("/secondSubtitle/runs/0/text")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    let thumbnails = parse_thumbnails(
        header.pointer("/thumbnail/musicThumbnailRenderer/thumbnail/thumbnails"),
    );

    (album_type, year, track_count, thumbnails)
}

fn parse_thumbnails(node: Option<&Value>) -> Vec<AlbumThumbnail> {
    node.and_then(|n| n.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    Some(AlbumThumbnail {
                        url: t.get("url")?.as_str()?.to_string(),
                        width: t.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                        height: t.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Fetch lyrics by browse ID (e.g. "MPLYt_...").
pub fn get_lyrics(client: &Client, browse_id: &str) -> Result<Lyrics> {
    let body = json!({ "browseId": browse_id });
    let resp = post(client, "browse", body)?;

    // Try timed lyrics (newer format)
    if let Some(lyrics_text) = resp
        .pointer("/contents/elementRenderer/newElement/type/componentType/model/timedLyricsModel/lyricsData/timedLyricsData")
        .and_then(|v| v.as_str())
    {
        return Ok(Lyrics {
            lyrics: lyrics_text.to_string(),
            has_timestamps: true,
        });
    }

    // Standard lyrics in musicDescriptionShelfRenderer
    let lyrics_text = resp
        .pointer("/contents/sectionListRenderer/contents/0/musicDescriptionShelfRenderer/description/runs/0/text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(Lyrics {
        lyrics: lyrics_text,
        has_timestamps: false,
    })
}

/// Fetch a playlist's tracks by playlist ID.
pub fn get_playlist(client: &Client, playlist_id: &str) -> Result<Vec<SearchResult>> {
    // Browse ID for playlists is "VL" + playlist_id
    let browse_id = if playlist_id.starts_with("VL") {
        playlist_id.to_string()
    } else {
        format!("VL{playlist_id}")
    };

    let body = json!({ "browseId": browse_id });
    let resp = post(client, "browse", body)?;

    let contents = resp
        .pointer("/contents/singleColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/musicShelfRenderer/contents")
        .and_then(|c| c.as_array());

    let items = match contents {
        Some(items) => items,
        None => return Ok(vec![]),
    };

    let mut results = Vec::new();
    for item in items {
        let data = match item.get("musicResponsiveListItemRenderer") {
            Some(d) => d,
            None => continue,
        };

        let title = match get_item_text(data, 0) {
            Some(t) => t.to_string(),
            None => continue,
        };

        let video_id = data
            .pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/videoId")
            .or_else(|| data.pointer("/flexColumns/0/musicResponsiveListItemFlexColumnRenderer/text/runs/0/navigationEndpoint/watchEndpoint/videoId"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if video_id.is_empty() {
            continue;
        }

        let video_type = data
            .pointer("/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/watchEndpointMusicSupportedConfigs/watchEndpointMusicConfig/musicVideoType")
            .and_then(|v| v.as_str())
            .unwrap_or("MUSIC_VIDEO_TYPE_ATV")
            .to_string();

        let second_col_runs = get_item_runs(data, 1);
        let artists = second_col_runs
            .map(|r| parse_artists_from_runs(r))
            .unwrap_or_default();
        let album = second_col_runs
            .map(|r| parse_album_from_runs(r))
            .unwrap_or_else(|| YtAlbumRef { name: None, id: None });

        results.push(SearchResult {
            video_id,
            title,
            artists,
            album,
            video_type,
        });
    }

    Ok(results)
}

/// Download album art bytes from a URL.
pub fn download_album_art(client: &Client, url: &str) -> Option<Vec<u8>> {
    client
        .get(url)
        .send()
        .ok()
        .filter(|r| r.status().is_success())
        .and_then(|r| r.bytes().ok())
        .map(|b| b.to_vec())
}

pub fn make_client() -> Client {
    Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .expect("Failed to build HTTP client")
}
