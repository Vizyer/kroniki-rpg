use anyhow::{anyhow, Context, Result};
use chrono::Local;
use eframe::egui;
use reqwest::blocking::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};
use zip::ZipArchive;

const LAUNCHER_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LauncherConfig {
    repository: String,
    #[serde(default = "default_manifest_asset")]
    manifest_asset: String,
    #[serde(default = "default_package_asset")]
    package_asset: String,
}
fn default_manifest_asset() -> String { "update-manifest.json".into() }
fn default_package_asset() -> String { "KronikiRPG-win-x64.zip".into() }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LauncherSettings { channel: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateManifest {
    version: String,
    channel: String,
    package_url: String,
    sha256: String,
    #[serde(default)]
    package_size: Option<u64>,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    min_launcher_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    prerelease: bool,
    draft: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}
#[derive(Debug, Deserialize)]
struct GithubAsset { name: String, browser_download_url: String }

#[derive(Debug, Clone)]
struct Candidate { manifest: UpdateManifest }

#[derive(Debug)]
enum Msg {
    Status(String),
    Progress(f32),
    Candidate(Candidate),
    UpToDate(String),
    Done(String),
    Error(String),
}

struct LauncherApp {
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    config: LauncherConfig,
    channel: String,
    installed_version: String,
    candidate: Option<Candidate>,
    status: String,
    progress: f32,
    busy: bool,
    log: Vec<String>,
}

impl LauncherApp {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let config = read_config().unwrap_or(LauncherConfig {
            repository: "OWNER/REPOSITORY".into(),
            manifest_asset: default_manifest_asset(),
            package_asset: default_package_asset(),
        });
        let settings = read_settings().unwrap_or_default();
        let channel = if settings.channel == "preview" { "preview" } else { "stable" }.to_string();
        let installed_version = read_installed_version().unwrap_or_else(|| "0.0.0".into());
        let mut app = Self {
            tx, rx, config, channel, installed_version,
            candidate: None,
            status: "Gotowy".into(), progress: 0.0, busy: false, log: Vec::new(),
        };
        app.check_updates();
        app
    }

    fn push_log(&mut self, s: impl Into<String>) {
        let s = s.into();
        self.log.push(s);
        if self.log.len() > 80 { self.log.drain(0..self.log.len()-80); }
    }

    fn check_updates(&mut self) {
        if self.busy { return; }
        if !valid_repo(&self.config.repository) {
            self.status = "Tryb lokalny: skonfiguruj repozytorium GitHub, aby włączyć aktualizacje.".into();
            return;
        }
        self.busy = true;
        self.progress = 0.0;
        self.status = "Sprawdzam aktualizacje...".into();
        let tx = self.tx.clone();
        let cfg = self.config.clone();
        let channel = self.channel.clone();
        let current = self.installed_version.clone();
        thread::spawn(move || {
            match find_update(&cfg, &channel, &current) {
                Ok(Some(c)) => { let _ = tx.send(Msg::Candidate(c)); }
                Ok(None) => { let _ = tx.send(Msg::UpToDate(current)); }
                Err(e) => { let _ = tx.send(Msg::Error(format!("Nie udało się sprawdzić aktualizacji: {e:#}"))); }
            }
        });
    }

    fn install_candidate(&mut self) {
        if self.busy { return; }
        let Some(candidate) = self.candidate.clone() else { return; };
        self.busy = true;
        self.status = format!("Pobieram {}...", candidate.manifest.version);
        self.progress = 0.0;
        let tx = self.tx.clone();
        let old_version = self.installed_version.clone();
        thread::spawn(move || {
            if let Err(e) = install_update(&candidate.manifest, &old_version, &tx) {
                let _ = tx.send(Msg::Error(format!("Aktualizacja nie powiodła się: {e:#}")));
            }
        });
    }

    fn rollback(&mut self) {
        if self.busy { return; }
        self.busy = true;
        self.status = "Przywracam poprzednią wersję...".into();
        let tx = self.tx.clone();
        thread::spawn(move || {
            match rollback_app() {
                Ok(v) => { let _ = tx.send(Msg::Done(format!("Przywrócono poprzednią wersję {v}."))); }
                Err(e) => { let _ = tx.send(Msg::Error(format!("Rollback nie powiódł się: {e:#}"))); }
            }
        });
    }

    fn play(&mut self) {
        let exe = install_root().join("app").join("KronikiRPG.exe");
        if !exe.exists() {
            self.status = "Brak zainstalowanej gry. Użyj Aktualizuj/Zainstaluj.".into();
            return;
        }
        match Command::new(&exe).current_dir(exe.parent().unwrap()).spawn() {
            Ok(_) => std::process::exit(0),
            Err(e) => self.status = format!("Nie mogę uruchomić gry: {e}"),
        }
    }

    fn drain_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Status(s) => { self.status=s.clone(); self.push_log(s); }
                Msg::Progress(p) => self.progress=p.clamp(0.0,1.0),
                Msg::Candidate(c) => {
                    self.busy=false; self.progress=0.0;
                    self.status = format!("Dostępna aktualizacja {} ({})", c.manifest.version, c.manifest.channel);
                    self.push_log(self.status.clone()); self.candidate=Some(c);
                }
                Msg::UpToDate(v) => {
                    self.busy=false; self.progress=0.0; self.candidate=None;
                    self.status=format!("Gra jest aktualna — wersja {v}."); self.push_log(self.status.clone());
                }
                Msg::Done(s) => {
                    self.busy=false; self.progress=1.0; self.status=s.clone(); self.push_log(s);
                    self.installed_version = read_installed_version().unwrap_or_else(|| self.installed_version.clone());
                    self.candidate=None;
                }
                Msg::Error(s) => { self.busy=false; self.status=s.clone(); self.push_log(s); }
            }
        }
    }
}

