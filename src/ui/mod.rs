use std::{
    error::Error,
    f32::consts::TAU,
    fmt,
    fs::File,
    future::Future,
    io::{self, BufWriter, Write},
    ops::Deref,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use iced::{
    Background, Border, Color, Element, Fill, Font, Padding, Shadow, Subscription, Task, Theme,
    Vector,
    alignment::{Horizontal, Vertical},
    font::Weight,
    widget::{
        Column, button, column, container, row, scrollable, slider, stack, text, text_editor,
        text_input,
    },
};
use tokio::{
    runtime::{Builder, Runtime},
    task::{self, JoinError},
};

use hmm::{
    Audio, Inference, KokoroOptions, Options, VoiceAccent, VoiceEntry, VoiceEnumerator,
    VoiceGender, VoiceModel,
};

const TITLE: &str = "HMM";
const TITLE_SIZE: f32 = 30.0;

const DEFAULT_TEXT: &str = "Testing text to speech with Burn!";
const DEFAULT_OUTPUT: &str = "sample.wav";

const WORKER_STACK_BYTES: usize = 32 * 1024 * 1024;

const VOICES_PANEL_WIDTH: f32 = 380.0;
const TOAST_WIDTH: f32 = 360.0;
const MAX_TOASTS: usize = 6;

const TICK: Duration = Duration::from_millis(40);
const DOT_COUNT: usize = 3;
const DOT_PERIOD: f32 = 1.2;
const DOT_STAGGER: f32 = 0.14;
const DOT_LIFT: f32 = 8.0;
const ACCENTS: [VoiceAccent; 2] = [VoiceAccent::EnglishAmerican, VoiceAccent::EnglishBritish];
const GENDERS: [VoiceGender; 2] = [VoiceGender::Female, VoiceGender::Male];

pub fn run() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title(TITLE)
        .theme(App::theme)
        .subscription(App::subscription)
        .window_size((1120.0, 720.0))
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
    match worker().spawn(work).await {
        Ok(result) => result,
        Err(error) => Err(join_error(scope, error)),
    }
}

#[derive(Debug, Clone)]
enum Message {
    Tick(Instant),

    VoicesLoaded(Result<Shared<VoiceEnumerator>, UiError>),
    EngineLoaded(Result<Shared<Inference>, UiError>),
    Reload,

    Search(String),
    Accent(Option<VoiceAccent>),
    Gender(Option<VoiceGender>),
    SelectVoice(usize),

    Editor(text_editor::Action),
    Speed(f32),
    OutputPath(String),
    Generate,
    Generated(Result<Generated, UiError>),

    DismissToast(u64),
    ClearToasts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Voices,
    Engine,
    Input,
    Synthesis,
    Output,
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
        self.detail = format!("{}\n{hint}", self.detail);
        self
    }
}

