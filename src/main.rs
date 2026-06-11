#![windows_subsystem = "windows"]

mod utils;
mod sgp;
mod consts;

use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use utils::find_files;
use crate::sgp::{start_game_processes};
use eframe::egui::{self};
use consts::{LAUNCHER_EXE, EAC_EXE, SHIPPING_EXE};
use lazy_static::lazy_static;

lazy_static! {
    pub static ref LOG_BUFFER: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
}

#[macro_export]
macro_rules! log_msg {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        eprintln!("{}", msg);
        if let Ok(mut logs) = $crate::LOG_BUFFER.lock() {
            logs.push(msg);
            // Feel free to change 200 to whatever you want the max log length to be
            if logs.len() > 200 {
                logs.remove(0);
            }
        }
    }};
}

// Very blatant copy of reboot launcher logs.,.,.,
fn create_reboot_args(username: &str, password: &str, host: bool, headless: bool, additional_args: &str) -> Vec<String> {
    let mut args = HashMap::new();
    
    // I have no idea what these do or how to get them but they seem to work so whatever lol
    args.insert("-epicapp".to_string(), "Fortnite".to_string());
    args.insert("-epicenv".to_string(), "Prod".to_string());
    args.insert("-epiclocale".to_string(), "en-us".to_string());
    args.insert("-epicportal".to_string(), "".to_string());
    args.insert("-skippatchcheck".to_string(), "".to_string());
    args.insert("-nobe".to_string(), "".to_string());
    args.insert("-fromfl".to_string(), "eac".to_string());
    args.insert("-fltoken".to_string(), "3db3ba5dcbd2e16703f3978d".to_string());
    args.insert("-caldera".to_string(), 
        "eyJhbGciOiJFUzI1NiIsInR5cCI6IkpXVCJ9.eyJhY2NvdW50X2lkIjoiYmU5ZGE1YzJmYmVhNDQwN2IyZjQwZWJhYWQ4NTlhZDQiLCJnZW5lcmF0ZWQiOjE2Mzg3MTcyNzgsImNhbGRlcmFHdWlkIjoiMzgxMGI4NjMtMmE2NS00NDU3LTliNTgtNGRhYjNiNDgyYTg2IiwiYWNQcm92aWRlciI6IkVhc3lBbnRpQ2hlYXQiLCJub3RlcyI6IiIsImZhbGxiYWNrIjpmYWxzZX0.VAWQB67RTxhiWOxx7DBjnzDnXyyEnX7OljJm-j2d88G_WgwQ9wrE6lwMEHZHjBd1ISJdUO1UVUqkfLdU5nofBQ".to_string());
    
    // Not bothering changing these but it's probably fine
    let final_username = if password.is_empty() {
        format!("{}@projectreboot.dev", username)
    } else {
        username.to_string()
    };
    let final_password = if password.is_empty() { "Rebooted".to_string() } else { password.to_string() };
    
    args.insert("-AUTH_LOGIN".to_string(), final_username);
    args.insert("-AUTH_PASSWORD".to_string(), final_password);
    args.insert("-AUTH_TYPE".to_string(), "epic".to_string());
    
    // Host args. Really no purpose to these quite yet, use reboot instead if you need to host a gs
    if host {
        args.insert("-nosplash".to_string(), "".to_string());
        args.insert("-nosound".to_string(), "".to_string());
        if headless {
            args.insert("-nullrhi".to_string(), "".to_string());
        }
    }
    
    for arg_str in additional_args.split_whitespace() {
        if arg_str.is_empty() {
            continue;
        }
        let separator_idx = arg_str.find('=');
        let (key, value) = match separator_idx {
            Some(idx) => {
                let k = &arg_str[..idx];
                let v = if idx + 1 < arg_str.len() { &arg_str[idx + 1..] } else { "" };
                (k, v)
            }
            None => (arg_str, ""),
        };
        args.insert(key.to_string(), value.to_string());
    }
    
    // [key1, value1, key2, value2 and so on]
    let mut result = Vec::new();
    for (key, value) in args.iter() {
        result.push(key.clone());
        if !value.is_empty() {
            result.push(value.clone());
        }
    }
    result
}

