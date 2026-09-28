use anyhow::{anyhow, Context, Result};
use eframe::egui;
use reqwest::blocking::Client;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use zip::ZipArchive;

const MODEL_URL: &str = "https://huggingface.co/Qwen/Qwen3-8B-GGUF/resolve/main/Qwen3-8B-Q5_K_M.gguf?download=true";
const MODEL_NAME: &str = "Qwen3-8B-Q5_K_M.gguf";

#[derive(Debug, Clone, Deserialize)]
struct LauncherConfig {
    repository: String,
    channel: String,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self { repository: "Vizyer/kroniki-rpg".into(), channel: "preview".into() }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct AppVersion {
    version: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubRelease {
    tag_name: String,
    prerelease: bool,
    draft: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct UpdateManifest {
    version: String,
    channel: String,
    package_url: String,
    sha256: String,
}

#[derive(Default, Debug, Clone)]
struct JobState {
    running: bool,
    progress: f32,
    message: String,
    error: Option<String>,
    done: bool,
}

struct LauncherApp {
    root: PathBuf,
    config: LauncherConfig,
    installed_version: String,
    available: Option<UpdateManifest>,
    status: String,
    update_job: Arc<Mutex<JobState>>,
    model_job: Arc<Mutex<JobState>>,
}

impl LauncherApp {
    fn new() -> Self {
        let root = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        let config = read_json::<LauncherConfig>(&root.join("launcher-config.json")).unwrap_or_default();
        let installed_version = read_json::<AppVersion>(&root.join("app/version.json"))
            .map(|v| v.version)
            .unwrap_or_else(|| "0.0.0".into());

        Self {
            root,
            config,
            installed_version,
            available: None,
            status: "Gotowy.".into(),
            update_job: Arc::new(Mutex::new(JobState::default())),
            model_job: Arc::new(Mutex::new(JobState::default())),
        }
    }

    fn game_exe(&self) -> PathBuf {
        self.root.join("app/KronikiRPG.exe")
    }

    fn model_path(&self) -> PathBuf {
        data_dir().join("ai").join(MODEL_NAME)
    }

    fn model_installed(&self) -> bool {
        self.model_path().is_file()
    }

    fn check_updates(&mut self) {
        self.status = "Sprawdzanie aktualizacji…".into();
        match find_update(&self.config, &self.installed_version) {
            Ok(Some(m)) => {
                self.status = format!("Dostępna wersja {}.", m.version);
                self.available = Some(m);
            }
            Ok(None) => {
                self.available = None;
                self.status = "Masz najnowszą wersję dla wybranego kanału.".into();
            }
            Err(e) => self.status = format!("Nie udało się sprawdzić aktualizacji: {e}"),
        }
    }

    fn start_update(&mut self) {
        let Some(manifest) = self.available.clone() else { return; };
        let root = self.root.clone();
        let job = self.update_job.clone();
        if job.lock().map(|j| j.running).unwrap_or(true) { return; }
        if let Ok(mut j) = job.lock() {
            *j = JobState { running: true, message: "Pobieranie aktualizacji…".into(), ..Default::default() };
        }
        thread::spawn(move || {
            let result = install_update(&root, &manifest, &job);
            finish_job(&job, result.map(|_| "Aktualizacja zainstalowana. Uruchom grę.".into()));
        });
    }

    fn start_model_download(&mut self) {
        if self.model_installed() { return; }
        let target = self.model_path();
        let job = self.model_job.clone();
        if job.lock().map(|j| j.running).unwrap_or(true) { return; }
        if let Ok(mut j) = job.lock() {
            *j = JobState { running: true, message: "Pobieranie Qwen3-8B Q5_K_M…".into(), ..Default::default() };
        }
        thread::spawn(move || {
            let result = download_large_file(MODEL_URL, &target, &job);
            finish_job(&job, result.map(|_| "Lokalny MGAI został zainstalowany.".into()));
        });
    }

    fn play(&mut self) {
        let exe = self.game_exe();
        if !exe.is_file() {
            self.status = "Brak pliku gry. Uruchom aktualizację albo instalator.".into();
            return;
        }
        match Command::new(&exe).current_dir(exe.parent().unwrap_or(&self.root)).spawn() {
            Ok(_) => self.status = "Gra uruchomiona.".into(),
            Err(e) => self.status = format!("Nie można uruchomić gry: {e}"),
        }
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(250));

        if let Ok(job) = self.update_job.lock() {
            if job.done {
                if let Some(e) = &job.error {
                    self.status = format!("Aktualizacja nie powiodła się: {e}");
                } else if !job.message.is_empty() {
                    self.status = job.message.clone();
                    if let Ok(v) = read_json::<AppVersion>(&self.root.join("app/version.json")) {
                        self.installed_version = v.version;
                    }
                }
            }
        }
        if let Ok(job) = self.model_job.lock() {
            if job.done {
                if let Some(e) = &job.error {
                    self.status = format!("Instalacja MGAI nie powiodła się: {e}");
                } else if !job.message.is_empty() {
                    self.status = job.message.clone();
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(18.0);
            ui.heading("Kroniki RPG");
            ui.label("Autonomiczny Mistrz Gry • Godot + Rust Core");
            ui.add_space(12.0);

            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Zainstalowana: {}", self.installed_version));
                    ui.separator();
                    ui.label(format!("Kanał: {}", self.config.channel));
                });
                ui.label(&self.status);
            });

            ui.add_space(14.0);

            let update_running = self.update_job.lock().map(|j| j.running).unwrap_or(false);
            let game_present = self.game_exe().is_file();

            ui.horizontal(|ui| {
                if ui.add_enabled(game_present && !update_running, egui::Button::new("GRAJ")).clicked() {
                    self.play();
                }
                if ui.add_enabled(!update_running, egui::Button::new("Sprawdź aktualizacje")).clicked() {
                    self.check_updates();
                }
                if ui.add_enabled(self.available.is_some() && !update_running, egui::Button::new("AKTUALIZUJ")).clicked() {
                    self.start_update();
                }
            });

            if let Ok(j) = self.update_job.lock() {
                if j.running {
                    ui.add_space(8.0);
                    ui.label(&j.message);
                    ui.add(egui::ProgressBar::new(j.progress.clamp(0.0, 1.0)).show_percentage());
                }
            }

            ui.add_space(18.0);
            ui.separator();
            ui.add_space(12.0);
            ui.heading("Lokalny Mistrz Gry");

            if self.model_installed() {
                ui.label("Qwen3-8B Q5_K_M • zainstalowany");
                ui.label("Model działa lokalnie i nie wymaga wysyłania zwykłych scen do chmury.");
            } else {
                ui.label("Qwen3-8B Q5_K_M • około 5,85 GB");
                ui.label("Dopasowany do profilu 16 GB RAM / 8 GB VRAM.");
                let model_running = self.model_job.lock().map(|j| j.running).unwrap_or(false);
                if ui.add_enabled(!model_running, egui::Button::new("POBIERZ LOKALNEGO MGAI")).clicked() {
                    self.start_model_download();
                }
            }

            if let Ok(j) = self.model_job.lock() {
                if j.running {
                    ui.add_space(8.0);
                    ui.label(&j.message);
                    ui.add(egui::ProgressBar::new(j.progress.clamp(0.0, 1.0)).show_percentage());
                }
            }

            ui.add_space(18.0);
            ui.label("Save'y i model AI są przechowywane poza katalogiem instalacji w LocalAppData.");
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 520.0])
            .with_min_inner_size([680.0, 460.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Kroniki RPG Launcher",
        options,
        Box::new(|_cc| Ok(Box::new(LauncherApp::new()))),
    )
}

fn data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("KronikiRPG")
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let data = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(serde_json::from_slice(&data)?)
}

