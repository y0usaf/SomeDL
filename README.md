# SomeDL-rs — Song+Metadata Downloader

A command-line tool to download music from YouTube with rich metadata pulled
from MusicBrainz (genre), Genius (album correction), and Deezer (ISRC, label).
No API tokens required.

> **Credits:** This is a Rust rewrite of the original Python project by
> [ChemistryGull](https://github.com/ChemistryGull/SomeDL), licensed under the
> [GPL-3.0](https://github.com/ChemistryGull/SomeDL?tab=GPL-3.0-1-ov-file).
> All credit for the original concept, API integration design, and metadata
> pipeline goes to ChemistryGull.

## Features

- Download by search query, YouTube URL, or YouTube playlist URL
- Metadata from multiple sources: YouTube Music, MusicBrainz, Genius, Deezer
- Supports MP3, Opus, M4A, OGG/Vorbis, FLAC output formats
- Album art embedded automatically
- Configurable output template, directory, and format

## Requirements

- [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) — must be on your `PATH`
- `ffmpeg` — required for audio conversion

## Installation

```bash
cargo install --path .
```

Or build and run directly:

```bash
cargo run -- "Artist - Song"
```

## Usage

```
somedl [OPTIONS] [INPUTS]...

INPUTS:
  "Artist - Song"                    search query
  "https://music.youtube.com/..."    YouTube Music URL
  "https://youtube.com/..."          YouTube URL
  "https://youtu.be/..."             shortened URL
  "https://...?list=PL..."           playlist URL

OPTIONS:
  -f, --format <FORMAT>   Output format [best|best/opus|best/m4a|opus|m4a|mp3|vorbis|flac]
  -o, --output <PATH>     Output directory
  -l, --here              Download to current directory
  -v, --verbose           Debug output
  -q, --quiet             Errors only
  -R, --download-report   Always show download report
      --no-musicbrainz    Skip MusicBrainz (no genre)
      --generate-config   Generate config file
      --version           Print version
```

## Configuration

Run `somedl --generate-config` to create a config file at:
- Linux: `~/.config/SomeDL-rs/somedl_config.toml`
- macOS: `~/Library/Application Support/SomeDL-rs/somedl_config.toml`
- Windows: `%APPDATA%\SomeDL-rs\somedl_config.toml`

## License

GPL-3.0-only — same as the original project.