fn describe(error: &(dyn Error + 'static)) -> String {
    let mut message = error.to_string();
    let mut source = error.source();

    while let Some(inner) = source {
        let inner_text = inner.to_string();

        if !message.contains(&inner_text) {
            message.push_str(": ");
            message.push_str(&inner_text);
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

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Shared<{}>", std::any::type_name::<T>())
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
    output: String,

    busy: bool,
    status: Option<String>,

    toasts: Vec<Toast>,
    next_toast: u64,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let now = Instant::now();

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
            output: DEFAULT_OUTPUT.to_owned(),
            busy: false,
            status: None,
            toasts: Vec::new(),
            next_toast: 0,
        };

        let task = app.load_backend();

        (app, task)
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn subscription(&self) -> Subscription<Message> {
        iced::time::every(TICK).map(Message::Tick)
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
                    UiError::from_error(Scope::Voices, "Couldn't open the voice library", &error)
                        .with_hint("Check the data folder, then press Reload.")
                })
        });

        let engine = on_worker(Scope::Engine, async {
            let device = burn::tensor::Device::flex();

            Inference::new(device)
                .await
                .map(Shared::new)
                .map_err(|error| {
                    UiError::from_error(Scope::Engine, "Couldn't load the speech model", &error)
                        .with_hint("Press Reload to try again.")
                })
        });

        Task::batch([
            Task::perform(voices, Message::VoicesLoaded),
            Task::perform(engine, Message::EngineLoaded),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick(now) => self.now = now,

            Message::VoicesLoaded(Ok(enumerator)) => {
                let entries = enumerator.iter().cloned().collect();

                self.voices = Load::Ready(VoiceBank {
                    enumerator,
                    entries,
                });
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
            Message::Reload => {
                if self.busy {
                    return Task::none();
                }

                self.clear_toasts(&[Scope::Voices, Scope::Engine]);

                return self.load_backend();
            }

            Message::Search(search) => self.search = search,
            Message::Accent(accent) => self.accent = accent,
            Message::Gender(gender) => self.gender = gender,
            Message::SelectVoice(index) => self.selected = Some(index),

            Message::Editor(action) => self.text.perform(action),
            Message::Speed(speed) => self.speed = speed,
            Message::OutputPath(path) => self.output = path,
            Message::Generate => return self.generate(),
            Message::Generated(result) => {
                self.busy = false;

                match result {
                    Ok(done) => {
                        self.status = Some(format!(
                            "Saved {} ({:.1} s of audio, generated in {:.1} s)",
                            done.path.display(),
                            done.seconds,
                            done.elapsed.as_secs_f32(),
                        ));
                    }
                    Err(error) => {
                        self.status = None;
                        self.push_toast(error);
                    }
                }
            }

            Message::DismissToast(id) => self.toasts.retain(|toast| toast.id != id),
            Message::ClearToasts => self.toasts.clear(),
        }

        Task::none()
    }

    fn generate(&mut self) -> Task<Message> {
        if self.busy {
            return Task::none();
        }

        self.clear_toasts(&[Scope::Input, Scope::Synthesis, Scope::Output]);
        self.status = None;

        let script = self.text.text().trim().to_owned();
        let output = self.output.trim().to_owned();
        let mut problems = Vec::new();

        if script.is_empty() {
            problems.push(UiError::new(
                Scope::Input,
                "Add some text first",
                "Type or paste what the voice should say.",
            ));
        }

        if output.is_empty() {
            problems.push(UiError::new(
                Scope::Input,
                "Choose where to save",
                "Enter a file name for the WAV output.",
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

        let mut path = PathBuf::from(output);

        if path.extension().is_none() {
            path.set_extension("wav");
        }

        self.busy = true;
        self.status = Some(format!("Generating with {}…", entry.name));

        let job = Job {
            engine,
            voices,
            entry,
            text: script,
            speed: self.speed,
            path,
        };

        Task::perform(
            on_worker(Scope::Synthesis, run_job(job)),
            Message::Generated,
        )
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
        let (accent, gender) = traits(entry);

        self.accent.is_none_or(|wanted| wanted == accent)
            && self.gender.is_none_or(|wanted| wanted == gender)
            && (query.is_empty()
                || entry.name.to_lowercase().contains(query)
                || entry.id.to_lowercase().contains(query))
    }
}

struct Job {
    engine: Shared<Inference>,
    voices: Shared<VoiceEnumerator>,
    entry: VoiceEntry,
    text: String,
    speed: f32,
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct Generated {
    path: PathBuf,
    seconds: f32,
    elapsed: Duration,
}

async fn run_job(job: Job) -> Result<Generated, UiError> {
    let started = Instant::now();

    let voice = job.voices.load(&job.entry).await.map_err(|error| {
        UiError::from_error(Scope::Synthesis, "Couldn't load the voice", &error)
    })?;

    let options = match &job.entry.model {
        VoiceModel::Kokoro { .. } => Options::Kokoro(KokoroOptions { speed: job.speed }),
    };

    let audio = job
        .engine
        .synthesize(job.text.as_str(), voice, options)
        .await
        .map_err(|error| {
            UiError::from_error(Scope::Synthesis, "Speech synthesis failed", &error)
        })?;

    let seconds = audio.samples.len() as f32
        / (f32::from(audio.channels.max(1)) * audio.sample_rate.max(1) as f32);

    let path = job.path.clone();

    match task::spawn_blocking(move || write_wav(&path, &audio)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            return Err(UiError::new(
                Scope::Output,
                "Couldn't save the audio",
                format!(
                    "{}: {}\nCheck that the folder exists and that you can write to it.",
                    job.path.display(),
                    describe(&error),
                ),
            ));
        }
        Err(error) => return Err(join_error(Scope::Output, error)),
    }

    Ok(Generated {
        path: job.path,
        seconds,
        elapsed: started.elapsed(),
    })
}

fn write_wav(path: &Path, audio: &Audio) -> io::Result<()> {
    const BYTES_PER_SAMPLE: u16 = 2;

    let too_large = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "audio is too large for a WAV file",
        )
    };

    let block_align = audio
        .channels
        .checked_mul(BYTES_PER_SAMPLE)
        .ok_or_else(too_large)?;
    let byte_rate = audio
        .sample_rate
        .checked_mul(u32::from(block_align))
        .ok_or_else(too_large)?;

    let data_len = audio
        .samples
        .len()
        .checked_mul(usize::from(BYTES_PER_SAMPLE))
        .ok_or_else(too_large)?;

    let data_len = u32::try_from(data_len).map_err(|_| too_large())?;
    let riff_len = 36u32.checked_add(data_len).ok_or_else(too_large)?;

    let mut file = BufWriter::new(File::create(path)?);

    file.write_all(b"RIFF")?;
    file.write_all(&riff_len.to_le_bytes())?;
    file.write_all(b"WAVE")?;

    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&audio.channels.to_le_bytes())?;
    file.write_all(&audio.sample_rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;

    file.write_all(b"data")?;
    file.write_all(&data_len.to_le_bytes())?;

    for sample in &audio.samples {
        let sample = if sample.is_finite() {
            sample.clamp(-1.0, 1.0)
        } else {
            0.0
        };

        let pcm = if sample <= -1.0 {
            i16::MIN
        } else {
            (sample * f32::from(i16::MAX)) as i16
        };

        file.write_all(&pcm.to_le_bytes())?;
    }

    file.flush()
}

fn traits(entry: &VoiceEntry) -> (VoiceAccent, VoiceGender) {
    match &entry.model {
        VoiceModel::Kokoro { accent, gender, .. } => (*accent, *gender),
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

fn visible_voices<'a>(app: &App, bank: &'a VoiceBank, query: &str) -> Vec<(usize, &'a VoiceEntry)> {
    bank.entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| app.matches(entry, query))
        .collect()
}

impl App {
    fn view(&self) -> Element<'_, Message> {
        let page = column![
            header(self),
            row![voices_panel(self), compose_panel(self)]
                .spacing(16)
                .height(Fill),
        ]
        .spacing(16)
        .padding(20);

        if self.toasts.is_empty() {
            page.into()
        } else {
            stack![page, toasts_layer(&self.toasts)].into()
        }
    }
}