fn client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent("KronikiRPG-Launcher/0.10")
        .timeout(Duration::from_secs(30))
        .build()?)
}

fn find_update(config: &LauncherConfig, installed: &str) -> Result<Option<UpdateManifest>> {
    let url = format!("https://api.github.com/repos/{}/releases?per_page=20", config.repository);
    let releases: Vec<GithubRelease> = client()?.get(url).send()?.error_for_status()?.json()?;
    let installed = normalize_version(installed).unwrap_or_else(|| Version::new(0,0,0));
    let mut candidates = Vec::new();

    for release in releases {
        if release.draft { continue; }
        let is_preview = config.channel.eq_ignore_ascii_case("preview");
        if !is_preview && release.prerelease { continue; }
        if is_preview && !release.prerelease { continue; }

        let Some(asset) = release.assets.iter().find(|a| a.name == "update-manifest.json") else { continue; };
        let manifest: UpdateManifest = client()?.get(&asset.browser_download_url).send()?.error_for_status()?.json()?;
        if !manifest.channel.eq_ignore_ascii_case(&config.channel) { continue; }
        if let Some(v) = normalize_version(&manifest.version) {
            if v > installed { candidates.push((v, manifest)); }
        }
    }

    candidates.sort_by(|a,b| b.0.cmp(&a.0));
    Ok(candidates.into_iter().next().map(|(_,m)|m))
}

