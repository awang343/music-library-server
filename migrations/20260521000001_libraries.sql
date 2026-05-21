-- Introduce a `libraries` table; scope tracks and playlists by library_id.
-- Existing rows are migrated into a placeholder library (id=1) that the
-- server's bootstrap renames/repaths from config on next startup.

PRAGMA foreign_keys = OFF;

CREATE TABLE libraries (
    id        INTEGER PRIMARY KEY,
    name      TEXT NOT NULL UNIQUE,
    root_path TEXT NOT NULL
);

INSERT INTO libraries (id, name, root_path) VALUES (1, '__pending__', '');

CREATE TABLE tracks_new (
    id            INTEGER PRIMARY KEY,
    library_id    INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    path          TEXT NOT NULL,
    title         TEXT,
    album         TEXT,
    artist        TEXT,
    album_artist  TEXT,
    track_no      INTEGER,
    disc_no       INTEGER,
    duration_ms   INTEGER,
    year          INTEGER,
    bitrate       INTEGER,
    sample_rate   INTEGER,
    channels      INTEGER,
    file_size     INTEGER NOT NULL,
    mtime         INTEGER NOT NULL,
    content_hash  TEXT,
    added_at      INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    UNIQUE (library_id, path)
);

INSERT INTO tracks_new (
    id, library_id, path, title, album, artist, album_artist,
    track_no, disc_no, duration_ms, year, bitrate, sample_rate, channels,
    file_size, mtime, content_hash, added_at, updated_at
)
SELECT id, 1, path, title, album, artist, album_artist,
       track_no, disc_no, duration_ms, year, bitrate, sample_rate, channels,
       file_size, mtime, content_hash, added_at, updated_at
FROM tracks;

DROP TABLE tracks;
ALTER TABLE tracks_new RENAME TO tracks;

CREATE INDEX tracks_library_idx      ON tracks(library_id);
CREATE INDEX tracks_album_idx        ON tracks(album);
CREATE INDEX tracks_artist_idx       ON tracks(artist);
CREATE INDEX tracks_album_artist_idx ON tracks(album_artist);
CREATE INDEX tracks_title_idx        ON tracks(title);

CREATE TABLE playlists_new (
    id          INTEGER PRIMARY KEY,
    library_id  INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    description TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    UNIQUE (library_id, name)
);

INSERT INTO playlists_new (id, library_id, name, description, created_at, updated_at)
SELECT id, 1, name, description, created_at, updated_at FROM playlists;

DROP TABLE playlists;
ALTER TABLE playlists_new RENAME TO playlists;

CREATE INDEX playlists_library_idx ON playlists(library_id);

PRAGMA foreign_keys = ON;
