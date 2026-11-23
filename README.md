# bop-merge
[![GitHub Release](https://img.shields.io/github/v/release/sco1/bop-merge)](https://github.com/sco1/bop-merge/releases)
[![GitHub License](https://img.shields.io/github/license/sco1/bop-merge)](https://github.com/sco1/bop-merge/blob/main/LICENSE)

`bop-merge` is a helper utility for mirroring your yearly bop playlists into a single collection of bops for you to enjoy.

## Installation
Releases for each platform are built in CI and uploaded as artifacts for each [tagged release](https://github.com/sco1/bop-merge/releases).

You may also build your own release from the cloned source using `cargo install --path . --locked`.

You can confirm proper installation via the CLI:

```
$ bop-merge --help

Mirror every Spotify playlist, named as "YYYY Bops", into a single merged playlist

Usage: bop-merge <COMMAND>

Commands:
  login  Login to Spotify & retrieve an access token
  sync   Mirror the source playlists into the target playlist
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## Configuration
Use of this application requires the creation of a [Spotify application](https://developer.spotify.com/dashboard). When creating the application, it is assumed that your redirect URI is registered as the local listener `http://127.0.0.1:8888/callback`.

Two environment variables are required:
  * `BOPMERGE_CLIENT_ID` - Client ID for your registered Spotify application; client secret is not needed for the authorization flow
  * `BOPMERGE_TARGET_PLAYLIST_ID` - ID of your desired bop collection playlist; this must be created prior to using the application

These environment variables may be set in your environment, or provided by a local `.env` file.

## Usage
### `bop-merge login`
This prints a Spotify authorization URL. Open it in a browser and approve access; the browser is sent back to `http://127.0.0.1:8888/callback` and the terminal reports who is logged in.

The following scopes are the minimum set required for functionality:
  * `playlist-read-private`: read your playlists, including private ones
  * `playlist-modify-public`, `playlist-modify-private`: change the merged playlist

The refresh token is saved in your OS credential store & should be valid for 180 days.

### `bop-merge sync`
The synchronization flow parses the logged in user's owned playlists & extracts tracks from all playlists that match the `YYYY Bops` pattern (case-insensitive). Only music tracks hosted by spotify are considered for the merged playlist. Tracks are deduplicated and added to the merged playlist. Any tracks present in the merged playlist that are not present in the collected yearly bops playlists are removed.

A sample sync run should look like the following:

```text
$ bop-merge sync

logged in as BeepBoop
found target playlist 'Let's bop'
found 3 source playlists
2024 Bops: 48 tracks (0 skipped)
2025 Bops: 61 tracks (1 skipped)
2026 Bops: 55 tracks (0 skipped)
sync plan: 12 to add, 2 to remove
removed 2 tracks
added 12 tracks
```

The sync plan can be previewed without making any changes using the `--dry-run` flag:

```text
$ bop-merge sync --dry-run

logged in as BeepBoop
found target playlist 'Let's bop'
found 3 source playlists
2024 Bops: 48 tracks (0 skipped)
2025 Bops: 61 tracks (1 skipped)
2026 Bops: 55 tracks (0 skipped)
sync plan: 12 to add, 2 to remove
```

**NOTE:** To protect the merged playlist from being emptied by a renamed playlist or a typo in the target ID, `sync` refuses to change the target playlist if any of the following conditions are met:
  * No source playlists were found
  * The source playlists contain no tracks
  * The sync would remove more than 25% of the target's tracks.

This safety block can be overridden by passing the `--force` flag.
