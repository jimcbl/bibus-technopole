#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod core;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod desktop;
#[cfg(target_os = "linux")]
mod linux;
mod realtime;

use core::{Model, Provider};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Duration;

pub fn worker() -> (SyncSender<()>, Receiver<Result<core::Snapshot, String>>) {
    let (refresh, commands) = mpsc::sync_channel(1);
    let (results, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut provider = Provider::default();
        loop {
            if results.send(provider.fetch()).is_err() {
                break;
            }
            match commands.recv_timeout(Duration::from_secs(60)) {
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                _ => while commands.try_recv().is_ok() {},
            }
        }
    });
    (refresh, receiver)
}

fn main() {
    // Also useful for checking the feeds without registering a tray icon.
    if std::env::args().any(|arg| arg == "--check") {
        let mut model = Model::new();
        model.receive(Provider::default().fetch());
        for line in model.lines(chrono::Utc::now().timestamp()) {
            println!("{line}");
        }
        if model.snapshot.is_none() {
            std::process::exit(1);
        }
        return;
    }
    #[cfg(target_os = "linux")]
    if let Err(error) = linux::run() {
        eprintln!(
            "Bibus: {error}\n{}",
            Model::new().text(
                "Activez Ubuntu AppIndicators si l’icône n’apparaît pas.",
                "Enable Ubuntu AppIndicators if the icon does not appear."
            )
        );
        std::process::exit(1);
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    desktop::run();
}
