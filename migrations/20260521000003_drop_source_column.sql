-- All track_tags rows are now user-sourced (prior migration removed the rest,
-- and the scanner no longer creates them). Drop the `source` column.
-- SQLite requires a table rebuild because `source` is part of the primary key.
-- No other tables reference track_tags, so dropping and renaming is safe.

CREATE TABLE track_tags_new (
    track_id  INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    tag_id    INTEGER NOT NULL REFERENCES tags(id)   ON DELETE CASCADE,
    added_at  INTEGER NOT NULL,
    PRIMARY KEY (track_id, tag_id)
);

INSERT INTO track_tags_new (track_id, tag_id, added_at)
SELECT track_id, tag_id, added_at FROM track_tags;

DROP TABLE track_tags;
ALTER TABLE track_tags_new RENAME TO track_tags;

CREATE INDEX track_tags_tag_idx ON track_tags(tag_id);