#[derive(serde::Serialize, serde::Deserialize, Default, Clone)]
struct Settings {
    game_path: String,
    auth_patch: String,
    console_patch: String,
    mod_patch: String,
}

fn settings_file() -> Option<PathBuf> {
    let proj = directories::ProjectDirs::from("com", "rifting", "cometlauncher")?;
    let dir = proj.config_dir();
    std::fs::create_dir_all(dir).ok()?;
    Some(dir.join("settings.json"))
}

fn load_settings() -> Settings {
    if let Some(path) = settings_file() {
        if let Ok(s) = std::fs::read_to_string(path) {
            if let Ok(cfg) = serde_json::from_str(&s) {
                return cfg;
            }
        }
    }
    Settings::default()
}

fn save_settings(s: &Settings) {
    if let Some(path) = settings_file() {
        if let Ok(txt) = serde_json::to_string_pretty(s) {
            let _ = std::fs::write(path, txt);
        }
    }
}

struct CometApp {
    settings: Settings,
    logo: Option<egui::TextureHandle>,
    logs: Vec<String>,
}

impl CometApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let settings = load_settings();
        Self { 
            settings, 
            logo: None, 
            logs: Vec::new(),
        }
    }

    fn browse_folder(&mut self, current: &str) -> Option<String> {
        if current.is_empty() {
            rfd::FileDialog::new().pick_folder().map(|p| p.display().to_string())
        } else {
            rfd::FileDialog::new()
                .set_directory(current)
                .pick_folder()
                .map(|p| p.display().to_string())
        }
    }

    fn browse_file(&mut self, current: &str) -> Option<String> {
        if current.is_empty() {
            rfd::FileDialog::new().pick_file().map(|p| p.display().to_string())
        } else {
            rfd::FileDialog::new()
                .set_directory(current)
                .pick_file()
                .map(|p| p.display().to_string())
        }
    }
}

