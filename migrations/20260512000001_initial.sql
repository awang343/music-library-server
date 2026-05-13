PRAGMA foreign_keys = ON;

CREATE TABLE tracks (
    id            INTEGER PRIMARY KEY,
    path          TEXT NOT NULL UNIQUE,
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
    updated_at    INTEGER NOT NULL
);

CREATE INDEX tracks_album_idx        ON tracks(album);
CREATE INDEX tracks_artist_idx       ON tracks(artist);
CREATE INDEX tracks_album_artist_idx ON tracks(album_artist);
CREATE INDEX tracks_title_idx        ON tracks(title);

CREATE TABLE tags (
    id        INTEGER PRIMARY KEY,
    namespace TEXT NOT NULL,
    value     TEXT NOT NULL,
    UNIQUE(namespace, value)
);

CREATE INDEX tags_value_idx ON tags(value);

CREATE TABLE track_tags (
    track_id  INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    tag_id    INTEGER NOT NULL REFERENCES tags(id)   ON DELETE CASCADE,
    source    TEXT NOT NULL,
    added_at  INTEGER NOT NULL,
    PRIMARY KEY (track_id, tag_id, source)
);

CREATE INDEX track_tags_tag_idx    ON track_tags(tag_id);
CREATE INDEX track_tags_source_idx ON track_tags(source);
