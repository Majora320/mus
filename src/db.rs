use directories::ProjectDirs;
use druid::Data;
use log::{info, trace};
use rusqlite::{Connection, NO_PARAMS, params, Transaction};
use rusqlite::Error::QueryReturnedNoRows;
use taglib::File;
use thiserror::Error;
use thiserror::private::PathAsDisplay;
use walkdir::WalkDir;
use std::fs::create_dir_all;

pub struct Database {
    conn: Connection
}

pub struct Library {
    id: i64,
    path: String,
    name: String,
}

impl Library {
    /// Returns the path of this library, or None for the 'Individual Tracks' library.
    pub fn path(&self) -> Option<&String> {
        if self.path == "NONE" {
            None
        } else {
            Some(&self.path)
        }
    }

    pub fn name(&self) -> &String {
        &self.name
    }
}

#[derive(Debug, Clone, Data)]
pub struct Track {
    id: i64,
    library_id: i64,
    path: String,
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    comment: Option<String>,
    genre: Option<String>,
    year: Option<i32>,
    track: Option<i32>,
    length: i32,
    bitrate: i32,
    samplerate: i32,
    rating: Option<i32>,
}

impl Track {
    pub fn get_field_as_string(&self, field: TrackField) -> String {
        match field {
            TrackField::Path       => self.path.clone(),
            TrackField::Title      => self.title.clone().unwrap_or_default(),
            TrackField::Artist     => self.artist.clone().unwrap_or_default(),
            TrackField::Album      => self.album.clone().unwrap_or_default(),
            TrackField::Comment    => self.comment.clone().unwrap_or_default(),
            TrackField::Genre      => self.genre.clone().unwrap_or_default(),
            TrackField::Year       => self.year.map(|y| y.to_string()).unwrap_or(String::new()),
            TrackField::Track      => self.year.map(|t| t.to_string()).unwrap_or(String::new()),
            TrackField::Length     => self.length.to_string(),
            TrackField::Bitrate    => self.bitrate.to_string(),
            TrackField::Samplerate => self.samplerate.to_string(),
            TrackField::Rating     => self.rating.unwrap_or(-1).to_string(),
        }
    }
}

#[derive(Debug, Copy, Clone, Data, PartialEq)]
pub enum TrackField {
    Path, Title, Artist, Album, Comment, Genre, Year,
    Track, Length, Bitrate, Samplerate, Rating
}