fn header(app: &App) -> Element<'_, Message> {
    let elapsed = app.now.duration_since(app.started).as_secs_f32();

    let controls = row![
        engine_status(&app.engine),
        reload_button(!app.busy && !app.is_loading()),
    ]
    .spacing(12)
    .align_y(Vertical::Center);

    row![
        app_title(elapsed),
        container(controls).width(Fill).align_x(Horizontal::Right),
    ]
    .align_y(Vertical::Center)
    .into()
}

fn app_title(elapsed: f32) -> Element<'static, Message> {
    row![text(TITLE).size(TITLE_SIZE), thinking_dots(elapsed)]
        .align_y(Vertical::Center)
        .into()
}

fn thinking_dots(elapsed: f32) -> Element<'static, Message> {
    let dots = (0..DOT_COUNT).map(|index| -> Element<'static, Message> {
        let phase = (elapsed / DOT_PERIOD - index as f32 * DOT_STAGGER).rem_euclid(1.0);

        let lift = (phase * TAU).sin().max(0.0) * DOT_LIFT;

        container(text(".").size(TITLE_SIZE).style(text::primary))
            .height(TITLE_SIZE * 1.3)
            .padding(Padding {
                top: DOT_LIFT - lift,
                ..Padding::ZERO
            })
            .into()
    });

    row(dots).into()
}