impl eframe::App for CometApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // update log
        if let Ok(buffer) = LOG_BUFFER.lock() {
            self.logs = buffer.clone();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(tex) = &self.logo {
                    ui.add(egui::Image::new((tex.id(), egui::vec2(64.0, 64.0))));
                }
                ui.vertical(|ui| {
                    ui.heading("Comet Launcher");
                    ui.label("Enter paths below to the DLLs you wish to inject and hit play!");
                });
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Game Path:");
                ui.add(egui::TextEdit::singleline(&mut self.settings.game_path).desired_width(400.0));
                if ui.button("Browse").clicked() {
                    let current = self.settings.game_path.clone();
                    if let Some(p) = self.browse_folder(&current) {
                        self.settings.game_path = p;
                        save_settings(&self.settings);
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label("Auth Patch Path:");
                ui.add(egui::TextEdit::singleline(&mut self.settings.auth_patch).desired_width(400.0));
                if ui.button("Browse").clicked() {
                    let current = self.settings.auth_patch.clone();
                    if let Some(p) = self.browse_file(&current) {
                        self.settings.auth_patch = p;
                        save_settings(&self.settings);
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label("Console Patch Path:");
                ui.add(egui::TextEdit::singleline(&mut self.settings.console_patch).desired_width(400.0));
                if ui.button("Browse").clicked() {
                    let current = self.settings.console_patch.clone();
                    if let Some(p) = self.browse_file(&current) {
                        self.settings.console_patch = p;
                        save_settings(&self.settings);
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label("Memleak Fix Patch Path:");
                ui.add(egui::TextEdit::singleline(&mut self.settings.mod_patch).desired_width(400.0));
                if ui.button("Browse").clicked() {
                    let current = self.settings.mod_patch.clone();
                    if let Some(p) = self.browse_file(&current) {
                        self.settings.mod_patch = p;
                        save_settings(&self.settings);
                    }
                }
            });

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Play").clicked() {

                    // TODO: CHECK TCP 3551 FOR BACKEND

                    if self.settings.game_path.is_empty() {
                        log_msg!("[COMET] Game path is empty");
                    } else {
                        let gp = PathBuf::from(self.settings.game_path.clone());
                        let shipping = find_files(gp.clone(), SHIPPING_EXE.to_string());
                        if shipping.is_empty() {
                            log_msg!("[COMET] No game executable found in provided game path");
                        } else {
                            let auth_path = if self.settings.auth_patch.is_empty() { None } else { Some(PathBuf::from(self.settings.auth_patch.clone())) };
                            let console_path = if self.settings.console_patch.is_empty() { None } else { Some(PathBuf::from(self.settings.console_patch.clone())) };
                            let memory_path = if self.settings.mod_patch.is_empty() { None } else { Some(PathBuf::from(self.settings.mod_patch.clone())) };
                            let mut missing = Vec::new();
                            if let Some(p) = &auth_path {
                                if !p.exists() { missing.push(p.display().to_string()); }
                            }
                            if let Some(p) = &console_path {
                                if !p.exists() { missing.push(p.display().to_string()); }
                            }
                            if let Some(p) = &memory_path {
                                if !p.exists() { missing.push(p.display().to_string()); }
                            }
                            if !missing.is_empty() {
                                log_msg!("[COMET] Missing DLLs: {:?}", missing);
                            } else {
                                log_msg!("[COMET] Starting game processes with configured DLLs");
                                // spawn so we dont block the UI
                                let game_path = gp.clone();
                                let auth = auth_path;
                                let console = console_path;
                                let memory = memory_path;
                                std::thread::spawn(move || {
                                    // This function is really copied from reboot lol. May add the headless mode etc in the future...
                                    let args = create_reboot_args("", "", false, false, "");
                                    log_msg!("[COMET] Generated args: {:?}", args);
                                    let dlls = sgp::DllPaths {
                                        auth,
                                        console,
                                        memory_leak: memory,
                                    };
                                    match start_game_processes(game_path, SHIPPING_EXE, LAUNCHER_EXE, EAC_EXE, dlls, &args) {
                                        Some(instance) => log_msg!("[COMET] Started game instance: version={} pid={}", instance.version, instance.game_pid),
                                        None => log_msg!("[COMET] Failed to start game processes"),
                                    }
                                });
                            }
                        }
                    }
                }
                if ui.button("Save Config").clicked() {
                    save_settings(&self.settings);
                }
                if ui.button("Clear Console").clicked() {
                    if let Ok(mut logs) = LOG_BUFFER.lock() {
                        logs.clear();
                    }
                    self.logs.clear();
                }
            });

            ui.separator();

            ui.label("Logs");
            let frame = egui::Frame {
                inner_margin: egui::Margin::same(8.0),
                outer_margin: egui::Margin::ZERO,
                rounding: egui::Rounding::same(8.0),
                shadow: Default::default(),
                fill: egui::Color32::from_black_alpha(200),
                stroke: egui::Stroke::new(1.0, egui::Color32::DARK_GRAY),
            };
            
            frame.show(ui, |ui| {
                ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        let mut text = self.logs.join("\n");
                        ui.add(
                            egui::TextEdit::multiline(&mut text)
                                .desired_width(f32::INFINITY)
                                .desired_rows(20)
                                .frame(false)
                                .font(egui::TextStyle::Monospace)
                                .cursor_at_end(true),
                        );
                    });
            });
        });
    }
}

fn main() {
    let icon_data = match eframe::icon_data::from_png_bytes(include_bytes!("../assets/comet.png")) {
        Ok(icon) => Some(icon),
        Err(e) => {
            eprintln!("Failed to load icon: {}", e);
            None
        }
    };
    
    let mut viewport = egui::ViewportBuilder::default();
    if let Some(icon) = icon_data {
        viewport = viewport.with_icon(icon);
    }
    
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let _ = eframe::run_native(
        "Comet Launcher",
        options,
        Box::new(|cc| Box::new(CometApp::new(cc))),
    );
}
