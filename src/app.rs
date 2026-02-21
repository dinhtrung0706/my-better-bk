use crate::event::{AppEvent, CheckOutcome, Event, EventHandler, SplashStep};
use chrome_driver_rs::ensure_latest_driver;
use ratatui::{
    DefaultTerminal,
    crossterm::event::{KeyCode, KeyEvent},
};
use std::{
    collections::HashMap,
    fs,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use thirtyfour::{By, ChromiumLikeCapabilities, DesiredCapabilities, WebDriver};

const AUTH_CHECK_URL: &str = "https://mybk.hcmut.edu.vn/dkmh/dangKyMonHocForm.action";
const AUTH_LOGIN_URL: &str = "https://sso.hcmut.edu.vn/cas/login?service=https%3A%2F%2Fmybk.hcmut.edu.vn%2Fdkmh%2FdangKyMonHocForm.action";
const GITHUB_LATEST_RELEASE_URL: &str = "https://api.github.com/repos/owner/repo/releases/latest";
const STRATEGIES_FILE: &str = "strategies.conf";
const DEV_FLAG: &str = "--dd07t06-dev";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const MIN_SPLASH_DURATION: Duration = Duration::from_secs(5);
const LOGIN_REDIRECT_TIMEOUT: Duration = Duration::from_secs(30);
const WEBDRIVER_SERVER_URL: &str = "http://localhost:4444";
const WEBDRIVER_LOG_FILE: &str = "chromedriver.log";
const MAIN_MENU_LEN: usize = 4;
const DEFAULT_STRATEGIES_CONTENTS: &str = "0|1-2|3|0\n--:--\n\n";

/// Application.
#[derive(Debug)]
pub struct App {
    /// Is the application running?
    pub running: bool,
    /// Event handler.
    pub events: EventHandler,
    /// Current screen.
    pub screen: Screen,
    /// Tracks splash step outcomes.
    pub splash_results: HashMap<SplashStep, CheckOutcome>,
    /// When the splash started.
    pub splash_started_at: Instant,
    /// Auth status message.
    pub auth_message: Option<String>,
    /// Whether an update notice should be shown in main UI.
    pub update_notice: Option<String>,
    /// Whether we are in dev mode.
    pub dev_mode: bool,
    /// Auth input placeholders.
    pub auth_username: String,
    pub auth_password: String,
    /// Which auth field is active.
    pub auth_field: AuthField,
    /// Whether an auth attempt is currently running.
    pub auth_in_progress: bool,
    /// Selected menu index in the main screen.
    pub main_menu_index: usize,
    /// Whether to show the menu 4 easter egg art.
    pub show_menu_art: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Splash,
    Auth,
    Main,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthField {
    Username,
    Password,
}

impl Default for App {
    fn default() -> Self {
        Self {
            running: true,
            events: EventHandler::new(),
            screen: Screen::Splash,
            splash_results: HashMap::new(),
            splash_started_at: Instant::now(),
            auth_message: None,
            update_notice: None,
            dev_mode: false,
            auth_username: String::new(),
            auth_password: String::new(),
            auth_field: AuthField::Username,
            auth_in_progress: false,
            main_menu_index: 0,
            show_menu_art: false,
        }
    }
}

impl App {
    /// Constructs a new instance of [`App`].
    pub fn new() -> Self {
        let dev_mode = std::env::args().any(|arg| arg == DEV_FLAG);
        Self {
            dev_mode,
            ..Self::default()
        }
    }

    /// Run the application's main loop.
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> color_eyre::Result<()> {
        self.spawn_splash_checks();
        while self.running {
            terminal.draw(|frame| frame.render_widget(&self, frame.area()))?;
            match self.events.next().await? {
                Event::Tick => self.tick(),
                Event::Crossterm(event) => match event {
                    crossterm::event::Event::Key(key_event)
                        if key_event.kind == crossterm::event::KeyEventKind::Press =>
                    {
                        self.handle_key_events(key_event)?
                    }
                    _ => {}
                },
                Event::App(app_event) => self.handle_app_event(app_event),
            }
        }
        Ok(())
    }

    fn handle_app_event(&mut self, app_event: AppEvent) {
        match app_event {
            AppEvent::Quit => self.quit(),
            AppEvent::SplashCheckCompleted(step, outcome) => {
                if matches!(step, SplashStep::Version)
                    && matches!(outcome, CheckOutcome::Warning(_))
                    && let CheckOutcome::Warning(message) = &outcome
                {
                    self.update_notice = Some(message.clone());
                }
                self.splash_results.insert(step, outcome);
            }
            AppEvent::SplashFinished => {
                if matches!(self.screen, Screen::Splash) {
                    if matches!(
                        self.splash_results.get(&SplashStep::Auth),
                        Some(CheckOutcome::Failure(_))
                    ) {
                        self.screen = Screen::Auth;
                    } else {
                        self.screen = Screen::Main;
                    }
                }
            }
            AppEvent::AuthRequired => {
                // Wait for SplashFinished to transition screens.
            }
            AppEvent::AuthSucceeded => {
                self.screen = Screen::Main;
                self.auth_message = None;
                self.auth_in_progress = false;
            }
            AppEvent::AuthFailed(message) => {
                self.auth_message = Some(message);
                self.auth_in_progress = false;
            }
        }
    }

    /// Handles the key events and updates the state of [`App`].
    pub fn handle_key_events(&mut self, key_event: KeyEvent) -> color_eyre::Result<()> {
        match key_event.code {
            KeyCode::Esc | KeyCode::Char('q') => self.events.send(AppEvent::Quit),
            KeyCode::Enter if matches!(self.screen, Screen::Auth) => {
                if self.auth_in_progress {
                    self.auth_message = Some("Login already in progress...".to_string());
                    return Ok(());
                }
                if self.auth_username.trim().is_empty() || self.auth_password.trim().is_empty() {
                    self.auth_message = Some("Username and password cannot be blank.".to_string());
                    return Ok(());
                }
                self.auth_in_progress = true;
                self.auth_message = Some("Attempting login...\n".to_string());
                self.spawn_auth_login();
            }
            KeyCode::Tab if matches!(self.screen, Screen::Auth) => {
                self.auth_field = match self.auth_field {
                    AuthField::Username => AuthField::Password,
                    AuthField::Password => AuthField::Username,
                };
            }
            KeyCode::Char(ch) if matches!(self.screen, Screen::Auth) => match self.auth_field {
                AuthField::Username => self.auth_username.push(ch),
                AuthField::Password => self.auth_password.push(ch),
            },
            KeyCode::Backspace if matches!(self.screen, Screen::Auth) => match self.auth_field {
                AuthField::Username => {
                    self.auth_username.pop();
                }
                AuthField::Password => {
                    self.auth_password.pop();
                }
            },
            KeyCode::Up if matches!(self.screen, Screen::Main) => {
                if self.main_menu_index == 0 {
                    self.main_menu_index = MAIN_MENU_LEN - 1;
                } else {
                    self.main_menu_index -= 1;
                }
            }
            KeyCode::Down if matches!(self.screen, Screen::Main) => {
                self.main_menu_index = (self.main_menu_index + 1) % MAIN_MENU_LEN;
            }
            KeyCode::Char(ch) if matches!(self.screen, Screen::Main) => {
                if ('1'..='9').contains(&ch) {
                    let index = (ch as u8).saturating_sub(b'1') as usize;
                    if index < MAIN_MENU_LEN {
                        self.main_menu_index = index;
                        self.show_menu_art = index == 3;
                    }
                } else if ch == ' ' {
                    self.show_menu_art = self.main_menu_index == 3;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Handles the tick event of the terminal.
    ///
    /// The tick event is where you can update the state of your application with any logic that
    /// needs to be updated at a fixed frame rate. E.g. polling a server, updating an animation.
    pub fn tick(&self) {}

    /// Set running to false to quit the application.
    pub fn quit(&mut self) {
        self.running = false;
    }

    fn spawn_splash_checks(&mut self) {
        let sender = self.events.sender();
        let splash_started = self.splash_started_at;
        tokio::spawn(async move {
            let auth_outcome = check_auth().await;
            let _ = sender.send(Event::App(AppEvent::SplashCheckCompleted(
                SplashStep::Auth,
                auth_outcome.clone(),
            )));

            let strategies_outcome = check_strategies();
            let _ = sender.send(Event::App(AppEvent::SplashCheckCompleted(
                SplashStep::Strategies,
                strategies_outcome,
            )));

            let version_outcome = check_version().await;
            let _ = sender.send(Event::App(AppEvent::SplashCheckCompleted(
                SplashStep::Version,
                version_outcome,
            )));

            if matches!(auth_outcome, CheckOutcome::Failure(_)) {
                let _ = sender.send(Event::App(AppEvent::AuthRequired));
            }

            let elapsed = splash_started.elapsed();
            if elapsed < MIN_SPLASH_DURATION {
                tokio::time::sleep(MIN_SPLASH_DURATION - elapsed).await;
            }

            let _ = sender.send(Event::App(AppEvent::SplashFinished));
        });
    }

    fn spawn_auth_login(&mut self) {
        let sender = self.events.sender();
        let dev_mode = self.dev_mode;
        let username = self.auth_username.clone();
        let password = self.auth_password.clone();
        tokio::spawn(async move {
            let outcome = simulate_auth_login(dev_mode, username, password).await;
            match outcome {
                Ok(()) => {
                    let _ = sender.send(Event::App(AppEvent::AuthSucceeded));
                }
                Err(message) => {
                    let _ = sender.send(Event::App(AppEvent::AuthFailed(message)));
                }
            }
        });
    }
}

async fn check_auth() -> CheckOutcome {
    let cookie = match std::env::var("JSESSIONID") {
        Ok(value) if !value.is_empty() => value,
        _ => return CheckOutcome::Failure("Missing JSESSIONID env".to_string()),
    };

    let client = match reqwest::Client::builder().timeout(DEFAULT_TIMEOUT).build() {
        Ok(client) => client,
        Err(err) => return CheckOutcome::Failure(format!("Auth client error: {err}")),
    };

    let response = client
        .get(AUTH_CHECK_URL)
        .header("Cookie", format!("JSESSIONID={cookie}"))
        .send()
        .await;

    match response {
        Ok(resp) => {
            if resp.url().as_str().starts_with(AUTH_LOGIN_URL) {
                CheckOutcome::Failure("Redirected to login".to_string())
            } else {
                CheckOutcome::Success
            }
        }
        Err(err) => CheckOutcome::Failure(format!("Auth request failed: {err}")),
    }
}

fn check_strategies() -> CheckOutcome {
    match fs::read_to_string(STRATEGIES_FILE) {
        Ok(contents) if !contents.trim().is_empty() => {
            let lines: Vec<String> = contents.lines().map(|line| line.trim().to_string()).collect();
            if lines.iter().all(|line| line.is_empty()) {
                return CheckOutcome::Warning("Strategies empty".to_string());
            }
            if lines.len() != 3 {
                return reset_invalid_strategies(format!(
                    "expected 3 lines, got {}",
                    lines.len()
                ));
            }
            // if lines.iter().any(|line| line.is_empty()) {
            //     return reset_invalid_strategies("empty line detected".to_string());
            // }
            for line in 0..1 {
                if lines[line].is_empty() {
                    return reset_invalid_strategies("empty line detected".to_string());
                }
            }

            let (line1, line2, line3) = (&lines[0], &lines[1], &lines[2]);
            let cron_enabled = match parse_line1(line1) {
                Ok(enabled) => enabled,
                Err(message) => {
                    return reset_invalid_strategies(message);
                }
            };

            if let Err(message) = validate_cron_time(line2, cron_enabled) {
                return reset_invalid_strategies(message);
            }

            if let Err(message) = validate_subject_ids(line3) {
                return reset_invalid_strategies(message);
            }

            CheckOutcome::Success
        }
        Ok(_) => CheckOutcome::Warning("Strategies empty".to_string()),
        Err(_) => {
            if let Err(err) = fs::write(STRATEGIES_FILE, "") {
                return CheckOutcome::Warning(format!(
                    "Strategies missing; failed to create blank file: {err}"
                ));
            }
            CheckOutcome::Warning("Strategies missing; created blank file".to_string())
        }
    }
}

fn reset_invalid_strategies(reason: String) -> CheckOutcome {
    match fs::write(STRATEGIES_FILE, DEFAULT_STRATEGIES_CONTENTS) {
        Ok(()) => CheckOutcome::Warning(format!(
            "Strategies invalid ({reason}); reset to default"
        )),
        Err(err) => CheckOutcome::Failure(format!(
            "Strategies invalid ({reason}); failed to reset default: {err}"
        )),
    }
}

fn parse_line1(line: &str) -> Result<bool, String> {
    let parts: Vec<&str> = line.split('|').map(str::trim).collect();
    if parts.len() != 4 {
        return Err("line 1 must have 4 fields separated by '|'".to_string());
    }

    validate_range_list(
        parts[0],
        0,
        6,
        true,
        false,
        "day range",
    )?;
    validate_range_list(
        parts[1],
        1,
        16,
        false,
        true,
        "lesson range",
    )?;

    let max_subjects: u32 = parts[2]
        .parse()
        .map_err(|_| "maximum subjects must be a positive integer".to_string())?;
    if max_subjects == 0 {
        return Err("maximum subjects must be greater than 0".to_string());
    }

    let cron_enabled = match parts[3] {
        "0" => false,
        "1" => true,
        _ => return Err("cron mode must be 0 or 1".to_string()),
    };

    Ok(cron_enabled)
}

fn validate_range_list(
    value: &str,
    min: u32,
    max: u32,
    allow_single: bool,
    allow_slash: bool,
    label: &str,
) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("{label} cannot be empty"));
    }

    let normalized = if allow_slash {
        value.replace('/', ",")
    } else {
        value.to_string()
    };

    for token in normalized.split(',') {
        let token = token.trim();
        if token.is_empty() {
            return Err(format!("{label} contains empty entry"));
        }

        if let Some((start_raw, end_raw)) = token.split_once('-') {
            let start: u32 = start_raw
                .trim()
                .parse()
                .map_err(|_| format!("{label} has invalid number"))?;
            let end: u32 = end_raw
                .trim()
                .parse()
                .map_err(|_| format!("{label} has invalid number"))?;

            if start < min || end > max {
                return Err(format!("{label} must be within {min}-{max}"));
            }
            if allow_single {
                if start > end {
                    return Err(format!("{label} range must be ascending"));
                }
            } else if start >= end {
                return Err(format!("{label} range must have at least two values"));
            }
        } else {
            if !allow_single {
                return Err(format!("{label} must use ranges with '-'"));
            }
            let value: u32 = token
                .parse()
                .map_err(|_| format!("{label} has invalid number"))?;
            if value < min || value > max {
                return Err(format!("{label} must be within {min}-{max}"));
            }
        }
    }

    Ok(())
}

fn validate_cron_time(line: &str, cron_enabled: bool) -> Result<(), String> {
    if cron_enabled {
        let mut parts = line.split(':');
        let hour = parts.next().ok_or_else(|| "cron time missing hour".to_string())?;
        let minute = parts.next().ok_or_else(|| "cron time missing minute".to_string())?;
        if parts.next().is_some() {
            return Err("cron time must be HH:MM".to_string());
        }
        if hour.len() != 2 || minute.len() != 2 {
            return Err("cron time must be HH:MM".to_string());
        }

        let hour: u32 = hour
            .parse()
            .map_err(|_| "cron hour must be numeric".to_string())?;
        let minute: u32 = minute
            .parse()
            .map_err(|_| "cron minute must be numeric".to_string())?;
        if hour > 23 || minute > 59 {
            return Err("cron time must be within 00:00-23:59".to_string());
        }
        Ok(())
    } else if line == "--:--" {
        Ok(())
    } else {
        Err("cron time must be --:-- when cron mode is 0".to_string())
    }
}

fn validate_subject_ids(line: &str) -> Result<(), String> {
    if line.trim().is_empty() {
        return Ok(());
    }

    let subjects: Vec<&str> = line
        .split(',')
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect();

    if subjects.len() != line.split(',').count() {
        return Err("subject list contains empty entry".to_string());
    }

    Ok(())
}

async fn check_version() -> CheckOutcome {
    let package_version = env!("CARGO_PKG_VERSION");
    let current = match semver::Version::parse(package_version) {
        Ok(version) => version,
        Err(err) => return CheckOutcome::Warning(format!("Invalid version: {err}")),
    };

    let client = match reqwest::Client::builder()
        .timeout(DEFAULT_TIMEOUT)
        .user_agent("my-better-bk")
        .build()
    {
        Ok(client) => client,
        Err(err) => return CheckOutcome::Warning(format!("Version client error: {err}")),
    };

    let response = client.get(GITHUB_LATEST_RELEASE_URL).send().await;
    let response = match response {
        Ok(response) => response,
        Err(err) => return CheckOutcome::Warning(format!("Version check failed: {err}")),
    };

    let json: serde_json::Value = match response.json().await {
        Ok(json) => json,
        Err(err) => return CheckOutcome::Warning(format!("Invalid version response: {err}")),
    };

    let tag_name = json
        .get("tag_name")
        .and_then(|value| value.as_str())
        .unwrap_or("0.0.0");

    let latest = tag_name.trim_start_matches('v');
    match semver::Version::parse(latest) {
        Ok(latest_version) if latest_version > current => {
            CheckOutcome::Warning(format!("Update required: {current} -> {latest_version}"))
        }
        Ok(_) => CheckOutcome::Success,
        Err(err) => CheckOutcome::Warning(format!("Invalid latest version: {err}")),
    }
}

async fn simulate_auth_login(
    dev_mode: bool,
    auth_username: String,
    auth_password: String,
) -> Result<(), String> {
    let info = ensure_latest_driver("./driver").await.unwrap();
    let mut driver_process = Command::new(&info.driver_path)
        .arg("--port=4444")
        .arg("--log-level=INFO")
        .arg(format!("--log-path={WEBDRIVER_LOG_FILE}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| err.to_string())?;
    let mut caps = DesiredCapabilities::chrome();
    if !dev_mode {
        caps.add_arg("--headless").map_err(|err| err.to_string())?;
    }
    caps.add_arg("--no-sandbox")
        .map_err(|err| err.to_string())?;
    caps.add_arg("--disable-gpu")
        .map_err(|err| err.to_string())?;
    caps.add_arg("--disable-logging")
        .map_err(|err| err.to_string())?;
    caps.add_arg("--log-level=3")
        .map_err(|err| err.to_string())?;
    caps.add_arg("--silent").map_err(|err| err.to_string())?;

    if let Err(err) = wait_for_driver_ready(&mut driver_process, WEBDRIVER_SERVER_URL).await {
        let _ = driver_process.kill();
        return Err(err);
    }

    let driver = WebDriver::new(WEBDRIVER_SERVER_URL, caps)
        .await
        .map_err(|err| err.to_string())?;

    driver
        .goto(AUTH_LOGIN_URL)
        .await
        .map_err(|err| err.to_string())?;

    let login_result = async {
        let username_element = driver
            .find(By::Css("#username"))
            .await
            .map_err(|err| err.to_string())?;
        username_element
            .send_keys(auth_username)
            .await
            .map_err(|err| err.to_string())?;
        let password_element = driver
            .find(By::Css("#password"))
            .await
            .map_err(|err| err.to_string())?;
        password_element
            .send_keys(auth_password)
            .await
            .map_err(|err| err.to_string())?;
        let login_button = driver
            .find(By::Css("#fm1 > div.row.btn-row > input.btn-submit"))
            .await
            .map_err(|err| err.to_string())?;
        login_button.click().await.map_err(|err| err.to_string())?;

        wait_for_auth_redirect(&driver).await?;
        let session_id = extract_jsessionid(&driver).await?;
        write_jsessionid_to_env(&session_id)?;
        Ok(())
    }
    .await;

    if let Err(err) = login_result {
        let _ = driver.quit().await;
        let _ = driver_process.kill();
        return Err(err);
    }

    driver.quit().await.map_err(|err| err.to_string())?;
    let _ = driver_process.kill();
    Ok(())
}

async fn wait_for_driver_ready(
    driver_process: &mut std::process::Child,
    server_url: &str,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|err| err.to_string())?;
    let started = Instant::now();
    let timeout = Duration::from_secs(10);

    loop {
        if let Ok(Some(status)) = driver_process.try_wait() {
            return Err(format!(
                "ChromeDriver exited early with status {status}. See {WEBDRIVER_LOG_FILE}"
            ));
        }
        if started.elapsed() >= timeout {
            return Err(format!(
                "WebDriver did not become ready in time. See {WEBDRIVER_LOG_FILE}"
            ));
        }

        match client.get(format!("{server_url}/status")).send().await {
            Ok(response) if response.status().is_success() => return Ok(()),
            _ => tokio::time::sleep(Duration::from_millis(200)).await,
        }
    }
}

async fn wait_for_auth_redirect(driver: &WebDriver) -> Result<(), String> {
    let started = Instant::now();
    loop {
        let url = driver.current_url().await.map_err(|err| err.to_string())?;
        if url.as_str().starts_with(AUTH_CHECK_URL) {
            return Ok(());
        }
        if url.as_str().starts_with(AUTH_LOGIN_URL.split("?").nth(0).unwrap_or(""))
            && started.elapsed() >= Duration::from_secs(1)
            && is_wrong_credential_message_visible(driver).await?
        {
            return Err("Wrong username or password".to_string());
        }
        if started.elapsed() >= LOGIN_REDIRECT_TIMEOUT {
            return Err("Login redirect timed out".to_string());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn is_wrong_credential_message_visible(driver: &WebDriver) -> Result<bool, String> {
    if let Ok(element) = driver.find(By::Css("#msg")).await {
        let text = element.text().await.map_err(|err| err.to_string())?;
        if text.contains("cannot be determined to be authentic") {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn extract_jsessionid(driver: &WebDriver) -> Result<String, String> {
    let cookies = driver
        .get_all_cookies()
        .await
        .map_err(|err| err.to_string())?;
    let session = cookies
        .into_iter()
        .find(|cookie| cookie.name == "JSESSIONID")
        .ok_or_else(|| "JSESSIONID cookie not found".to_string())?;
    Ok(session.value.to_string())
}

fn write_jsessionid_to_env(session_id: &str) -> Result<(), String> {
    let env_path = ".env";
    let contents = fs::read_to_string(env_path).unwrap_or_default();
    let mut lines: Vec<String> = if contents.is_empty() {
        Vec::new()
    } else {
        contents.lines().map(|line| line.to_string()).collect()
    };

    let mut replaced = false;
    for line in &mut lines {
        if line.starts_with("JSESSIONID=") {
            *line = format!("JSESSIONID={session_id}");
            replaced = true;
            break;
        }
    }

    if !replaced {
        lines.push(format!("JSESSIONID={session_id}"));
    }

    let updated = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };
    fs::write(env_path, updated).map_err(|err| err.to_string())?;
    Ok(())
}
