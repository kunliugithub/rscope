//! Parallel file preparation. Only the receiving thread touches SQLite.
use crate::{
    language::{Language, ParsedFile},
    source::{Source, Stamp},
};
use anyhow::{Context, Result, bail};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::sync_channel,
    },
    thread,
    time::{Duration, Instant},
};

pub struct Prepared {
    pub path: PathBuf,
    pub stamp: Stamp,
    pub parsed: ParsedFile,
    pub read_time: Duration,
    pub parse_time: Duration,
}

fn prepare(path: &std::path::Path, allow_parse_errors: bool) -> Result<Prepared> {
    let name = path.to_str().context("source paths must be UTF-8")?;
    let started = Instant::now();
    let (source, stamp) = Source::read_for_index(name)?;
    let read_time = started.elapsed();
    let started = Instant::now();
    let parsed = Language::for_path(path)?
        .parse(&source, allow_parse_errors)
        .with_context(|| format!("parsing {name}"))?;
    Ok(Prepared {
        path: path.to_owned(),
        stamp,
        parsed,
        read_time,
        parse_time: started.elapsed(),
    })
}

pub fn run(
    files: &[PathBuf],
    jobs: usize,
    capacity: usize,
    allow_parse_errors: bool,
    mut consume: impl FnMut(Prepared) -> Result<()>,
) -> Result<()> {
    if jobs == 1 {
        for path in files {
            consume(prepare(path, allow_parse_errors)?)?;
        }
        return Ok(());
    }
    let next = AtomicUsize::new(0);
    thread::scope(|scope| {
        let (sender, receiver) = sync_channel(capacity);
        let mut workers = Vec::new();
        for _ in 0..jobs.min(files.len()) {
            let sender = sender.clone();
            let next = &next;
            workers.push(scope.spawn(move || {
                loop {
                    // Dropping the receiver cancels blocked sends; at most one
                    // already-started file per worker finishes after an error.
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = files.get(index) else {
                        break;
                    };
                    let result = prepare(path, allow_parse_errors);
                    let failed = result.is_err();
                    if sender.send(result).is_err() || failed {
                        break;
                    }
                }
            }));
        }
        drop(sender);
        let result: Result<()> = (|| {
            for prepared in &receiver {
                consume(prepared?)?;
            }
            Ok(())
        })();
        // Drop before joining, including on writer/parse failure, to unblock producers.
        drop(receiver);
        let mut panicked = false;
        for worker in workers {
            panicked |= worker.join().is_err();
        }
        result?;
        if panicked {
            bail!("parser worker panicked; index was not replaced");
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumer_failure_releases_blocked_producers() {
        let dir = tempfile::tempdir().unwrap();
        let files: Vec<_> = (0..32)
            .map(|i| {
                let path = dir.path().join(format!("file{i}.py"));
                std::fs::write(&path, "def leaf():\n    return 1\n").unwrap();
                path
            })
            .collect();
        for capacity in [0, 1] {
            let mut consumed = 0;
            let result = run(&files, 8, capacity, false, |_| {
                consumed += 1;
                bail!("simulated SQLite writer failure")
            });
            assert_eq!(consumed, 1);
            assert!(result.unwrap_err().to_string().contains("writer failure"));
        }
    }
}
