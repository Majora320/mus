BEGIN;

CREATE TABLE track
(
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    library_id INTEGER NOT NULL,
    path       TEXT    NOT NULL, -- Absolute path
    title      TEXT,
    artist     TEXT,
    album      TEXT,
    comment    TEXT,
    genre      TEXT,
    year       INTEGER,
    track      INTEGER,
    length     INTEGER NOT NULL, -- In seconds
    bitrate    INTEGER NOT NULL, -- In kb/s
    samplerate INTEGER NOT NULL, -- In kb/s
    rating     INTEGER,
    FOREIGN KEY (library_id) REFERENCES library (id)
);

CREATE UNIQUE INDEX path_index
    ON track (path);

CREATE INDEX artist_index
    ON track (artist);

CREATE INDEX album_index
    ON track (artist, album);

CREATE INDEX genre_index
    ON track (genre);

CREATE TABLE library
(
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL,
    name TEXT,
    UNIQUE (path),
    UNIQUE (name)
);

-- Special library used for individual tracks
INSERT INTO library (path)
VALUES ('NONE');

CREATE TABLE playlist
(
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    UNIQUE (name)
);

CREATE TABLE playlist_tracks
(
    id       INTEGER,
    track_id INTEGER,
    FOREIGN KEY (id) REFERENCES playlist (id),
    FOREIGN KEY (track_id) REFERENCES track (id)
);

COMMIT;