fn engine_status(engine: &Load<Shared<Inference>>) -> Element<'static, Message> {
    match engine {
        Load::Loading => text("Loading engine…").size(13).style(muted).into(),
        Load::Ready(_) => text("Engine ready on flex").size(13).style(muted).into(),
        Load::Failed => text("Engine failed to load")
            .size(13)
            .style(text::danger)
            .into(),
    }
}

fn reload_button(enabled: bool) -> Element<'static, Message> {
    button(text("Reload").size(13))
        .padding([6, 14])
        .style(button::secondary)
        .on_press_maybe(enabled.then_some(Message::Reload))
        .into()
}

fn voices_panel(app: &App) -> Element<'_, Message> {
    let query = app.search.trim().to_lowercase();

    let (count, body): (Option<String>, Element<'_, Message>) = match &app.voices {
        Load::Loading => (None, centered("Loading voices…")),
        Load::Failed => (
            None,
            centered("The voice library didn't load. Press Reload to try again."),
        ),
        Load::Ready(bank) => {
            let visible = visible_voices(app, bank, &query);
            let count = format!("{} of {}", visible.len(), bank.entries.len());

            let body = if visible.is_empty() {
                centered("No voices match. Clear the search or filters.")
            } else {
                voice_list(&visible, app.selected)
            };

            (Some(count), body)
        }
    };

    container(
        column![
            voices_heading(count),
            search_box(&app.search),
            accent_filter(app.accent),
            gender_filter(app.gender),
            body,
        ]
        .spacing(12),
    )
    .padding(16)
    .width(VOICES_PANEL_WIDTH)
    .height(Fill)
    .style(panel_style)
    .into()
}

fn voices_heading(count: Option<String>) -> Element<'static, Message> {
    let mut heading = row![text("Voices").size(22)]
        .spacing(10)
        .align_y(Vertical::Center);

    if let Some(count) = count {
        heading = heading.push(text(count).size(13).style(muted));
    }

    heading.into()
}

fn search_box(search: &str) -> Element<'_, Message> {
    text_input("Search by name or id", search)
        .on_input(Message::Search)
        .size(16)
        .padding(12)
        .into()
}

fn accent_filter(selected: Option<VoiceAccent>) -> Element<'static, Message> {
    let mut filter = row![
        label("Accent"),
        chip("All", selected.is_none(), Message::Accent(None)),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    for accent in ACCENTS {
        filter = filter.push(chip(
            accent_label(accent),
            selected == Some(accent),
            Message::Accent(Some(accent)),
        ));
    }

    filter.into()
}

fn gender_filter(selected: Option<VoiceGender>) -> Element<'static, Message> {
    let mut filter = row![
        label("Gender"),
        chip("All", selected.is_none(), Message::Gender(None)),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    for gender in GENDERS {
        filter = filter.push(chip(
            gender_label(gender),
            selected == Some(gender),
            Message::Gender(Some(gender)),
        ));
    }

    filter.into()
}

fn voice_list<'a>(
    voices: &[(usize, &'a VoiceEntry)],
    selected: Option<usize>,
) -> Element<'a, Message> {
    let rows: Vec<Element<'a, Message>> = voices
        .iter()
        .map(|&(index, entry)| voice_row(index, entry, selected == Some(index)))
        .collect();

    scrollable(container(column(rows).spacing(2)).padding(Padding {
        right: 10.0,
        ..Padding::ZERO
    }))
    .width(Fill)
    .height(Fill)
    .into()
}