impl eframe::App for LauncherApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain_messages();
        ui.ctx().request_repaint_after(Duration::from_millis(100));
        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(14.0);
            ui.heading("Kroniki RPG");
            ui.label(format!("Wersja gry: {}   •   Launcher: {}", self.installed_version, LAUNCHER_VERSION));
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label("Kanał aktualizacji:");
                let old = self.channel.clone();
                egui::ComboBox::from_id_salt("channel")
                    .selected_text(if self.channel=="preview" {"Preview"} else {"Stable"})
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.channel, "stable".into(), "Stable");
                        ui.selectable_value(&mut self.channel, "preview".into(), "Preview");
                    });
                if self.channel != old {
                    let _ = write_settings(&LauncherSettings { channel:self.channel.clone() });
                    self.candidate=None;
                    self.check_updates();
                }
            });

            ui.add_space(8.0);
            ui.label(&self.status);
            if self.busy || self.progress > 0.0 { ui.add(egui::ProgressBar::new(self.progress).show_percentage()); }
            ui.add_space(12.0);

            ui.horizontal(|ui| {
                let play_enabled = install_root().join("app").join("KronikiRPG.exe").exists() && !self.busy;
                if ui.add_enabled(play_enabled, egui::Button::new("▶  GRAJ").min_size(egui::vec2(170.0,44.0))).clicked() { self.play(); }
                let update_label = if self.installed_version=="0.0.0" {"ZAINSTALUJ"} else {"AKTUALIZUJ"};
                if ui.add_enabled(self.candidate.is_some() && !self.busy, egui::Button::new(update_label).min_size(egui::vec2(140.0,44.0))).clicked() { self.install_candidate(); }
                if ui.add_enabled(!self.busy, egui::Button::new("Sprawdź aktualizacje")).clicked() { self.check_updates(); }
            });

            if let Some(c) = &self.candidate {
                ui.add_space(12.0);
                ui.group(|ui| {
                    ui.strong(format!("Nowa wersja {}", c.manifest.version));
                    if !c.manifest.notes.trim().is_empty() { ui.label(&c.manifest.notes); }
                    if let Some(n)=c.manifest.package_size { ui.label(format!("Paczka: {:.1} MB", n as f64 / 1024.0 / 1024.0)); }
                });
            }

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(rollback_available() && !self.busy, egui::Button::new("Przywróć poprzednią wersję")).clicked() { self.rollback(); }
                if ui.button("Folder zapisów").clicked() { open_folder(&data_dir()); }
                if ui.button("Folder gry").clicked() { open_folder(&install_root()); }
            });

            ui.add_space(10.0);
            ui.collapsing("Dziennik launchera", |ui| {
                egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                    for line in self.log.iter().rev() { ui.monospace(line); }
                });
            });
        });
    }
}

fn valid_repo(repo:&str)->bool { repo.contains('/') && !repo.contains("OWNER") && !repo.contains("REPOSITORY") }

