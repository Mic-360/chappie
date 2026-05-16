use rand::Rng;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::collections::VecDeque;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

macro_rules! logln {
    ($($arg:tt)*) => {{ eprintln!("[chappie] {}", format!($($arg)*)); }};
}

struct SoundAsset {
    cache_name: &'static str,
    url: &'static str,
}

const SOUND_ASSETS: [SoundAsset; 6] = [
    SoundAsset {
        cache_name: "click1.wav",
        url: "https://raw.githubusercontent.com/Nigh/OpenClickSound/master/Sound/01/kt01-click-02-01.wav",
    },
    SoundAsset {
        cache_name: "click2.wav",
        url: "https://raw.githubusercontent.com/Nigh/OpenClickSound/master/Sound/01/kt01-click-03-01.wav",
    },
    SoundAsset {
        cache_name: "click3.wav",
        url: "https://raw.githubusercontent.com/Nigh/OpenClickSound/master/Sound/01/kt01-click-05-01.wav",
    },
    SoundAsset {
        cache_name: "click4.wav",
        url: "https://raw.githubusercontent.com/Nigh/OpenClickSound/master/Sound/01/kt01-click-02-03.wav",
    },
    SoundAsset {
        cache_name: "spacebar.wav",
        url: "https://raw.githubusercontent.com/Nigh/OpenClickSound/master/Sound/01/kt01-bottom-00-01.wav",
    },
    SoundAsset {
        cache_name: "alert.wav",
        url: "https://raw.githubusercontent.com/akx/Notifications/master/WAV/Calm.wav",
    },
];

const CLICK_COUNT: usize = 4;
const SPACE_IDX: usize = 4;
const ALERT_IDX: usize = 5;

const SIGNAL_POLL_MS: u64 = 40;
const IDLE_POLL_MS: u64 = 200;
const TYPING_TIMEOUT_MS: u64 = 120_000;
const IDLE_SHUTDOWN_MS: u64 = 15_000;
const ALERT_REPEAT_MS: u64 = 3000;

const MIN_PITCH: f32 = 0.92;
const MAX_PITCH: f32 = 1.08;
const MIN_VOLUME: f32 = 0.62;
const MAX_VOLUME: f32 = 1.0;

const WORD_LEN_MIN: u32 = 2;
const WORD_LEN_MAX: u32 = 9;
const ROLL_CHANCE: f64 = 0.22;
const ROLL_LEN_MIN: u32 = 2;
const ROLL_LEN_MAX: u32 = 4;
const ROLL_MS_MIN: u64 = 28;
const ROLL_MS_MAX: u64 = 46;
const TYPO_CHANCE: f64 = 0.06;
const BACKSPACE_MIN: u32 = 2;
const BACKSPACE_MAX: u32 = 5;
const BACKSPACE_MS_MIN: u64 = 38;
const BACKSPACE_MS_MAX: u64 = 58;
const SENTENCE_WORDS_MIN: u32 = 8;
const SENTENCE_WORDS_MAX: u32 = 16;
const SENTENCE_PAUSE_MS_MIN: u64 = 340;
const SENTENCE_PAUSE_MS_MAX: u64 = 680;
const THINK_PAUSE_MS_MIN: u64 = 420;
const THINK_PAUSE_MS_MAX: u64 = 1100;
const FLOW_RUN_MIN: u32 = 10;
const FLOW_RUN_MAX: u32 = 30;

#[derive(Clone, Copy)]
enum Flow {
    Sprint,
    Cruise,
    Deliberate,
}

impl Flow {
    fn params(self) -> (u64, u64, u64, u64, f64) {
        match self {
            Flow::Sprint => (45, 78, 95, 175, 0.03),
            Flow::Cruise => (62, 112, 135, 265, 0.08),
            Flow::Deliberate => (95, 175, 210, 410, 0.22),
        }
    }

