use anyhow::{Context, Result};
use lofty::config::WriteOptions;
use lofty::file::TaggedFileExt;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::{ItemKey, Tag};
use std::path::Path;

use crate::api::ytmusic::AlbumThumbnail;
use crate::config::Config;

/// All the metadata we want to embed into the downloaded file.
#[derive(Debug, Default)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: String,
    pub genre: String,
    pub track_pos: Option<u32>,
    pub track_count: Option<u32>,
    pub lyrics: Option<String>,
    pub isrc: Option<String>,
    pub copyright: Option<String>,
    pub musicbrainz_artist_id: Option<String>,
    pub source_url: String,
    pub album_art_url: Option<String>,
    pub thumbnails: Vec<AlbumThumbnail>,
}

impl TrackMetadata {
    /// Return the highest-res thumbnail URL, if any.
    pub fn best_thumbnail_url(&self) -> Option<&str> {
        self.thumbnails.last().map(|t| t.url.as_str())
    }
}

/// Write metadata to the audio file at `path`.
pub fn write_metadata(
    config: &Config,
    meta: &TrackMetadata,
    path: &Path,
    art_bytes: Option<&[u8]>,
) -> Result<()> {
    let mut tagged_file = Probe::open(path)
        .context("Failed to open audio file for tagging")?
        .read()
        .context("Failed to read audio tags")?;

    // Get or create the primary tag
    let tag_type = tagged_file.primary_tag_type();
    if tagged_file.primary_tag().is_none() {
        tagged_file.insert_tag(Tag::new(tag_type));
    }
    let tag = tagged_file.primary_tag_mut().unwrap();

    // Basic fields
    tag.set_title(meta.title.clone());
    tag.set_artist(meta.artist.clone());
    tag.set_album(meta.album.clone());

    if !meta.year.is_empty() {
        if let Ok(y) = meta.year.parse::<u32>() {
            tag.set_year(y);
        }
    }

    if config.metadata.genre && !meta.genre.is_empty() {
        tag.set_genre(meta.genre.clone());
    }

    if let Some(pos) = meta.track_pos {
        tag.set_track(pos);
    }
    if let Some(total) = meta.track_count {
        tag.set_track_total(total);
    }

    // Lyrics
    if config.metadata.lyrics {
        if let Some(lyrics) = &meta.lyrics {
            if !lyrics.is_empty() {
                tag.insert_text(ItemKey::Lyrics, lyrics.clone());
            }
        }
    }

    // ISRC
    if config.metadata.isrc {
        if let Some(isrc) = &meta.isrc {
            if !isrc.is_empty() {
                tag.insert_text(ItemKey::Isrc, isrc.clone());
            }
        }
    }

    // Copyright
    if config.metadata.copyright {
        if let Some(copyright) = &meta.copyright {
            if !copyright.is_empty() {
                tag.insert_text(ItemKey::CopyrightMessage, copyright.clone());
            }
        }
    }

    // MusicBrainz artist ID
    if let Some(mbid) = &meta.musicbrainz_artist_id {
        if !mbid.is_empty() {
            tag.insert_text(ItemKey::MusicBrainzArtistId, mbid.clone());
        }
    }

    // Note: AudioSourceUrl (WOAS) is a URL link frame in ID3v2 and requires
    // format-specific API — skip via the generic Tag interface to avoid errors.

    // Album art
    if let Some(art) = art_bytes {
        let picture = Picture::new_unchecked(
            PictureType::CoverFront,
            Some(MimeType::Jpeg),
            Some("Cover".to_string()),
            art.to_vec(),
        );
        tag.push_picture(picture);
    }

    // Write back
    tagged_file
        .save_to_path(path, WriteOptions::default())
        .with_context(|| format!("Failed to save audio tags to '{}'", path.display()))?;

    log::info!("Successfully wrote metadata to {}", path.display());
    Ok(())
}
