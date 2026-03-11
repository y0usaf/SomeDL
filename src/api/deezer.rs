use reqwest::blocking::Client;
use serde_json::Value;

const DEEZER_BASE: &str = "https://api.deezer.com";

#[derive(Debug, Clone, Default)]
pub struct DeezerData {
    pub isrc: String,
    pub label: Option<String>,
    pub album_name: Option<String>,
    pub album_id: Option<u64>,
    pub artist_name: Option<String>,
    pub genres: Vec<String>,
}

/// Search Deezer for a track and return combined album metadata.
pub fn get_album_data(
    client: &Client,
    artist: &str,
    album: &str,
    song: &str,
) -> Option<DeezerData> {
    let result = search_song(client, artist, album, song)
        .or_else(|| {
            // Retry with cleaned song title (no parens)
            let clean = regex::Regex::new(r"\(.*?\)")
                .unwrap()
                .replace_all(song, "")
                .trim()
                .to_string();
            if clean != song {
                search_song(client, artist, album, &clean)
            } else {
                None
            }
        })?;

    let isrc = result
        .pointer("/data/0/isrc")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let album_id = result
        .pointer("/data/0/album/id")
        .and_then(|v| v.as_u64())?;

    let album_resp = get_album_by_id(client, album_id)?;

    let genres: Vec<String> = album_resp
        .pointer("/genres/data")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|g| g.get("name")?.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Some(DeezerData {
        isrc,
        label: album_resp
            .get("label")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        album_name: album_resp
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        album_id: Some(album_id),
        artist_name: album_resp
            .pointer("/artist/name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        genres,
    })
}

fn search_song(client: &Client, artist: &str, album: &str, song: &str) -> Option<Value> {
    let url = format!(
        "{DEEZER_BASE}/search/?q=artist:\"{artist}\" album:\"{album}\" track:\"{song}\"&index=0&limit=5",
        artist = encode(artist),
        album = encode(album),
        song = encode(song),
    );

    let resp = client
        .get(&url)
        .send()
        .ok()
        .filter(|r| r.status().is_success())
        .and_then(|r| r.json::<Value>().ok())?;

    if resp.get("error").is_some() {
        return None;
    }

    let total = resp.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
    if total == 0 {
        return None;
    }

    Some(resp)
}

fn get_album_by_id(client: &Client, id: u64) -> Option<Value> {
    let url = format!("{DEEZER_BASE}/album/{id}");
    let resp = client
        .get(&url)
        .send()
        .ok()
        .filter(|r| r.status().is_success())
        .and_then(|r| r.json::<Value>().ok())?;

    if resp.get("error").is_some() {
        return None;
    }

    Some(resp)
}

fn encode(s: &str) -> String {
    s.replace('"', "%22").replace(' ', "%20")
}