fn voice_row(index: usize, entry: &VoiceEntry, selected: bool) -> Element<'_, Message> {
    let (accent, gender) = traits(entry);

    let names = column![
        text(entry.name.as_str()).size(15),
        text(entry.id.as_str()).size(12).style(muted),
    ];

    let tags = row![pill(accent_label(accent)), pill(gender_label(gender))].spacing(6);

    let content = row![monogram(entry, 30.0), container(names).width(Fill), tags,]
        .spacing(10)
        .align_y(Vertical::Center);

    button(content)
        .padding([8, 10])
        .width(Fill)
        .style(move |theme, status| voice_row_style(theme, status, selected))
        .on_press(Message::SelectVoice(index))
        .into()
}

fn compose_panel(app: &App) -> Element<'_, Message> {
    let entry = app.selected_entry();
    let characters = app.text.text().trim().chars().count();

    let mut form = column![
        label("Text"),
        script_editor(&app.text),
        character_count(characters),
        label("Voice"),
        voice_summary(entry),
        model_options(entry, app.speed),
        label("Save to"),
        output_field(&app.output),
        generate_button(app.busy),
    ]
    .spacing(10);

    if let Some(status) = &app.status {
        form = form.push(status_line(status));
    }

    container(form)
        .padding(16)
        .width(Fill)
        .height(Fill)
        .style(panel_style)
        .into()
}

fn script_editor(content: &text_editor::Content) -> Element<'_, Message> {
    text_editor(content)
        .placeholder("Type or paste what the voice should say")
        .on_action(Message::Editor)
        .size(18.0)
        .padding(14)
        .height(Fill)
        .into()
}

fn character_count(count: usize) -> Element<'static, Message> {
    text(format!("{count} characters"))
        .size(12)
        .style(muted)
        .into()
}

fn voice_summary(entry: Option<&VoiceEntry>) -> Element<'_, Message> {
    match entry {
        Some(entry) => row![
            monogram(entry, 34.0),
            column![
                text(entry.name.as_str()).size(15),
                text(entry.id.as_str()).size(12).style(muted),
            ]
        ]
        .spacing(10)
        .align_y(Vertical::Center)
        .into(),
        None => text("No voice selected").size(14).style(muted).into(),
    }
}

fn model_options(entry: Option<&VoiceEntry>, speed: f32) -> Element<'static, Message> {
    match entry.map(|entry| &entry.model) {
        Some(VoiceModel::Kokoro { .. }) => kokoro_options(speed),
        None => text("Model options appear once you pick a voice.")
            .size(13)
            .style(muted)
            .into(),
    }
}

fn kokoro_options(speed: f32) -> Element<'static, Message> {
    column![
        row![
            label("Speed"),
            container(text(format!("{speed:.2}×")).size(13))
                .width(Fill)
                .align_x(Horizontal::Right),
        ],
        slider(0.5..=2.0, speed, Message::Speed).step(0.05),
    ]
    .spacing(6)
    .into()
}

fn output_field(path: &str) -> Element<'_, Message> {
    text_input(DEFAULT_OUTPUT, path)
        .on_input(Message::OutputPath)
        .size(15)
        .padding(12)
        .into()
}

fn generate_button(busy: bool) -> Element<'static, Message> {
    let caption = if busy {
        "Generating…"
    } else {
        "Generate speech"
    };

    button(
        container(text(caption).size(16))
            .width(Fill)
            .align_x(Horizontal::Center),
    )
    .padding(12)
    .width(Fill)
    .style(button::primary)
    .on_press_maybe((!busy).then_some(Message::Generate))
    .into()
}

fn status_line(status: &str) -> Element<'_, Message> {
    text(status).size(13).style(muted).into()
}

fn toasts_layer(toasts: &[Toast]) -> Element<'_, Message> {
    let mut list = Column::new().spacing(8).align_x(Horizontal::Right);

    if toasts.len() > 1 {
        list = list.push(clear_all_button());
    }

    list = list.extend(toasts.iter().map(toast_view));

    container(list)
        .width(Fill)
        .height(Fill)
        .align_x(Horizontal::Right)
        .align_y(Vertical::Bottom)
        .padding(20)
        .into()
}

