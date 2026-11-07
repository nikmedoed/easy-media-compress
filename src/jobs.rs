use crate::media::{self, Kind, Options, Tools, Update};
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::{
    collections::HashSet,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct Job {
    pub id: usize,
    pub kind: Kind,
    pub input: PathBuf,
    pub output: PathBuf,
    pub options: Options,
    pub input_bytes: u64,
}

#[derive(Debug)]
pub enum Event {
    Queued(Job),
    Started(usize),
    Update(usize, Update),
    Finished(usize, Result<u64, String>),
    Cancelled(usize),
    ScanProblem(String),
    ScanDone,
}

type Post = Arc<dyn Fn(Event) + Send + Sync>;
type Request = (Vec<PathBuf>, Options, Arc<media::Cancellation>);

pub struct Queue {
    requests: Sender<Request>,
    cancellation: Arc<media::Cancellation>,
    pub events: Receiver<Event>,
}

impl Queue {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let (tx_events, events) = unbounded();
        let post: Post = Arc::new(move |event| {
            if tx_events.send(event).is_ok() {
                wake();
            }
        });
        let (requests, incoming) = unbounded();
        thread::spawn(move || coordinate(incoming, post));
        Self {
            requests,
            events,
            cancellation: Arc::default(),
        }
    }

    pub fn submit(&self, inputs: Vec<PathBuf>, options: Options) {
        let _ = self
            .requests
            .send((inputs, options, self.cancellation.clone()));
    }

    pub fn cancel(&mut self) {
        std::mem::take(&mut self.cancellation).cancel();
    }
}

fn key(path: &Path) -> OsString {
    if cfg!(windows) {
        path.as_os_str().to_ascii_lowercase()
    } else {
        path.as_os_str().to_owned()
    }
}

fn scan(
    inputs: Vec<PathBuf>,
    filter: Option<Kind>,
    post: &Post,
    cancellation: &media::Cancellation,
) -> Vec<(PathBuf, Kind, u64)> {
    let mut work = Vec::new();
    let mut seen = HashSet::new();
    for input in inputs {
        let entries = WalkDir::new(&input)
            .follow_root_links(true)
            .sort_by_file_name();
        for entry in entries {
            if cancellation.is_cancelled() {
                return work;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    post(Event::ScanProblem(error.to_string()));
                    continue;
                }
            };
            let path = entry.path();
            let Some(kind) = media::kind(path).filter(|kind| filter.is_none_or(|f| f == *kind))
            else {
                continue;
            };
            if !(entry.file_type().is_file() || (entry.depth() == 0 && path.is_file())) {
                continue;
            }
            // Never enqueue another worker's in-progress temporary output.
            if path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(".compress-"))
            {
                continue;
            }
            let absolute = match dunce::canonicalize(path) {
                Ok(path) => path,
                Err(error) => {
                    post(Event::ScanProblem(format!("{}: {error}", path.display())));
                    continue;
                }
            };
            if seen.insert(key(&absolute)) {
                let size = entry.metadata().map(|m| m.len()).unwrap_or_default();
                let mut input = absolute;
                // Keep the supplied extension on explicit file links.
                if entry.path_is_symlink()
                    && let Ok(alias) = std::path::absolute(path)
                {
                    input = alias;
                }
                work.push((input, kind, size));
            }
        }
    }
    work
}

fn coordinate(incoming: Receiver<Request>, post: Post) {
    let (tx_jobs, jobs) = unbounded::<(Job, Arc<media::Cancellation>)>();
    let tools = Arc::new(Tools::default());
    let outputs = Arc::new(Mutex::new(HashSet::new()));
    let mut id = 0;
    let mut workers_started = false;
    for (paths, options, cancellation) in incoming {
        let files = scan(paths, options.only, &post, &cancellation);
        if !files.is_empty() && !workers_started {
            for _ in 0..4 {
                let (jobs, post, tools, outputs) =
                    (jobs.clone(), post.clone(), tools.clone(), outputs.clone());
                thread::spawn(move || {
                    for (job, cancellation) in jobs {
                        if !cancellation.is_cancelled() {
                            post(Event::Started(job.id));
                        }
                        let result = media::convert(
                            &tools,
                            &job.input,
                            &job.output,
                            job.kind,
                            &job.options,
                            &cancellation,
                            |update| {
                                post(Event::Update(job.id, update));
                            },
                        );
                        outputs.lock().unwrap().remove(&key(&job.output));
                        if result
                            .as_ref()
                            .is_err_and(|error| error.is::<media::Cancelled>())
                        {
                            post(Event::Cancelled(job.id));
                        } else {
                            post(Event::Finished(
                                job.id,
                                result.map_err(|error| format!("{error:#}")),
                            ));
                        }
                    }
                });
            }
            workers_started = true;
        }
        for (input, kind, input_bytes) in files {
            if cancellation.is_cancelled() {
                break;
            }
            let mut reserved = outputs.lock().unwrap();
            let output = (1..)
                .find_map(|serial| {
                    let output = options.output_name(&input, kind, serial);
                    let name = key(&output);
                    // Refuse to overwrite any existing media, including a previous conversion
                    // that might itself be queued as an input.
                    match output.symlink_metadata() {
                        Ok(_) => None,
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                            reserved.insert(name).then_some(Ok(output))
                        }
                        Err(err) => Some(Err(err)),
                    }
                })
                .expect("output path iterator is infinite");
            drop(reserved);
            let output = match output {
                Ok(path) => path,
                Err(error) => {
                    post(Event::ScanProblem(format!("{}: {error}", input.display())));
                    continue;
                }
            };
            let job = Job {
                id,
                kind,
                input,
                output,
                options: options.clone(),
                input_bytes,
            };
            id += 1;
            post(Event::Queued(job.clone()));
            let _ = tx_jobs.send((job, cancellation.clone()));
        }
        post(Event::ScanDone);
    }
}
