use reqwest::blocking::Client;
use serde_json::Value;
use std::thread;
use std::time::Duration;

const MB_BASE: &str = "https://musicbrainz.org/ws/2";
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn user_agent() -> String {
    format!("SomeDL/{VERSION} (html.gull@gmail.com)")
}

fn mb_get(client: &Client, url: &str, retries: u32) -> Option<Value> {
    let mut attempt = 0u32;
    loop {
        match client
            .get(url)
            .header("User-Agent", user_agent())
            .send()
        {
            Ok(resp) if resp.status().is_success() => {
                return resp.json::<Value>().ok();
            }
            Ok(resp) => {
                log::warn!("MusicBrainz returned status {}", resp.status());
            }
            Err(e) => {
                let wait = 5 + attempt * attempt;
                log::warn!(
                    "MusicBrainz request failed: {e}. Retrying in {wait}s ({} attempts left)",
                    retries.saturating_sub(attempt)
                );
                thread::sleep(Duration::from_secs(wait as u64));
            }
        }

        attempt += 1;
        if attempt > retries {
            return None;
        }
    }
}

/// Search MusicBrainz recordings by artist + song title.
/// Returns the first matching recording JSON blob (with artist-credit and tags).
pub fn get_song_by_name(
    client: &Client,
    artist: &str,
    song: Option<&str>,
    retries: u32,
) -> Option<Value> {
    let query = match song {
        Some(s) => format!("artist:({artist}) AND recording:({s})"),
        None => format!("artist:\"{artist}\""),
    };
    let url = format!("{MB_BASE}/recording/?query={query}&fmt=json",
        query = urlencoding_simple(&query));

    let resp = mb_get(client, &url, retries)?;

    if resp.get("error").is_some() {
        log::error!("MusicBrainz error: {:?}", resp.get("error"));
        return None;
    }

    let recordings = resp.get("recordings")?.as_array()?;
    if recordings.is_empty() {
        return None;
    }

    Some(resp)
}

/// Fetch artist details (including tags/genres) by MusicBrainz ID.
pub fn get_artist_by_mbid(client: &Client, mbid: &str, retries: u32) -> Option<Value> {
    let url = format!("{MB_BASE}/artist/{mbid}?inc=tags&fmt=json");
    let resp = mb_get(client, &url, retries)?;
    if resp.get("error").is_some() {
        return None;
    }
    Some(resp)
}

/// Extract the highest-count genre tag from an artist response.
pub fn extract_top_genre(artist_resp: &Value) -> Option<String> {
    let tags = artist_resp.get("tags")?.as_array()?;
    tags.iter()
        .filter_map(|tag| {
            let name = tag.get("name")?.as_str()?;
            let count = tag.get("count")?.as_u64().unwrap_or(0);
            Some((name.to_string(), count))
        })
        .max_by_key(|(_, count)| *count)
        .map(|(name, _)| name)
}

/// Extract artist MBID and name from a recording search response.
pub fn extract_artist_info(recording_resp: &Value) -> Option<(String, String)> {
    let recordings = recording_resp.get("recordings")?.as_array()?;
    let first = recordings.first()?;
    let artist_credit = first.get("artist-credit")?.as_array()?;
    let first_credit = artist_credit.first()?;
    let artist = first_credit.get("artist")?;
    let id = artist.get("id")?.as_str()?.to_string();
    let name = first_credit
        .get("name")
        .or_else(|| artist.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Some((id, name))
}

fn urlencoding_simple(s: &str) -> String {
    // Minimal percent-encoding for query parameters
    let mut out = String::with_capacity(s.len() * 2);
    for ch in s.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~'
            | '(' | ')' | ':' | '/' | ' ' => {
                if ch == ' ' {
                    out.push('+');
                } else {
                    out.push(ch);
                }
            }
            c => {
                for byte in c.to_string().as_bytes() {
                    out.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    out
}