    fn pick(rng: &mut impl Rng) -> Flow {
        match rng.gen_range(0..100u32) {
            0..=34 => Flow::Sprint,
            35..=89 => Flow::Cruise,
            _ => Flow::Deliberate,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum KeyKind {
    Click,
    Space,
    Backspace,
    Alert,
}

struct KeyEvent {
    kind: KeyKind,
    gap_ms: u64,
}

fn accel(idx: u32, word_len: u32) -> f32 {
    if word_len <= 1 {
        return 1.0;
    }
    let t = idx as f32 / (word_len - 1) as f32;
    (1.35 - 1.6 * t + 1.1 * t * t).max(0.6)
}

struct TypingRhythm {
    flow: Flow,
    words_left_in_run: u32,
    keys_left: u32,
    word_len: u32,
    roll_keys_left: u32,
    words_since_think: u32,
    words_since_sentence: u32,
    sentence_target: u32,
    typo_done: bool,
    queue: VecDeque<KeyEvent>,
}

impl TypingRhythm {
    fn new(rng: &mut impl Rng) -> Self {
        let mut r = TypingRhythm {
            flow: Flow::Cruise,
            words_left_in_run: 0,
            keys_left: 0,
            word_len: 0,
            roll_keys_left: 0,
            words_since_think: 0,
            words_since_sentence: 0,
            sentence_target: rng.gen_range(SENTENCE_WORDS_MIN..=SENTENCE_WORDS_MAX),
            typo_done: false,
            queue: VecDeque::new(),
        };
        r.start_word(rng);
        r
    }

    fn reset(&mut self, rng: &mut impl Rng) {
        self.queue.clear();
        self.words_left_in_run = 0;
        self.words_since_think = 0;
        self.words_since_sentence = 0;
        self.sentence_target = rng.gen_range(SENTENCE_WORDS_MIN..=SENTENCE_WORDS_MAX);
        self.typo_done = false;
        self.start_word(rng);
    }

    fn start_word(&mut self, rng: &mut impl Rng) {
        if self.words_left_in_run == 0 {
            self.flow = Flow::pick(rng);
            self.words_left_in_run = rng.gen_range(FLOW_RUN_MIN..=FLOW_RUN_MAX);
        }
        self.words_left_in_run -= 1;

        let r: f32 = rng.gen();
        let span = (WORD_LEN_MAX - WORD_LEN_MIN) as f32;
        self.word_len = WORD_LEN_MIN + (r * r * span) as u32;
        self.keys_left = self.word_len;

        self.roll_keys_left = if rng.gen_bool(ROLL_CHANCE) {
            rng.gen_range(ROLL_LEN_MIN..=ROLL_LEN_MAX).min(self.word_len)
        } else {
            0
        };

        self.words_since_think += 1;
        self.words_since_sentence += 1;
        self.typo_done = false;
    }

    fn boundary_gap(&mut self, rng: &mut impl Rng) -> u64 {
        let (_, _, gap_lo, gap_hi, think) = self.flow.params();

        if self.words_since_sentence >= self.sentence_target {
            self.words_since_sentence = 0;
            self.words_since_think = 0;
            self.sentence_target = rng.gen_range(SENTENCE_WORDS_MIN..=SENTENCE_WORDS_MAX);
            return rng.gen_range(SENTENCE_PAUSE_MS_MIN..=SENTENCE_PAUSE_MS_MAX);
        }
        if self.words_since_think >= 3 && rng.gen_bool(think) {
            self.words_since_think = 0;
            return rng.gen_range(THINK_PAUSE_MS_MIN..=THINK_PAUSE_MS_MAX);
        }
        rng.gen_range(gap_lo..=gap_hi)
    }

    fn next_event(&mut self, rng: &mut impl Rng) -> KeyEvent {
        if let Some(ev) = self.queue.pop_front() {
            return ev;
        }

        if self.keys_left > 0 {
            let idx = self.word_len - self.keys_left;
            self.keys_left -= 1;
            let gap = if self.roll_keys_left > 0 {
                self.roll_keys_left -= 1;
                rng.gen_range(ROLL_MS_MIN..=ROLL_MS_MAX)
            } else {
                let (lo, hi, ..) = self.flow.params();
                let base = rng.gen_range(lo..=hi) as f32;
                (base * accel(idx, self.word_len)).round().max(16.0) as u64
            };
            return KeyEvent {
                kind: KeyKind::Click,
                gap_ms: gap,
            };
        }

        if !self.typo_done && rng.gen_bool(TYPO_CHANCE) {
            self.typo_done = true;
            let n = rng.gen_range(BACKSPACE_MIN..=BACKSPACE_MAX);
            for _ in 0..n {
                self.queue.push_back(KeyEvent {
                    kind: KeyKind::Backspace,
                    gap_ms: rng.gen_range(BACKSPACE_MS_MIN..=BACKSPACE_MS_MAX),
                });
            }
            for _ in 0..n {
                self.queue.push_back(KeyEvent {
                    kind: KeyKind::Click,
                    gap_ms: rng.gen_range(ROLL_MS_MIN..=ROLL_MS_MAX),
                });
            }
            return self.queue.pop_front().unwrap();
        }

        let gap = self.boundary_gap(rng);
        self.start_word(rng);
        KeyEvent {
            kind: KeyKind::Space,
            gap_ms: gap,
        }
    }
}

#[derive(PartialEq)]
enum Signal {
    Start,
    Typing,
    Alert,
    Stop,
    Quit,
    None,
}

impl Signal {
    fn parse(s: &str) -> Signal {
        match s.trim().to_lowercase().as_str() {
            "start" => Signal::Start,
            "typing" => Signal::Typing,
            "alert" => Signal::Alert,
            "stop" => Signal::Stop,
            "quit" => Signal::Quit,
            _ => Signal::None,
        }
    }
}

fn read_signal(signal_file: &Path, last_nonce: &mut String) -> Signal {
    let content = match fs::read_to_string(signal_file) {
        Ok(c) => c,
        Err(_) => return Signal::None,
    };
    let mut parts = content.split_whitespace();
    let nonce = match parts.next() {
        Some(n) => n,
        None => return Signal::None,
    };
    let sig = parts.next().unwrap_or("");
    if nonce == last_nonce {
        return Signal::None;
    }
    *last_nonce = nonce.to_string();
    Signal::parse(sig)
}

/// Liveness check via a direct syscall — no subprocess spawn, so it is cheap
/// enough to run on every hook invocation.
#[cfg(windows)]
fn pid_alive(pid: u32) -> bool {
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn GetExitCodeProcess(handle: isize, code: *mut u32) -> i32;
        fn CloseHandle(handle: isize) -> i32;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle == 0 {
            return false;
        }
        let mut code: u32 = 0;
        let queried = GetExitCodeProcess(handle, &mut code) != 0;
        CloseHandle(handle);
        queried && code == STILL_ACTIVE
    }
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    unsafe { kill(pid as i32, 0) == 0 }
}

fn acquire_single_instance(state_dir: &Path) -> bool {
    let lock = state_dir.join("daemon.lock");
    for _ in 0..4 {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
        {
            Ok(mut f) => {
                let _ = write!(f, "{}", std::process::id());
                return true;
            }
            Err(_) => {
                let mut owner = None;
                for _ in 0..4 {
                    if let Ok(s) = fs::read_to_string(&lock) {
                        if let Ok(pid) = s.trim().parse::<u32>() {
                            owner = Some(pid);
                            break;
                        }
                    }
                    thread::sleep(Duration::from_millis(30));
                }
                match owner {
                    Some(pid) if pid != std::process::id() && pid_alive(pid) => return false,
                    _ => {
                        let _ = fs::remove_file(&lock);
                    }
                }
            }
        }
    }
    false
}

fn write_pid(state_dir: &Path) {
    let _ = fs::write(
        state_dir.join("daemon.pid"),
        std::process::id().to_string(),
    );
}

fn cleanup(state_dir: &Path) {
    let _ = fs::remove_file(state_dir.join("daemon.pid"));
    let _ = fs::remove_file(state_dir.join("daemon.lock"));
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// True if a daemon recorded in `daemon.pid` is currently alive.
fn daemon_running(state_dir: &Path) -> bool {
    fs::read_to_string(state_dir.join("daemon.pid"))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .map(pid_alive)
        .unwrap_or(false)
}

/// Launches the daemon as a fully detached background process that outlives
/// this short-lived hook invocation. Stdio is redirected to the log file so it
/// holds no console; on Unix `setsid` puts it in its own session, immune to a
/// terminal `SIGHUP`.
fn spawn_daemon(exe: &Path, state_dir: &Path) {
    let mut cmd = Command::new(exe);
    cmd.stdin(Stdio::null());

    match fs::File::create(state_dir.join("daemon.log")) {
        Ok(log) => {
            let err = log.try_clone().unwrap_or_else(|_| {
                fs::File::create(state_dir.join("daemon.log")).expect("log file")
            });
            cmd.stdout(Stdio::from(log)).stderr(Stdio::from(err));
        }
        Err(_) => {
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid is async-signal-safe and valid in the forked child.
        unsafe {
            cmd.pre_exec(|| {
                extern "C" {
                    fn setsid() -> i32;
                }
                setsid();
                Ok(())
            });
        }
    }

    let _ = cmd.spawn();
}

/// Hook entry point: relays a single signal to the daemon and starts the
/// daemon if it is not already running. Invoked as `chappie-daemon signal <name>`.
fn run_signal(sig: &str) {
    let sig = sig.trim().to_lowercase();
    if !matches!(sig.as_str(), "start" | "typing" | "alert" | "stop" | "quit") {
        eprintln!("[chappie] unknown signal: {}", sig);
        std::process::exit(2);
    }

    let home = match home_dir() {
        Some(h) => h,
        None => return,
    };
    let state_dir = home.join(".claude").join(".chappie_state");
    if fs::create_dir_all(&state_dir).is_err() {
        return;
    }

    // Write "<nonce> <signal>" atomically — a fresh nonce per write means the
    // daemon reacts exactly once and concurrent hooks never clobber a signal.
    let nonce = format!(
        "{}{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
        std::process::id()
    );
    let signal_file = state_dir.join("signal");
    let tmp = state_dir.join(format!("signal.{}.tmp", std::process::id()));
    if fs::write(&tmp, format!("{} {}", nonce, sig)).is_ok() {
        let _ = fs::rename(&tmp, &signal_file);
    }

    // `quit` only tells a running daemon to exit — never spawn one for it.
    if sig == "quit" || daemon_running(&state_dir) {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        spawn_daemon(&exe, &state_dir);
    }
}

fn download(url: &str, dest: &Path) -> Result<(), String> {
    let tmp = dest.with_extension("part");
    let _ = fs::remove_file(&tmp);

    let curl = std::process::Command::new("curl")
        .args(["-fsSL", "--retry", "2", "--max-time", "30", "-o"])
        .arg(&tmp)
        .arg(url)
        .status();

    let ok = match curl {
        Ok(s) if s.success() => true,
        _ => std::process::Command::new("wget")
            .args(["-q", "--tries=2", "--timeout=30", "-O"])
            .arg(&tmp)
            .arg(url)
            .status()
            .map(|s| s.success())
            .unwrap_or(false),
    };

    if !ok {
        let _ = fs::remove_file(&tmp);
        return Err(format!("curl/wget failed for {}", url));
    }
    match fs::metadata(&tmp) {
        Ok(m) if m.len() > 0 => {}
        _ => {
            let _ = fs::remove_file(&tmp);
            return Err(format!("downloaded file was empty: {}", url));
        }
    }
    fs::rename(&tmp, dest).map_err(|e| format!("rename failed: {}", e))
}

fn ensure_sounds(sounds_dir: &Path) -> Result<Vec<Arc<[u8]>>, String> {
    fs::create_dir_all(sounds_dir)
        .map_err(|e| format!("cannot create {}: {}", sounds_dir.display(), e))?;

    let mut buffers = Vec::with_capacity(SOUND_ASSETS.len());
    for asset in &SOUND_ASSETS {
        let path = sounds_dir.join(asset.cache_name);
        let cached = matches!(fs::metadata(&path), Ok(m) if m.len() > 0);
        if !cached {
            logln!("Downloading {} ...", asset.cache_name);
            download(asset.url, &path).map_err(|e| format!("{}: {}", asset.cache_name, e))?;
            logln!("Cached {}", asset.cache_name);
        }
        let data =
            fs::read(&path).map_err(|e| format!("cannot read {}: {}", asset.cache_name, e))?;
        if data.is_empty() {
            return Err(format!("{} is empty", asset.cache_name));
        }
        buffers.push(Arc::from(data.into_boxed_slice()));
    }
    Ok(buffers)
}

#[cfg(windows)]
fn enable_efficiency_mode() {
    #[repr(C)]
    struct ProcessPowerThrottlingState {
        version: u32,
        control_mask: u32,
        state_mask: u32,
    }

    const PROCESS_POWER_THROTTLING_EXECUTION_SPEED: u32 = 0x1;
    const PROCESS_POWER_THROTTLING_CURRENT_VERSION: u32 = 1;
    const PROCESS_INFORMATION_CLASS_POWER_THROTTLING: i32 = 4;
    const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;

    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn SetProcessInformation(
            handle: isize,
            information_class: i32,
            information: *const core::ffi::c_void,
            information_size: u32,
        ) -> i32;
        fn SetPriorityClass(handle: isize, priority_class: u32) -> i32;
    }

    let state = ProcessPowerThrottlingState {
        version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        control_mask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        state_mask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    };

    unsafe {
        let h = GetCurrentProcess();
        SetProcessInformation(
            h,
            PROCESS_INFORMATION_CLASS_POWER_THROTTLING,
            &state as *const _ as *const core::ffi::c_void,
            core::mem::size_of::<ProcessPowerThrottlingState>() as u32,
        );
        SetPriorityClass(h, BELOW_NORMAL_PRIORITY_CLASS);
    }
    logln!("Windows efficiency mode (EcoQoS) enabled.");
}

fn play_buffer(handle: &OutputStreamHandle, data: &Arc<[u8]>, speed: f32, volume: f32) {
    let decoder = match Decoder::new(Cursor::new(data.clone())) {
        Ok(d) => d,
        Err(e) => {
            logln!("decode error: {}", e);
            return;
        }
    };
    let source = decoder.speed(speed).amplify(volume);
    match Sink::try_new(handle) {
        Ok(sink) => {
            sink.append(source);
            sink.detach();
        }
        Err(e) => logln!("audio sink error: {}", e),
    }
}

fn play_event(
    handle: &OutputStreamHandle,
    buffers: &[Arc<[u8]>],
    kind: KeyKind,
    rng: &mut impl Rng,
) {
    let (idx, speed, volume) = match kind {
        KeyKind::Click => (
            rng.gen_range(0..CLICK_COUNT),
            rng.gen_range(MIN_PITCH..=MAX_PITCH),
            rng.gen_range(MIN_VOLUME..=MAX_VOLUME),
        ),
        KeyKind::Space => (
            SPACE_IDX,
            rng.gen_range(MIN_PITCH..=MAX_PITCH),
            rng.gen_range(MIN_VOLUME..=MAX_VOLUME),
        ),
        KeyKind::Backspace => (
            rng.gen_range(0..CLICK_COUNT),
            rng.gen_range(0.80..=0.90),
            rng.gen_range(0.45..=0.62),
        ),
        KeyKind::Alert => (ALERT_IDX, 1.0, 0.85),
    };
    play_buffer(handle, &buffers[idx], speed, volume);
}

#[derive(PartialEq, Clone, Copy)]
enum DaemonState {
    Idle,
    Typing,
    Alert,
}

fn run(state_dir: &Path, handle: &OutputStreamHandle, buffers: &[Arc<[u8]>]) {
    let signal_file = state_dir.join("signal");
    let mut last_nonce = String::new();

    let mut rng = rand::thread_rng();
    let mut state = DaemonState::Idle;
    let mut rhythm = TypingRhythm::new(&mut rng);
    let mut next_keystroke = Instant::now();
    let mut last_activity = Instant::now();
    let mut idle_since = Instant::now();
    let mut last_alert = Instant::now();

    loop {
        match read_signal(&signal_file, &mut last_nonce) {
            Signal::Quit => {
                logln!("Quit signal received — shutting down.");
                return;
            }
            Signal::Stop => {
                if state != DaemonState::Idle {
                    logln!("Stopped.");
                    state = DaemonState::Idle;
                }
                idle_since = Instant::now();
            }
            Signal::Start | Signal::Typing => {
                last_activity = Instant::now();
                if state != DaemonState::Typing {
                    logln!("Typing mode.");
                    state = DaemonState::Typing;
                    rhythm.reset(&mut rng);
                    next_keystroke = Instant::now();
                }
            }
            Signal::Alert => {
                last_activity = Instant::now();
                if state != DaemonState::Alert {
                    logln!("Alert — permission required.");
                    state = DaemonState::Alert;
                    last_alert = Instant::now() - Duration::from_millis(ALERT_REPEAT_MS);
                }
            }
            Signal::None => {}
        }

        if state == DaemonState::Typing
            && last_activity.elapsed() > Duration::from_millis(TYPING_TIMEOUT_MS)
        {
            logln!("Typing safety timeout — going idle.");
            state = DaemonState::Idle;
            idle_since = Instant::now();
        }

        if state == DaemonState::Idle
            && idle_since.elapsed() > Duration::from_millis(IDLE_SHUTDOWN_MS)
        {
            logln!("Idle — shutting down (restarts on next hook).");
            return;
        }

        match state {
            DaemonState::Typing => {
                if Instant::now() >= next_keystroke {
                    let ev = rhythm.next_event(&mut rng);
                    play_event(handle, buffers, ev.kind, &mut rng);
                    next_keystroke = Instant::now() + Duration::from_millis(ev.gap_ms);
                }
                let wait = next_keystroke
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(SIGNAL_POLL_MS))
                    .max(Duration::from_millis(1));
                thread::sleep(wait);
            }
            DaemonState::Alert => {
                if last_alert.elapsed() >= Duration::from_millis(ALERT_REPEAT_MS) {
                    play_event(handle, buffers, KeyKind::Alert, &mut rng);
                    last_alert = Instant::now();
                }
                thread::sleep(Duration::from_millis(SIGNAL_POLL_MS));
            }
            DaemonState::Idle => {
                thread::sleep(Duration::from_millis(IDLE_POLL_MS));
            }
        }
    }
}

fn main() {
    // `chappie-daemon signal <name>` — hook entry point (cross-platform, no
    // shell scripts). Bare `chappie-daemon` — run as the audio daemon.
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "signal" {
        run_signal(&args[2]);
        return;
    }
    run_daemon();
}

fn run_daemon() {
    logln!(
        "Chappie daemon v{} starting (pid {}).",
        env!("CARGO_PKG_VERSION"),
        std::process::id()
    );

    #[cfg(windows)]
    enable_efficiency_mode();

    let home = match home_dir() {
        Some(h) => h,
        None => {
            logln!("Cannot determine home directory. Exiting.");
            std::process::exit(1);
        }
    };
    let claude_dir = home.join(".claude");
    let state_dir = claude_dir.join(".chappie_state");
    if let Err(e) = fs::create_dir_all(&state_dir) {
        logln!("Cannot create state directory: {}", e);
        std::process::exit(1);
    }

    if !acquire_single_instance(&state_dir) {
        logln!("Another Chappie daemon is already running — exiting.");
        return;
    }
    write_pid(&state_dir);

    let buffers = match ensure_sounds(&claude_dir.join("sounds")) {
        Ok(b) => b,
        Err(e) => {
            logln!("Sound assets unavailable: {}", e);
            cleanup(&state_dir);
            std::process::exit(1);
        }
    };

    let (_stream, handle) = match OutputStream::try_default() {
        Ok(s) => s,
        Err(e) => {
            logln!("No audio output device: {}", e);
            cleanup(&state_dir);
            std::process::exit(1);
        }
    };

    logln!(
        "Ready — watching {}",
        state_dir.join("signal").display()
    );
    run(&state_dir, &handle, &buffers);

    cleanup(&state_dir);
    logln!("Chappie daemon stopped.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accel_is_slow_at_word_edges_fast_in_middle() {
        let len = 8;
        let first = accel(0, len);
        let middle = accel(4, len);
        let last = accel(len - 1, len);
        assert!(first > middle, "first key should be slower than the middle");
        assert!(last > middle, "last key should ease off vs the middle");
        assert!(accel(0, 1) == 1.0, "single-key words are neutral");
    }

    #[test]
    fn rhythm_produces_only_sane_gaps_and_all_event_kinds() {
        let mut rng = rand::thread_rng();
        let mut rhythm = TypingRhythm::new(&mut rng);
        let (mut clicks, mut spaces, mut backspaces) = (0u32, 0u32, 0u32);

        for _ in 0..20_000 {
            let ev = rhythm.next_event(&mut rng);
            assert!(
                ev.gap_ms >= 16 && ev.gap_ms <= THINK_PAUSE_MS_MAX,
                "gap {} out of range",
                ev.gap_ms
            );
            match ev.kind {
                KeyKind::Click => clicks += 1,
                KeyKind::Space => spaces += 1,
                KeyKind::Backspace => backspaces += 1,
                KeyKind::Alert => panic!("rhythm must never emit Alert"),
            }
        }
        assert!(clicks > 0 && spaces > 0, "expected clicks and spaces");
        assert!(backspaces > 0, "expected at least one typo correction");
    }

    #[test]
    fn signal_parsing_round_trips() {
        assert!(Signal::parse(" START ") == Signal::Start);
        assert!(Signal::parse("quit") == Signal::Quit);
        assert!(Signal::parse("garbage") == Signal::None);
    }
}
