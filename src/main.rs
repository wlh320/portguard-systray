#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]
use std::env;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tauri::api::dialog;
use tauri_api::dialog::Response::Okay;
// use tauri::window::{Window, WindowId};
use log;
use tauri::Manager;
use tauri::{SystemTray, SystemTrayEvent};
use uuid::Uuid;

use portguard_systray::PorguardManager;

fn main() {
    env_logger::init();
    let pm = Arc::new(Mutex::new(PorguardManager::new()));
    pm.lock().unwrap().init();
    let tray_menu = pm.lock().unwrap().build_menu();
    tauri::Builder::default()
        .system_tray(SystemTray::new().with_menu(tray_menu))
        .on_system_tray_event(move |app, event| {
            match event {
            SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
                "add" => {
                    let window = app.get_window("main").unwrap();
                    let filepath =
                        tauri_api::dialog::select(Option::<&String>::None, Option::<&Path>::None);
                    match filepath {
                        Ok(Okay(s)) => {
                            log::debug!("add client: {:?}", s);
                            pm.lock().unwrap().add_client(s);
                            pm.lock().unwrap().save_config();
                            pm.lock().unwrap().update_menu(app);
                        }
                        _ => {
                            dialog::message(Some(&window), "Invalid file", "select an invalid file")
                        }
                    }
                }
                "start" => {
                    let window = app.get_window("main").unwrap();
                    if let Err(e) = pm.lock().unwrap().start_background() {
                        dialog::message(Some(&window), e.to_string(), e.to_string());
                    }
                    pm.lock().unwrap().update_menu(app);
                }
                "stop" => {
                    let window = app.get_window("main").unwrap();
                    if let Err(e) = pm.lock().unwrap().stop_background() {
                        dialog::message(Some(&window), e.to_string(), e.to_string());
                    }

                    pm.lock().unwrap().update_menu(app);
                }
                "quit" => {
                    pm.lock().unwrap().stop_background().ok();
                    pm.lock().unwrap().save_config();
                    std::process::exit(0);
                }
                id => {
                    let window = app.get_window("main").unwrap();
                    let uuid = Uuid::parse_str(&id[2..]).expect("Invalid input");
                    // remove or select
                    println!("{}", id);
                    match &id[0..2] {
                        "r-" => {
                            if let Err(e) = pm.lock().unwrap().remove_client(uuid) {
                                dialog::message(Some(&window), "Remove Error", e.to_string());
                            }
                        }
                        "s-" => {
                            if let Err(e) = pm.lock().unwrap().select_client(uuid) {
                                dialog::message(Some(&window), "Select Error", e.to_string());
                            }
                        }
                        _ => (),
                    };
                    pm.lock().unwrap().update_menu(app);
                }
            },
            _ => {}
        }
    })
        .run(tauri::generate_context!("tauri.conf.json"))
        .expect("error while running tauri application");
}