fn find_update(cfg:&LauncherConfig, channel:&str, current:&str)->Result<Option<Candidate>> {
    let client = Client::builder().user_agent(format!("KronikiRPGLauncher/{LAUNCHER_VERSION}")).build()?;
    let url = format!("https://api.github.com/repos/{}/releases?per_page=20", cfg.repository);
    let releases:Vec<GithubRelease> = client.get(url).send()?.error_for_status()?.json()?;
    let release = releases.into_iter().find(|r| {
        if r.draft { return false; }
        if channel=="stable" { !r.prerelease } else { true }
    }).ok_or_else(|| anyhow!("Brak opublikowanych wydań dla kanału {channel}."))?;
    let manifest_asset = release.assets.iter().find(|a| a.name==cfg.manifest_asset)
        .ok_or_else(|| anyhow!("W wydaniu {} brakuje {}.", release.tag_name, cfg.manifest_asset))?;
    let manifest:UpdateManifest = client.get(&manifest_asset.browser_download_url).send()?.error_for_status()?.json()?;
    if let Some(min) = &manifest.min_launcher_version {
        if version(min)? > version(LAUNCHER_VERSION)? {
            return Err(anyhow!("Ta aktualizacja wymaga launchera {} lub nowszego.", min));
        }
    }
    if version(&manifest.version)? > version(current)? { Ok(Some(Candidate{manifest})) } else { Ok(None) }
}

fn install_update(m:&UpdateManifest, old_version:&str, tx:&Sender<Msg>)->Result<()> {
    stop_orphan_core();
    let root = install_root();
    let app = root.join("app");
    let staging = root.join(".staging");
    let rollback = root.join(".rollback").join("previous-app");
    let cache = data_dir().join("cache");
    fs::create_dir_all(&cache)?;
    fs::create_dir_all(root.join(".rollback"))?;

    let _ = tx.send(Msg::Status("Tworzę kopię zapisów...".into()));
    backup_saves(old_version)?;

    let zip_path = cache.join(format!("KronikiRPG-{}.zip", m.version));
    let _ = tx.send(Msg::Status(format!("Pobieram wersję {}...",m.version)));
    download_file(&m.package_url,&zip_path,tx)?;
    let _ = tx.send(Msg::Status("Weryfikuję SHA-256...".into()));
    verify_sha256(&zip_path,&m.sha256)?;

    if staging.exists() { fs::remove_dir_all(&staging)?; }
    fs::create_dir_all(&staging)?;
    let _ = tx.send(Msg::Status("Rozpakowuję aktualizację...".into()));
    extract_zip(&zip_path,&staging)?;
    if !staging.join("KronikiRPG.exe").exists() || !staging.join("kroniki_core.exe").exists() {
        return Err(anyhow!("Paczka aktualizacji nie zawiera wymaganych plików gry."));
    }
    fs::write(staging.join("version.json"), serde_json::to_vec_pretty(&serde_json::json!({"version":m.version,"channel":m.channel}))?)?;

    if rollback.exists() { fs::remove_dir_all(&rollback)?; }
    if app.exists() { fs::rename(&app,&rollback).context("Nie można odłożyć poprzedniej wersji. Upewnij się, że gra jest zamknięta.")?; }
    if let Err(e)=fs::rename(&staging,&app) {
        if rollback.exists() && !app.exists() { let _=fs::rename(&rollback,&app); }
        return Err(e).context("Nie można aktywować nowej wersji");
    }
    let _=fs::remove_file(&zip_path);
    let _ = tx.send(Msg::Progress(1.0));
    let _ = tx.send(Msg::Done(format!("Aktualizacja do {} zakończona. Możesz kliknąć GRAJ.",m.version)));
    Ok(())
}

fn rollback_app()->Result<String> {
    stop_orphan_core();
    let root=install_root(); let app=root.join("app"); let previous=root.join(".rollback").join("previous-app");
    if !previous.exists(){return Err(anyhow!("Brak poprzedniej wersji do przywrócenia."));}
    let temp=root.join(".rollback").join("failed-current");
    if temp.exists(){fs::remove_dir_all(&temp)?;}
    if app.exists(){fs::rename(&app,&temp)?;}
    if let Err(e)=fs::rename(&previous,&app){ if temp.exists(){let _=fs::rename(&temp,&app);} return Err(e.into()); }
    if temp.exists(){fs::rename(&temp,&previous)?;}
    Ok(read_installed_version().unwrap_or_else(||"nieznana".into()))
}

fn stop_orphan_core(){
    let client=Client::builder().timeout(Duration::from_millis(700)).user_agent(format!("KronikiRPGLauncher/{LAUNCHER_VERSION}")).build();
    if let Ok(c)=client { let _=c.post("http://127.0.0.1:17377/shutdown").json(&serde_json::json!({})).send(); }
    thread::sleep(Duration::from_millis(250));
}

