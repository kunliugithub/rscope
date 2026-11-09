use anyhow::{Context, Result, bail};
use std::{
    fs::{self, File, Metadata},
    io::{BufRead, BufReader, Read},
    time::UNIX_EPOCH,
};

/// Cheap freshness metadata, not a content hash. Preserved size/mtime changes
/// can evade this check; callers must rebuild after editing their source tree.
#[derive(Debug, PartialEq, Eq)]
pub struct Stamp {
    pub byte_len: i64,
    pub modified_ns: String,
}

impl Stamp {
    fn from_metadata(metadata: Metadata) -> Result<Self> {
        if !metadata.is_file() {
            bail!("source is not a regular file");
        }
        let modified = metadata.modified()?;
        let modified_ns = match modified.duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos().to_string(),
            Err(error) => format!("-{}", error.duration().as_nanos()),
        };
        Ok(Self {
            byte_len: metadata.len().try_into()?,
            modified_ns,
        })
    }
}

pub struct Source {
    path: String,
    reader: BufReader<File>,
    stamp: Stamp,
    pub line: i64,
    pub text: String,
}

impl Source {
    pub fn open(path: &str, expected: Option<&Stamp>) -> Result<Self> {
        let file = File::open(path)
            .with_context(|| format!("opening source {path}; rebuild with -b if moved/deleted"))?;
        let stamp = Stamp::from_metadata(file.metadata()?)?;
        if expected.is_some_and(|expected| *expected != stamp) {
            bail!(
                "source changed since indexing: {path}; rebuild with -b before querying symbol locations"
            );
        }
        Ok(Self {
            path: path.to_owned(),
            reader: BufReader::new(file),
            stamp,
            line: 0,
            text: String::new(),
        })
    }

    pub fn read_for_index(path: &str) -> Result<(String, Stamp)> {
        let mut source = Self::open(path, None)?;
        let mut text = String::new();
        source
            .reader
            .read_to_string(&mut text)
            .with_context(|| format!("reading {path} as UTF-8"))?;
        source.verify_unchanged()?;
        Ok((text, source.stamp))
    }

    pub fn next_line(&mut self) -> Result<bool> {
        self.text.clear();
        if self.reader.read_line(&mut self.text).with_context(|| {
            format!("reading {} as UTF-8 near line {}", self.path, self.line + 1)
        })? == 0
        {
            return Ok(false);
        }
        self.line += 1;
        if self.text.ends_with('\n') {
            self.text.pop();
            if self.text.ends_with('\r') {
                self.text.pop();
            }
        }
        Ok(true)
    }

    pub fn snippet_at(&mut self, line: i64) -> Result<String> {
        while self.line < line {
            if !self.next_line()? {
                bail!(
                    "indexed line {line} no longer exists in {}; rebuild with -b",
                    self.path
                );
            }
        }
        Ok(self.text.trim().to_owned())
    }

    pub fn verify_unchanged(&self) -> Result<()> {
        let handle_stamp = Stamp::from_metadata(self.reader.get_ref().metadata()?)?;
        let path_stamp = Stamp::from_metadata(fs::metadata(&self.path).with_context(|| {
            format!(
                "checking source {}; rebuild with -b if moved/deleted",
                self.path
            )
        })?)?;
        if handle_stamp != self.stamp || path_stamp != self.stamp {
            bail!(
                "source changed while reading {}; rerun the operation after edits finish",
                self.path
            );
        }
        Ok(())
    }
}
