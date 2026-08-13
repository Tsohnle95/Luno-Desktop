//! Exportify CSV parsing.  This intentionally avoids a lossy line-based split:
//! quoted commas and quoted newlines are valid CSV data.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvTrack {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsvError {
    Io(String),
    UnterminatedQuote,
    UnevenFields { row: usize },
    MissingColumn(&'static str),
    MissingField { row: usize, column: &'static str },
}

pub fn parse_file(path: impl AsRef<Path>) -> Result<Vec<CsvTrack>, CsvError> {
    let mut input = String::new();
    File::open(path).map_err(|e| CsvError::Io(e.to_string()))?
        .read_to_string(&mut input).map_err(|e| CsvError::Io(e.to_string()))?;
    parse(input.as_bytes())
}

pub fn parse(input: &[u8]) -> Result<Vec<CsvTrack>, CsvError> {
    let text = String::from_utf8_lossy(input).trim_start_matches('\u{feff}').to_string();
    let rows = records(&text)?;
    let Some(header) = rows.first() else { return Ok(Vec::new()); };
    let column = |name: &'static str| header.iter().position(|v| v.trim() == name)
        .ok_or(CsvError::MissingColumn(name));
    let title = column("Track Name")?;
    let artist = column("Artist Name(s)")?;
    let album = header.iter().position(|v| v.trim() == "Album Name");
    let url = header.iter().position(|v| v.trim() == "URL");
    let mut tracks = Vec::new();
    for (index, row) in rows.iter().skip(1).enumerate() {
        let row_number = index + 2;
        if row.iter().all(|v| v.trim().is_empty()) { continue; }
        if row.len() != header.len() { return Err(CsvError::UnevenFields { row: row_number }); }
        let value = |at: usize, name: &'static str| {
            let value = row[at].trim();
            if value.is_empty() { Err(CsvError::MissingField { row: row_number, column: name }) } else { Ok(value.to_string()) }
        };
        tracks.push(CsvTrack {
            title: value(title, "Track Name")?,
            artist: value(artist, "Artist Name(s)")?,
            album: album.and_then(|i| nonempty(row[i])),
            url: url.and_then(|i| nonempty(row[i])),
        });
    }
    Ok(tracks)
}

fn nonempty(value: &str) -> Option<String> { (!value.trim().is_empty()).then(|| value.trim().to_string()) }

fn records(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    let mut result = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            match ch {
                '"' if chars.peek() == Some(&'"') => { field.push('"'); chars.next(); }
                '"' => quoted = false,
                _ => field.push(ch),
            }
        } else {
            match ch {
                '"' if field.is_empty() => quoted = true,
                ',' => { row.push(std::mem::take(&mut field)); }
                '\n' => { row.push(std::mem::take(&mut field)); result.push(std::mem::take(&mut row)); }
                '\r' => if chars.peek() != Some(&'\n') { row.push(std::mem::take(&mut field)); result.push(std::mem::take(&mut row)); },
                _ => field.push(ch),
            }
        }
    }
    if quoted { return Err(CsvError::UnterminatedQuote); }
    if !field.is_empty() || !row.is_empty() { row.push(field); result.push(row); }
    Ok(result)
}

impl From<io::Error> for CsvError { fn from(e: io::Error) -> Self { Self::Io(e.to_string()) } }