fn normalize_version(s: &str) -> Option<Version> {
    Version::parse(s.trim().trim_start_matches('v')).ok()
}

fn install_update(root: &Path, manifest: &UpdateManifest, job: &Arc<Mutex<JobState>>) -> Result<()> {
    let staging_zip = root.join(".update.zip");
    download_large_file(&manifest.package_url, &staging_zip, job)?;

    set_job_message(job, "Weryfikacja SHA-256…", 0.92);
    let actual = sha256_file(&staging_zip)?;
    if !actual.eq_ignore_ascii_case(&manifest.sha256) {
        let _ = fs::remove_file(&staging_zip);
        return Err(anyhow!("SHA-256 aktualizacji nie zgadza się"));
    }

    let staging = root.join(".staging");
    let rollback = root.join(".rollback");
    let app = root.join("app");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;

    set_job_message(job, "Rozpakowywanie aktualizacji…", 0.95);
    extract_zip(&staging_zip, &staging)?;

    let _ = fs::remove_dir_all(&rollback);
    if app.exists() {
        fs::rename(&app, &rollback).context("backup current app")?;
    }

    if let Err(e) = fs::rename(&staging, &app) {
        if rollback.exists() && !app.exists() {
            let _ = fs::rename(&rollback, &app);
        }
        return Err(e).context("activate staged update");
    }

    let _ = fs::remove_file(&staging_zip);
    set_job_message(job, "Aktualizacja gotowa.", 1.0);
    Ok(())
}

fn download_large_file(url: &str, target: &Path, job: &Arc<Mutex<JobState>>) -> Result<()> {
    if let Some(parent) = target.parent() { fs::create_dir_all(parent)?; }
    let part = target.with_extension("part");
    let mut response = client()?.get(url).send()?.error_for_status()?;
    let total = response.content_length().unwrap_or(0);
    let mut out = File::create(&part)?;
    let mut buf = vec![0u8; 1024 * 1024];
    let mut downloaded = 0u64;

    loop {
        let n = response.read(&mut buf)?;
        if n == 0 { break; }
        out.write_all(&buf[..n])?;
        downloaded += n as u64;
        if total > 0 {
            if let Ok(mut j) = job.lock() {
                j.progress = (downloaded as f64 / total as f64) as f32 * 0.9;
                j.message = format!(
                    "Pobrano {:.1} / {:.1} GB",
                    downloaded as f64 / 1_073_741_824.0,
                    total as f64 / 1_073_741_824.0
                );
            }
        }
    }
    out.flush()?;
    fs::rename(&part, target)?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 { break; }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    let file = File::open(zip_path)?;
    let mut zip = ZipArchive::new(file)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(name) = entry.enclosed_name().map(Path::to_path_buf) else { continue; };
        let out = dest.join(name);
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
        } else {
            if let Some(parent) = out.parent() { fs::create_dir_all(parent)?; }
            let mut f = File::create(out)?;
            std::io::copy(&mut entry, &mut f)?;
        }
    }
    Ok(())
}

fn set_job_message(job: &Arc<Mutex<JobState>>, msg: &str, progress: f32) {
    if let Ok(mut j) = job.lock() {
        j.message = msg.into();
        j.progress = progress;
    }
}

fn finish_job(job: &Arc<Mutex<JobState>>, result: Result<String>) {
    if let Ok(mut j) = job.lock() {
        j.running = false;
        j.done = true;
        j.progress = if result.is_ok() { 1.0 } else { j.progress };
        match result {
            Ok(msg) => { j.message = msg; j.error = None; }
            Err(e) => j.error = Some(e.to_string()),
        }
    }
}