fn download_file(url:&str,path:&Path,tx:&Sender<Msg>)->Result<()> {
    let client=Client::builder().user_agent(format!("KronikiRPGLauncher/{LAUNCHER_VERSION}")).build()?;
    let mut response=client.get(url).send()?.error_for_status()?;
    let total=response.content_length().unwrap_or(0);
    let mut out=fs::File::create(path)?; let mut buf=[0u8;64*1024]; let mut got=0u64;
    loop { let n=response.read(&mut buf)?; if n==0{break;} out.write_all(&buf[..n])?; got+=n as u64; if total>0{let _=tx.send(Msg::Progress(got as f32/total as f32));} }
    Ok(())
}
fn verify_sha256(path:&Path,expected:&str)->Result<()> {
    let mut f=fs::File::open(path)?; let mut hasher=Sha256::new(); let mut buf=[0u8;64*1024];
    loop{let n=f.read(&mut buf)?;if n==0{break;}hasher.update(&buf[..n]);}
    let actual=format!("{:x}",hasher.finalize());
    if actual.eq_ignore_ascii_case(expected.trim()){Ok(())}else{Err(anyhow!("Błędna suma SHA-256. Oczekiwano {}, otrzymano {}.",expected,actual))}
}
fn extract_zip(path:&Path,dest:&Path)->Result<()> {
    let f=fs::File::open(path)?; let mut zip=ZipArchive::new(f)?;
    for i in 0..zip.len(){
        let mut item=zip.by_index(i)?;
        let Some(rel)=item.enclosed_name().map(PathBuf::from) else {continue;};
        let out=dest.join(rel);
        if item.is_dir(){fs::create_dir_all(&out)?;}else{if let Some(p)=out.parent(){fs::create_dir_all(p)?;} let mut of=fs::File::create(&out)?; std::io::copy(&mut item,&mut of)?;}
    }
    Ok(())
}

fn backup_saves(version:&str)->Result<()> {
    let d=data_dir(); if !d.exists(){return Ok(());}
    let name=format!("pre-update-{}-{}",sanitize(version),Local::now().format("%Y%m%d-%H%M%S"));
    let target=d.join("backups").join(name); fs::create_dir_all(&target)?;
    for n in ["kroniki.sqlite3","kroniki.sqlite3-wal","kroniki.sqlite3-shm","settings.json","launcher-settings.json"] {
        let src=d.join(n); if src.exists(){let _=fs::copy(&src,target.join(n));}
    }
    Ok(())
}
fn sanitize(s:&str)->String{s.chars().map(|c|if c.is_ascii_alphanumeric()||".-_".contains(c){c}else{'_'}).collect()}

fn install_root()->PathBuf { std::env::current_exe().ok().and_then(|p|p.parent().map(Path::to_path_buf)).unwrap_or_else(||PathBuf::from(".")) }
fn data_dir()->PathBuf { std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(||PathBuf::from(".")).join("KronikiRPG") }
fn config_path()->PathBuf { install_root().join("launcher-config.json") }
fn settings_path()->PathBuf { data_dir().join("launcher-settings.json") }
fn read_config()->Result<LauncherConfig>{Ok(serde_json::from_slice(&fs::read(config_path())?)?)}
fn read_settings()->Result<LauncherSettings>{Ok(serde_json::from_slice(&fs::read(settings_path())?)?)}
fn write_settings(s:&LauncherSettings)->Result<()> {fs::create_dir_all(data_dir())?;fs::write(settings_path(),serde_json::to_vec_pretty(s)?)?;Ok(())}
fn read_installed_version()->Option<String>{
    let p=install_root().join("app").join("version.json"); let v:serde_json::Value=serde_json::from_slice(&fs::read(p).ok()?).ok()?; v.get("version")?.as_str().map(str::to_string)
}
fn rollback_available()->bool{install_root().join(".rollback").join("previous-app").exists()}
fn version(s:&str)->Result<Version>{Version::parse(s.trim().trim_start_matches('v')).map_err(Into::into)}
fn open_folder(path:&Path){
    let _=fs::create_dir_all(path);
    open_folder_platform(path);
}
#[cfg(windows)]
fn open_folder_platform(path:&Path){ let _=Command::new("explorer.exe").arg(path).spawn(); }
#[cfg(not(windows))]
fn open_folder_platform(_path:&Path){}

fn main() -> eframe::Result<()> {
    let options=eframe::NativeOptions{viewport:egui::ViewportBuilder::default().with_inner_size([760.0,540.0]).with_min_inner_size([640.0,460.0]),..Default::default()};
    eframe::run_native("Kroniki RPG Launcher",options,Box::new(|_cc|Ok(Box::new(LauncherApp::new()))))
}
