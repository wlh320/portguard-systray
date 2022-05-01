#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tauri::api::process::{self, CommandChild};
use tauri::{AppHandle, SystemTrayMenuItem};
use tauri::{CustomMenuItem, SystemTrayMenu, SystemTraySubmenu};
use uuid::Uuid;
#[derive(Debug, Serialize, Deserialize)]
struct Config {
    /// all clients, identified by binary file location
    clients: HashMap<Uuid, PathBuf>,
    /// client that be executed last time
    last_selected: Option<Uuid>,
}

impl Config {
    fn new() -> Config {
        Config {
            clients: HashMap::new(),
            last_selected: None,
        }
    }
    fn read() -> Config {
        env::current_exe()
            .map(|exe| {
                let config_file = exe.with_extension("json");
                if config_file.exists() {
                    let s = fs::read_to_string(config_file).expect("Failed to read config file");
                    let c: Config = serde_json::from_str(&s).expect("Failed to parse config file");
                    c
                } else {
                    Self::new()
                }
            })
            .unwrap_or_else(|_| Self::new())
    }
    fn save(&self) {
        let _ = env::current_exe().map(|exe| {
            let config_file = exe.with_extension("json");
            let file = File::create(config_file).expect("Failed to create config file");
            serde_json::to_writer_pretty(file, &self).expect("Failed to write config file");
        });
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Status {
    Running(Uuid),
    Stopped(Uuid),
    Unselected,
}

#[derive(Debug)]
pub struct PortguardManager {
    config: Config,
    status: Status,
    child: Option<CommandChild>,
}

impl Default for PortguardManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PortguardManager {
    pub fn new() -> PortguardManager {
        PortguardManager {
            config: Config::read(),
            status: Status::Unselected,
            child: None,
        }
    }
    pub fn status(&self) -> Status {
        self.status
    }
    pub fn init(&mut self) {
        if let Some(id) = self.config.last_selected {
            self.status = Status::Stopped(id);
            self.start_background().ok();
        }
    }
    fn build_client_menu(&self, id: &Uuid, path: &Path) -> SystemTraySubmenu {
        let name = path.file_stem().unwrap().to_string_lossy();
        let mut client_item = SystemTrayMenu::new();
        match self.status {
            Status::Running(uuid) | Status::Stopped(uuid) if uuid == *id => {
                client_item = client_item
                    .add_item(CustomMenuItem::new(format!("s-{}", id), "Select").selected());
            }
            _ => {
                client_item =
                    client_item.add_item(CustomMenuItem::new(format!("s-{}", id), "Select"));
            }
        };
        client_item = client_item.add_item(CustomMenuItem::new(format!("r-{}", id), "Remove"));
        SystemTraySubmenu::new(name, client_item)
    }
    pub fn build_menu(&self) -> SystemTrayMenu {
        let mut clients_items = SystemTrayMenu::new();
        for (id, path) in &self.config.clients {
            let client_menu = self.build_client_menu(id, path);
            clients_items = clients_items.add_submenu(client_menu);
        }
        let clients_submenu = SystemTraySubmenu::new("Clients", clients_items);
        let mut tray_menu = SystemTrayMenu::new();
        tray_menu = tray_menu
            .add_item(CustomMenuItem::new("add", "Add Client"))
            .add_submenu(clients_submenu)
            .add_native_item(SystemTrayMenuItem::Separator);
        match self.status {
            Status::Running(_) => {
                tray_menu = tray_menu
                    .add_item(CustomMenuItem::new("start", "Start").disabled())
                    .add_item(CustomMenuItem::new("stop", "Stop"));
            }
            Status::Stopped(_) => {
                tray_menu = tray_menu
                    .add_item(CustomMenuItem::new("start", "Start"))
                    .add_item(CustomMenuItem::new("stop", "Stop").disabled());
            }
            Status::Unselected => {
                tray_menu = tray_menu
                    .add_item(CustomMenuItem::new("start", "Start").disabled())
                    .add_item(CustomMenuItem::new("stop", "Stop").disabled());
            }
        }
        tray_menu = tray_menu
            .add_native_item(SystemTrayMenuItem::Separator)
            .add_item(CustomMenuItem::new("about", "About"))
            .add_item(CustomMenuItem::new("quit", "Quit"));
        tray_menu
    }
    pub fn update_menu(&self, app: &AppHandle) {
        let new_menu = self.build_menu();
        app.tray_handle()
            .set_menu(new_menu)
            .expect("Failed to update menu");
    }
    pub fn add_client(&mut self, p: PathBuf) {
        let id = Uuid::new_v4();
        self.config.clients.insert(id, p);
    }
    pub fn remove_client(&mut self, id: Uuid) -> Result<(), Box<dyn Error>> {
        match self.status {
            Status::Running(uuid) => {
                if id != uuid {
                    self.config.clients.remove(&id);
                } else {
                    return Err("Client is Running".into());
                }
            }
            Status::Stopped(uuid) => {
                self.config.clients.remove(&id);
                if id == uuid {
                    self.status = Status::Unselected;
                }
            }
            Status::Unselected => {
                self.config.clients.remove(&id);
            }
        }
        if let Some(uuid) = self.config.last_selected {
            if uuid == id {
                self.config.last_selected = None;
            }
        }
        Ok(())
    }
    pub fn select_client(&mut self, id: Uuid) -> Result<(), Box<dyn Error>> {
        match self.status {
            Status::Running(uuid) => {
                if uuid != id {
                    // stop old
                    self.stop_background()?;
                    // change uuid to new
                    self.status = Status::Stopped(id);
                    // start new
                    self.start_background()?;
                }
            }
            _ => {
                self.status = Status::Stopped(id);
            }
        }
        self.config.last_selected = Some(id);
        Ok(())
    }
    pub fn start_background(&mut self) -> Result<(), Box<dyn Error>> {
        match self.status {
            Status::Stopped(id) => {
                // start process
                let path = self.config.clients.get(&id).unwrap();
                let (mut _rx, child) = process::Command::new(path.to_string_lossy()).spawn()?;
                self.status = Status::Running(id);
                self.child = Some(child);
                Ok(())
            }
            _ => Err("Other Is Running".into()),
        }
    }
    pub fn stop_background(&mut self) -> Result<(), Box<dyn Error>> {
        match self.status {
            Status::Running(id) => {
                // stop process
                self.child.take().unwrap().kill()?;
                self.status = Status::Stopped(id);
                Ok(())
            }
            _ => Ok(()),
        }
    }
    pub fn save_config(&self) {
        self.config.save();
    }
}