impl Track {
    pub fn path(&self)        -> &str { &self.path }
    pub fn title(&self)      -> Option<&str> { self.title.as_deref() }
    pub fn artist(&self)     -> Option<&str> { self.artist.as_deref() }
    pub fn album(&self)      -> Option<&str> { self.album.as_deref() }
    pub fn comment(&self)    -> Option<&str> { self.comment.as_deref() }
    pub fn genre(&self)      -> Option<&str> { self.title.as_deref() }
    pub fn year(&self)       -> Option<i32> { self.year }
    pub fn track(&self)      -> Option<i32> { self.track }
    pub fn length(&self)     -> i32 { self.length }
    pub fn bitrate(&self)    -> i32 { self.bitrate }
    pub fn samplerate(&self) -> i32 { self.samplerate }
    pub fn rating(&self)     -> Option<i32> { self.rating }
}

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Could not find common directories. Maybe set up xdg?")]
    CommonDirectories,
    #[error("There was a problem with the database.")]
    SqliteError(#[from] rusqlite::Error),
    #[error("A directory does not exist.")]
    WalkDirError(#[from] walkdir::Error),
}

impl Database {
    pub fn new() -> Result<Database, DatabaseError> {
        let dir = ProjectDirs::from(
            "org", "Jesus Software Corp.", "mus")
            .ok_or(DatabaseError::CommonDirectories)?
            .data_local_dir().to_path_buf();

        create_dir_all(&dir).unwrap();

        let path = dir.join("data.sq3");

        info!("Data path: {}", path.as_display());

        let conn = Connection::open(path)?;

        trace!("Connection established");

        let check = conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'track'",
            NO_PARAMS,
            |_row| Ok(()),
        );

        if check == Err(QueryReturnedNoRows) {
            init_db(&conn)?;
        } else if check.is_err() {
            check?;
        }

        Ok(Database {
            conn
        })
    }

    /// Libraries will not be nested.
    pub fn libraries(&self) -> Result<Vec<Library>, DatabaseError> {
        let mut stmt = self.conn.prepare("SELECT id, path, name FROM library;")?;
        let rows = stmt.query_map(NO_PARAMS, |row| {
            Ok(Library {
                id: row.get(0)?,
                path: row.get(1)?,
                name: row.get(2)?,
            })
        })?;

        let mut res = Vec::new();
        for row in rows {
            res.push(row?)
        }

        Ok(res)
    }

    /// Libraries cannot be nested.
    pub fn add_library(&mut self, path: String, name: String) -> Result<Library, DatabaseError> {
        info!("Adding library {} at {}", name, path);

        self.conn.execute("INSERT INTO library (name, path) VALUES (?1, ?2)",
                          params![name, path])?;
        let id = self.conn.query_row("SELECT id FROM library WHERE name = ? AND path = ?",
                                     params![name, path],
                                     |row| row.get(0))?;

        Ok(Library {
            id,
            path,
            name,
        })
    }

    /// Scan the library given. If `full_rescan` is true, then we will clear out the library
    /// completely and then repopulate it; otherwise, we will ignore tracks we already have.
    /// Returns the list of tracks that are no longer in the library that were there before, if any.
    pub fn scan_library(&mut self, library: Library, full_rescan: bool) -> Result<Vec<String>, DatabaseError> {
        trace!("Performing rescan on library {}", library.name);

        if full_rescan {
            trace!("Clearing library {}", library.name);
            let tx = self.conn.transaction()?;
            tx.execute(
                "DELETE \
                FROM playlist_tracks
                WHERE track_id IN
                    (SELECT track_id \
                    FROM track \
                    WHERE library_id = ?1);",
                params![library.id])?;
            tx.execute(
                "DELETE \
                FROM track
                WHERE library_id = ?1",
                params![library.id])?;

            tx.commit()?;
        }

        trace!("Collecting paths...");
        // Collect all of the paths into a list
        // May include non-track files
        let mut new_tracks: Vec<String> = Vec::new();
        for entry in WalkDir::new(&library.path).follow_links(true) {
            let entry = entry?;

            if entry.file_type().is_file() {
                let file = entry
                    .into_path()
                    .canonicalize().unwrap()
                    .into_os_string().into_string();
                if let Ok(file) = file {
                    if !new_tracks.contains(&file) { new_tracks.push(file); }
                }
            }
        }

        // Tracks that are now missing
        let mut res: Vec<String> = Vec::new();

        // Remove tracks that are already in the database and tracks that are now missing if we
        // aren't doing a full rescan
        if !full_rescan {
            trace!("Removing duplicates and old tracks");

            let tx = self.conn.transaction()?;

            tx.execute("CREATE TEMPORARY TABLE scan_results (path TEXT PRIMARY KEY NOT NULL);", NO_PARAMS)?;

            { // We have to do this in a new scope so that tx.commit() works
                let mut insert = tx.prepare("INSERT INTO scan_results (path) VALUES (?1)")?;
                for file in &new_tracks {
                    insert.execute(params![file])?;
                }
            }

            // Remove tracks from that database that are missing
            remove_missing_tracks(&tx, &library, &mut res)?;

            // And remove tracks from the new_tracks list that are already in the library
            new_tracks.clear();

            {
                let mut remove_duplicates = tx.prepare(
                    "SELECT scan_results.path \
                FROM scan_results \
                LEFT JOIN track ON track.path = scan_results.path \
                WHERE track.path IS NULL:"
                )?;

                for track in remove_duplicates.query_map(NO_PARAMS, |row|
                    row.get(0),
                )? {
                    new_tracks.push(track?)
                }
            }

            tx.commit()?;
        }

        // Whether we had to remove duplicates or not, we now have a raw list of paths that we can
        // add directly to the database. We have to process them to extract their metadata (and
        // determine if they are in fact valid tracks)

        let mut stmt = self.conn.prepare(
            "INSERT INTO track (library_id, path, title, artist, album, comment, genre, year, track, length, bitrate, samplerate, rating) \
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13);"
        )?;

        for path in new_tracks {
            if let Ok(file) = File::new(&path) {
                if let (Ok(tag), Ok(properties)) = (file.tag(), file.audioproperties()) {
                    let initial_rating: Option<u8> = None;
                    trace!("Adding track {} located at {}", tag.title().unwrap_or("?".to_string()), path);
                    stmt.execute(params![
                        library.id,
                        path,
                        tag.title(),
                        tag.artist(),
                        tag.album(),
                        tag.comment(),
                        tag.genre(),
                        tag.year(),
                        tag.track(),
                        properties.length(),
                        properties.bitrate(),
                        properties.samplerate(),
                        initial_rating // TODO: implement rating,
                    ])?;
                }
            }
        }

        Ok(res)
    }

    pub fn dump_all_tracks(&self) -> Result<Vec<Track>, DatabaseError> {
        trace!("Dumping tracks");
        let mut stmt = self.conn.prepare("SELECT * FROM track;")?;

        let mut res = Vec::new();
        for track in stmt.query_map(params![], |row| {
            Ok(Track {
                id:         row.get::<_, Option<i64>>(0)?.unwrap(),
                library_id: row.get::<_, Option<i64>>(1)?.unwrap(),
                path:       row.get::<_, Option<String>>(2)?.unwrap(),
                title:      row.get(3)?,
                artist:     row.get(4)?,
                album:      row.get(5)?,
                comment:    row.get(6)?,
                genre:      row.get(7)?,
                year:       row.get(8)?,
                track:      row.get(9)?,
                length:     row.get::<_, Option<i32>>(10)?.unwrap(),
                bitrate:    row.get::<_, Option<i32>>(11)?.unwrap(),
                samplerate: row.get::<_, Option<i32>>(12)?.unwrap(),
                rating:     row.get(13)?
            })
        })? {
            res.push(track?);
        }

        Ok(res)
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        // TODO
    }
}

fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    trace!("Creating database schema");
    let create = include_str!("create.sql");
    conn.execute_batch(create)
}

fn remove_missing_tracks(tx: &Transaction, library: &Library, res: &mut Vec<String>) -> Result<(), DatabaseError> {
    // Remove tracks in the library that are no longer present on disk
    // We unfortunately need to do this in two queries because we have to return the tracks
    // that were removed

    let mut missing_tracks = tx.prepare(
        "WITH current_paths AS
                    (SELECT path
                    FROM track
                    WHERE library_id = ?1)
               SELECT current_paths.path
               FROM current_paths
                   LEFT JOIN scan_results ON current_paths.path = scan_results.path
               WHERE scan_results.path IS NULL);"
    )?;

    let mut delete_missing_tracks = tx.prepare(
        "DELETE FROM track WHERE path = ?"
    )?;

    for track in missing_tracks.query_map(params!(library.id), |row|
        row.get(0),
    )? {
        let track = track?;
        delete_missing_tracks.execute(params![&track])?;
        res.push(track);
    }

    Ok(())
}