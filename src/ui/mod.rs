use core::{
    any::type_name,
    error::Error,
    fmt,
    future::Future,
    ops::Deref,
    sync::atomic::{AtomicBool, Ordering},
};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use burn::tensor::Device;
use iced::{
    Element, Fill, Font, Subscription, Task, Theme,
    alignment::{Horizontal, Vertical},
    font::{self, Weight},
    time,
    widget::{button, column, container, row, stack, text, text_editor},
};
use hmm::{Inference, VoiceAccent, VoiceEntry, VoiceEnumerator, VoiceGender};
use tokio::{
    runtime::{Builder, Runtime},
    task::JoinError,
};

mod audio;
mod compose;
mod done;
mod icons;
mod playback;
mod queue;
mod tabs;
mod theme;
mod toast;
mod voices;

const TITLE: &str = "HMM";
const TITLE_SIZE: f32 = 30.0;
const DEFAULT_TEXT: &str = "Testing text to speech with Burn!";
const WORKER_STACK_BYTES: usize = 32 * 1024 * 1024;
const VOICES_PANEL_WIDTH: f32 = 390.0;
const TOAST_WIDTH: f32 = 380.0;
const MAX_TOASTS: usize = 6;
const TICK: Duration = Duration::from_millis(40);
const DOT_COUNT: usize = 3;
const DOT_PERIOD: f32 = 1.2;
const DOT_STAGGER: f32 = 0.14;
const DOT_LIFT: f32 = 8.0;
const ACCENTS: [VoiceAccent; 2] = [
    VoiceAccent::EnglishAmerican,
    VoiceAccent::EnglishBritish,
];
const GENDERS: [VoiceGender; 2] = [VoiceGender::Female, VoiceGender::Male];

pub fn run() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title(TITLE)
        .theme(App::theme)
        .subscription(App::subscription)
        .window_size((1180.0, 760.0))
        .run()
}

static WORKER: OnceLock<Runtime> = OnceLock::new();

fn worker() -> &'static Runtime {
    WORKER.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(4)
            .thread_stack_size(WORKER_STACK_BYTES)
            .thread_name("tts-worker")
            .enable_all()
            .build()
            .expect("failed to start the worker runtime")
    })
}

async fn on_worker<T, F>(scope: Scope, work: F) -> Result<T, UiError>
where
    T: Send + 'static,
    F: Future<Output = Result<T, UiError>> + Send + 'static,
{
    worker()
        .spawn(work)
        .await
        .map_err(|error| join_error(scope, error))?
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Voices,
    Engine,
    Input,
    Synthesis,
    Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Compose,
    Queue,
    Done,
}

#[derive(Debug, Clone)]
enum Message {
    Tick(Instant),
    VoicesLoaded(Result<Shared<VoiceEnumerator>, UiError>),
    EngineLoaded(Result<Shared<Inference>, UiError>),
    DoneLoaded(Result<Vec<PathBuf>, UiError>),
    IconFontLoaded(Result<(), String>),
    Reload,
    Search(String),
    Accent(Option<VoiceAccent>),
    Gender(Option<VoiceGender>),
    SelectVoice(usize),
    Editor(text_editor::Action),
    Speed(f32),
    OutputDir(String),
    Enqueue,
    SetPage(Page),
    RemoveQueued(u64),
    OpenDoneFolder(u64),
    PlayDone(u64),
    StopPlayback,
    PlaybackFinished {
        id: u64,
        result: Result<(), UiError>,
    },
    OutputActionFinished(Result<(), UiError>),
    DeleteDone(u64),
    DeleteDoneFinished {
        id: u64,
        result: Result<(), UiError>,
    },
    QueueFinished {
        id: u64,
        result: Result<audio::Generated, UiError>,
    },
    DismissToast(u64),
    ClearToasts,
}

#[derive(Debug, Clone)]
struct UiError {
    scope: Scope,
    title: String,
    detail: String,
}

impl UiError {
    fn new(scope: Scope, title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            scope,
            title: title.into(),
            detail: detail.into(),
        }
    }

    fn from_error(scope: Scope, title: impl Into<String>, error: &(dyn Error + 'static)) -> Self {
        Self::new(scope, title, describe(error))
    }

    fn with_hint(mut self, hint: &str) -> Self {
        self.detail.push('\n');
        self.detail.push_str(hint);
        self
    }
}