fn clear_all_button() -> Element<'static, Message> {
    button(text("Clear all").size(13))
        .padding([4, 12])
        .style(button::secondary)
        .on_press(Message::ClearToasts)
        .into()
}

fn toast_view(toast: &Toast) -> Element<'_, Message> {
    let content = column![
        text(toast.error.title.as_str())
            .size(15)
            .style(text::danger),
        text(toast.error.detail.as_str()).size(13),
    ]
    .spacing(4)
    .width(Fill);

    let close = button(text("×").size(16))
        .padding([0, 8])
        .style(button::text)
        .on_press(Message::DismissToast(toast.id));

    container(row![content, close].spacing(8))
        .width(TOAST_WIDTH)
        .padding(12)
        .style(toast_style)
        .into()
}

fn monogram<'a>(entry: &VoiceEntry, size: f32) -> Element<'a, Message> {
    let (_, gender) = traits(entry);

    let initial = entry
        .name
        .chars()
        .next()
        .map(|letter| letter.to_uppercase().collect::<String>())
        .unwrap_or_default();

    container(text(initial).size(size * 0.5))
        .center(size)
        .style(move |theme: &Theme| {
            let palette = theme.extended_palette();

            let pair = match gender {
                VoiceGender::Female => palette.primary.strong,
                VoiceGender::Male => palette.success.strong,
            };

            container::Style {
                background: Some(Background::Color(pair.color)),
                text_color: Some(pair.text),
                border: Border {
                    radius: (size / 2.0).into(),
                    ..Border::default()
                },
                ..container::Style::default()
            }
        })
        .into()
}

fn pill<'a>(content: &'a str) -> Element<'a, Message> {
    container(text(content).size(12))
        .padding([2, 8])
        .style(|theme: &Theme| {
            let text_color = theme.palette().text;

            container::Style {
                text_color: Some(text_color.scale_alpha(0.8)),
                border: Border {
                    radius: 8.0.into(),
                    width: 1.0,
                    color: text_color.scale_alpha(0.3),
                },
                ..container::Style::default()
            }
        })
        .into()
}

fn chip<'a>(content: &'a str, active: bool, message: Message) -> Element<'a, Message> {
    button(text(content).size(13))
        .padding([4, 12])
        .style(move |theme, status| {
            if active {
                button::primary(theme, status)
            } else {
                button::secondary(theme, status)
            }
        })
        .on_press(message)
        .into()
}

fn label<'a>(content: &'a str) -> Element<'a, Message> {
    text(content)
        .size(14)
        .font(Font {
            weight: Weight::Semibold,
            ..Font::DEFAULT
        })
        .into()
}

fn centered<'a>(content: &'a str) -> Element<'a, Message> {
    container(text(content).style(muted)).center(Fill).into()
}

fn muted(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.palette().text.scale_alpha(0.6)),
    }
}

fn panel_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(Background::Color(palette.background.weakest.color)),
        border: Border {
            radius: 12.0.into(),
            width: 1.0,
            color: palette.background.weak.color,
        },
        ..container::Style::default()
    }
}

fn toast_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        text_color: Some(palette.background.weak.text),
        border: Border {
            radius: 10.0.into(),
            width: 1.0,
            color: palette.danger.base.color,
        },
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(0.4),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 18.0,
        },
        ..container::Style::default()
    }
}

fn voice_row_style(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    let palette = theme.extended_palette();

    let (background, text_color) = if selected {
        (Some(palette.primary.weak.color), palette.primary.weak.text)
    } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
        (
            Some(palette.background.weak.color),
            palette.background.weak.text,
        )
    } else {
        (None, palette.background.base.text)
    };

    button::Style {
        background: background.map(Background::Color),
        text_color,
        border: Border {
            radius: 8.0.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}