fn describe(error: &(dyn Error + 'static)) -> String {
    let mut message = error.to_string();
    let mut source = error.source();

    while let Some(inner) = source {
        let detail = inner.to_string();

        if !message.contains(&detail) {
            message.push_str(": ");
            message.push_str(&detail);
        }

        source = inner.source();
    }

    message
}

fn join_error(scope: Scope, error: JoinError) -> UiError {
    let detail = if error.is_panic() {
        let payload = error.into_panic();
        let message = payload
            .downcast_ref::<&str>()
            .map(|message| (*message).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_owned());

        format!("The worker thread panicked: {message}")
    } else {
        "The background task was cancelled.".to_owned()
    };

    UiError::new(scope, "Background task failed", detail)
}

struct Shared<T>(Arc<T>);

impl<T> Shared<T> {
    fn new(value: T) -> Self {
        Self(Arc::new(value))
    }
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> Deref for Shared<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> fmt::Debug for Shared<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Shared<{}>", type_name::<T>())
    }
}

enum Load<T> {
    Loading,
    Ready(T),
    Failed,
}

struct VoiceBank {
    enumerator: Shared<VoiceEnumerator>,
    entries: Vec<VoiceEntry>,
}

struct ActiveJob {
    voice_name: String,
    path: PathBuf,
}

struct DoneItem {
    id: u64,
    path: PathBuf,
}

struct Playback {
    id: u64,
    stop: Arc<AtomicBool>,
    stopping: bool,
}

struct Toast {
    id: u64,
    error: UiError,
}

struct App {
    started: Instant,
    now: Instant,
    voices: Load<VoiceBank>,
    engine: Load<Shared<Inference>>,
    search: String,
    accent: Option<VoiceAccent>,
    gender: Option<VoiceGender>,
    selected: Option<usize>,
    text: text_editor::Content,
    speed: f32,
    output_dir: String,
    done_dir: PathBuf,
    page: Page,
    queue: VecDeque<audio::SynthesisJob>,
    active: Option<ActiveJob>,
    done: Vec<DoneItem>,
    playback: Option<Playback>,
    next_id: u64,
    busy_files: Vec<u64>,
    status: Option<String>,
    toasts: Vec<Toast>,
    next_toast: u64,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let now = Instant::now();
        let done_dir = audio::default_done_dir();
        let output_dir = done_dir.to_string_lossy().into_owned();

        let mut app = Self {
            started: now,
            now,
            voices: Load::Loading,
            engine: Load::Loading,
            search: String::new(),
            accent: None,
            gender: None,
            selected: None,
            text: text_editor::Content::with_text(DEFAULT_TEXT),
            speed: 1.0,
            output_dir,
            done_dir,
            page: Page::Compose,
            queue: VecDeque::new(),
            active: None,
            done: Vec::new(),
            playback: None,
            next_id: 0,
            busy_files: Vec::new(),
            status: None,
            toasts: Vec::new(),
            next_toast: 0,
        };

        let icon_fonts = Task::batch(iconflow::fonts().iter().map(|asset| {
            font::load(asset.bytes).map(|result| {
                Message::IconFontLoaded(result.map_err(|never| match never {}))
            })
        }));
        let task = Task::batch([app.load_backend(), app.load_done_files(), icon_fonts]);

        (app, task)
    }

    fn theme(&self) -> Theme {
        Theme::TokyoNight
    }

    fn subscription(&self) -> Subscription<Message> {
        time::every(TICK).map(Message::Tick)
    }

    fn load_backend(&mut self) -> Task<Message> {
        self.voices = Load::Loading;
        self.engine = Load::Loading;
        self.selected = None;

        let voices = on_worker(Scope::Voices, async {
            VoiceEnumerator::open()
                .await
                .map(Shared::new)
                .map_err(|error| {
                    UiError::from_error(
                        Scope::Voices,
                        "Couldn't open the voice library",
                        &error,
                    )
                    .with_hint("Check the data folder, then press Reload.")
                })
        });

        let engine = on_worker(Scope::Engine, async {
            Inference::new(Device::flex())
                .await
                .map(Shared::new)
                .map_err(|error| {
                    UiError::from_error(
                        Scope::Engine,
                        "Couldn't load the speech model",
                        &error,
                    )
                    .with_hint("Press Reload to try again.")
                })
        });

        Task::batch([
            Task::perform(voices, Message::VoicesLoaded),
            Task::perform(engine, Message::EngineLoaded),
        ])
    }

    fn load_done_files(&self) -> Task<Message> {
        let directory = self.done_dir.clone();
        let work = on_worker(Scope::Output, async move {
            audio::list_wav_files(&directory).map_err(|error| {
                UiError::from_error(Scope::Output, "Couldn't scan the Done folder", &error)
            })
        });

        Task::perform(work, Message::DoneLoaded)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick(now) => self.now = now,
            Message::VoicesLoaded(Ok(enumerator)) => {
                let entries = enumerator.iter().cloned().collect();
                self.voices = Load::Ready(VoiceBank { enumerator, entries });
            }
            Message::VoicesLoaded(Err(error)) => {
                self.voices = Load::Failed;
                self.push_toast(error);
            }
            Message::EngineLoaded(Ok(engine)) => self.engine = Load::Ready(engine),
            Message::EngineLoaded(Err(error)) => {
                self.engine = Load::Failed;
                self.push_toast(error);
            }
            Message::DoneLoaded(Ok(paths)) => {
                for path in paths {
                    if self.done.iter().any(|item| item.path == path) {
                        continue;
                    }

                    self.next_id += 1;
                    self.done.push(DoneItem {
                        id: self.next_id,
                        path,
                    });
                }
            }
            Message::DoneLoaded(Err(error)) => self.push_toast(error),
            Message::IconFontLoaded(result) => {
                if let Err(error) = result {
                    self.push_toast(UiError::new(
                        Scope::Output,
                        "Icon font failed to load",
                        error,
                    ));
                }
            }
            Message::Reload => {
                if self.active.is_some() || !self.queue.is_empty() {
                    return Task::none();
                }

                self.clear_toasts(&[Scope::Voices, Scope::Engine]);
                return Task::batch([self.load_backend(), self.load_done_files()]);
            }
            Message::Search(value) => self.search = value,
            Message::Accent(value) => self.accent = value,
            Message::Gender(value) => self.gender = value,
            Message::SelectVoice(index) => self.selected = Some(index),
            Message::Editor(action) => self.text.perform(action),
            Message::Speed(value) => self.speed = value,
            Message::OutputDir(value) => self.output_dir = value,
            Message::Enqueue => return self.enqueue(),
            Message::SetPage(page) => self.page = page,
            Message::RemoveQueued(id) => self.queue.retain(|job| job.id != id),
            Message::PlayDone(id) => return self.play_done(id),
            Message::StopPlayback => {
                if let Some(playback) = &mut self.playback {
                    playback.stopping = true;
                    playback.stop.store(true, Ordering::Release);
                }
            }
            Message::PlaybackFinished { id, result } => {
                if self.playback.as_ref().is_some_and(|playback| playback.id == id) {
                    self.playback = None;
                }

                if let Err(error) = result {
                    self.push_toast(error);
                }
            }
            Message::OpenDoneFolder(id) => {
                let Some(item) = self.done.iter().find(|item| item.id == id) else {
                    return Task::none();
                };
                let path = item.path.clone();
                let work = on_worker(Scope::Output, async move {
                    audio::open_parent_folder(&path).map_err(|error| {
                        UiError::from_error(
                            Scope::Output,
                            "Couldn't open the output folder",
                            &error,
                        )
                    })
                });

                return Task::perform(work, Message::OutputActionFinished);
            }
            Message::OutputActionFinished(result) => {
                if let Err(error) = result {
                    self.push_toast(error);
                }
            }
            Message::DeleteDone(id) => {
                if self.busy_files.contains(&id)
                    || self.playback.as_ref().is_some_and(|playback| playback.id == id)
                {
                    return Task::none();
                }

                let Some(item) = self.done.iter().find(|item| item.id == id) else {
                    return Task::none();
                };
                let path = item.path.clone();
                self.busy_files.push(id);

                let work = on_worker(Scope::Output, async move {
                    audio::delete_file(&path).map_err(|error| {
                        UiError::from_error(
                            Scope::Output,
                            "Couldn't delete the audio file",
                            &error,
                        )
                    })
                });

                return Task::perform(work, move |result| {
                    Message::DeleteDoneFinished { id, result }
                });
            }
            Message::DeleteDoneFinished { id, result } => {
                self.busy_files.retain(|busy_id| *busy_id != id);

                match result {
                    Ok(()) => self.done.retain(|item| item.id != id),
                    Err(error) => self.push_toast(error),
                }
            }
            Message::QueueFinished { id, result } => {
                self.active = None;

                match result {
                    Ok(done) => {
                        self.done.push(DoneItem {
                            id,
                            path: done.path.clone(),
                        });
                        let name = done
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy();
                        self.status = Some(format!(
                            "Finished {name} · {:.1}s audio · {:.1}s to generate",
                            done.seconds,
                            done.elapsed.as_secs_f32(),
                        ));
                    }
                    Err(error) => self.push_toast(error),
                }

                return self.start_next();
            }
            Message::DismissToast(id) => self.toasts.retain(|toast| toast.id != id),
            Message::ClearToasts => self.toasts.clear(),
        }

        Task::none()
    }

    fn play_done(&mut self, id: u64) -> Task<Message> {
        if self.playback.is_some() {
            return Task::none();
        }

        let Some(item) = self.done.iter().find(|item| item.id == id) else {
            return Task::none();
        };

        let path = item.path.clone();
        let stop = Arc::new(AtomicBool::new(false));

        self.playback = Some(Playback {
            id,
            stop: Arc::clone(&stop),
            stopping: false,
        });

        let work = on_worker(Scope::Output, async move {
            match tokio::task::spawn_blocking(move || playback::play(&path, stop)).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(error)) => Err(UiError::from_error(
                    Scope::Output,
                    "Couldn't play the audio",
                    &error,
                )),
                Err(error) => Err(join_error(Scope::Output, error)),
            }
        });

        Task::perform(work, move |result| Message::PlaybackFinished { id, result })
    }

    fn enqueue(&mut self) -> Task<Message> {
        self.clear_toasts(&[Scope::Input, Scope::Synthesis, Scope::Output]);
        self.status = None;

        let text = self.text.text().trim().to_owned();
        let output_dir = PathBuf::from(self.output_dir.trim());
        let mut problems = Vec::new();

        if text.is_empty() {
            problems.push(UiError::new(
                Scope::Input,
                "Add some text first",
                "Type or paste what the voice should say.",
            ));
        }

        if self.output_dir.trim().is_empty() {
            problems.push(UiError::new(
                Scope::Input,
                "Choose an output folder",
                "Enter a folder for generated WAV files.",
            ));
        }

        let voice = match &self.voices {
            Load::Ready(bank) => match self.selected.and_then(|index| bank.entries.get(index)) {
                Some(entry) => Some((bank.enumerator.clone(), entry.clone())),
                None => {
                    problems.push(UiError::new(
                        Scope::Input,
                        "Pick a voice",
                        "Choose a voice from the list on the left.",
                    ));
                    None
                }
            },
            Load::Loading => {
                problems.push(UiError::new(
                    Scope::Input,
                    "Voices are still loading",
                    "Try again in a moment.",
                ));
                None
            }
            Load::Failed => {
                problems.push(UiError::new(
                    Scope::Input,
                    "No voices available",
                    "The voice library failed to load. Press Reload to try again.",
                ));
                None
            }
        };

        let engine = match &self.engine {
            Load::Ready(engine) => Some(engine.clone()),
            Load::Loading => {
                problems.push(UiError::new(
                    Scope::Input,
                    "The model is still loading",
                    "Try again in a moment.",
                ));
                None
            }
            Load::Failed => {
                problems.push(UiError::new(
                    Scope::Input,
                    "The model isn't loaded",
                    "Loading it failed earlier. Press Reload to try again.",
                ));
                None
            }
        };

        if !problems.is_empty() {
            for problem in problems {
                self.push_toast(problem);
            }
            return Task::none();
        }

        let (Some(engine), Some((voices, entry))) = (engine, voice) else {
            return Task::none();
        };

        self.next_id += 1;
        let path = audio::timestamped_wav_path(&output_dir);
        let job = audio::SynthesisJob {
            id: self.next_id,
            engine,
            voices,
            entry: entry.clone(),
            voice_name: entry.name.clone(),
            text,
            speed: self.speed,
            path,
        };

        self.queue.push_back(job);
        self.page = Page::Queue;
        self.status = Some(format!("Added {} to the queue", entry.name));

        self.start_next()
    }

    fn start_next(&mut self) -> Task<Message> {
        if self.active.is_some() {
            return Task::none();
        }

        let Some(job) = self.queue.pop_front() else {
            return Task::none();
        };
        let active = ActiveJob {
            voice_name: job.voice_name.clone(),
            path: job.path.clone(),
        };
        let id = job.id;
        self.active = Some(active);

        let work = on_worker(Scope::Synthesis, audio::run_job(job));
        Task::perform(work, move |result| Message::QueueFinished { id, result })
    }

    fn push_toast(&mut self, error: UiError) {
        self.next_toast += 1;
        self.toasts.push(Toast {
            id: self.next_toast,
            error,
        });

        if self.toasts.len() > MAX_TOASTS {
            self.toasts.remove(0);
        }
    }

    fn clear_toasts(&mut self, scopes: &[Scope]) {
        self.toasts
            .retain(|toast| !scopes.contains(&toast.error.scope));
    }

    fn is_loading(&self) -> bool {
        matches!(self.voices, Load::Loading) || matches!(self.engine, Load::Loading)
    }

    fn selected_entry(&self) -> Option<&VoiceEntry> {
        match &self.voices {
            Load::Ready(bank) => self.selected.and_then(|index| bank.entries.get(index)),
            _ => None,
        }
    }

    fn matches(&self, entry: &VoiceEntry, query: &str) -> bool {
        let (accent, gender) = voices::traits(entry);

        self.accent.is_none_or(|wanted| wanted == accent)
            && self.gender.is_none_or(|wanted| wanted == gender)
            && (query.is_empty()
                || entry.name.to_lowercase().contains(query)
                || entry.id.to_lowercase().contains(query))
    }

    fn view(&self) -> Element<'_, Message> {
        let elapsed = self.now.duration_since(self.started).as_secs_f32();
        let engine_status = if self.active.is_some() {
            "Synthesizing"
        } else if !self.queue.is_empty() {
            "Queue pending"
        } else if matches!(self.engine, Load::Ready(_)) {
            "Engine ready"
        } else if self.is_loading() {
            "Loading engine"
        } else {
            "Engine unavailable"
        };

        let top = row![
            row![
                text(TITLE).size(TITLE_SIZE),
                compose::thinking_dots(elapsed),
            ]
            .align_y(Vertical::Center),
            container(
                row![
                    text(engine_status).size(13).style(theme::muted),
                    button(text("Reload").size(13))
                        .height(36.0)
                        .padding([6, 13])
                        .style(theme::secondary_button)
                        .on_press_maybe(
                            (self.active.is_none()
                                && self.queue.is_empty()
                                && !self.is_loading())
                            .then_some(Message::Reload),
                        ),
                ]
                .spacing(12)
                .align_y(Vertical::Center),
            )
            .width(Fill)
            .align_x(Horizontal::Right),
        ]
        .spacing(20)
        .align_y(Vertical::Center);

        let queued_count = self.queue.len() + usize::from(self.active.is_some());
        let tabs = tabs::view(self.page, queued_count, self.done.len());
        let content = match self.page {
            Page::Compose => compose::panel(self),
            Page::Queue => queue::page(self),
            Page::Done => done::page(self),
        };
        let workspace = column![tabs, content]
            .spacing(12)
            .width(Fill)
            .height(Fill);
        let body = row![voices::panel(self), workspace]
            .spacing(16)
            .height(Fill);

        let page = container(column![top, body].spacing(18).padding(20))
            .width(Fill)
            .height(Fill)
            .style(theme::app_background);

        if self.toasts.is_empty() {
            page.into()
        } else {
            stack![page, toast::layer(&self.toasts)].into()
        }
    }
}

fn accent_label(accent: VoiceAccent) -> &'static str {
    match accent {
        VoiceAccent::EnglishAmerican => "American",
        VoiceAccent::EnglishBritish => "British",
    }
}

fn gender_label(gender: VoiceGender) -> &'static str {
    match gender {
        VoiceGender::Female => "Female",
        VoiceGender::Male => "Male",
    }
}

fn label<'a>(value: &'a str) -> Element<'a, Message> {
    text(value)
        .size(13)
        .font(Font {
            weight: Weight::Semibold,
            ..Font::DEFAULT
        })
        .into()
}
