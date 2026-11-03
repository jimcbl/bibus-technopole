use crate::{
    core::{self, Model},
    worker,
};
use ksni::{
    blocking::TrayMethods,
    menu::{CheckmarkItem, StandardItem, SubMenu},
    MenuItem,
};
use std::{sync::mpsc::SyncSender, time::Duration};

struct Tray {
    model: Model,
    refresh: SyncSender<()>,
}
impl ksni::Tray for Tray {
    const MENU_ON_ACTIVATE: bool = true;
    fn id(&self) -> String {
        "bibus-technopole-rust".into()
    }
    fn title(&self) -> String {
        self.model.text(
            "Bibus · Technopôle → centre-ville",
            "Bibus · Technopôle → city centre",
        )
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let mut data = core::icon_rgba();
        for pixel in data.chunks_exact_mut(4) {
            pixel.rotate_right(1);
        }
        vec![ksni::Icon {
            width: 32,
            height: 32,
            data,
        }]
    }
    fn menu_about_to_show(&mut self) {
        let _ = self.refresh.try_send(());
    }
    fn menu(&self) -> Vec<MenuItem<Self>> {
        let label = |text: String, enabled| {
            StandardItem::<Self> {
                label: text,
                enabled,
                ..Default::default()
            }
            .into()
        };
        let mut items = vec![label("2 · Technopôle → Liberté".into(), false)];
        for line in self.model.lines(chrono::Utc::now().timestamp()) {
            items.push(label(line, true));
        }
        items.push(label(self.model.status(), false));
        let more = vec![
            StandardItem {
                label: self.model.text("Actualiser", "Refresh"),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.refresh.try_send(());
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "English".into(),
                checked: self.model.english,
                activate: Box::new(|tray: &mut Self| tray.model.set_english(true)),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Français".into(),
                checked: !self.model.english,
                activate: Box::new(|tray: &mut Self| tray.model.set_english(false)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Source · Bibus / Brest Métropole".into(),
                activate: Box::new(|_| core::open_source()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: self.model.text("Quitter", "Quit"),
                activate: Box::new(|_| std::process::exit(0)),
                ..Default::default()
            }
            .into(),
        ];
        items.push(
            SubMenu {
                label: self.model.text("Plus", "More"),
                submenu: more,
                ..Default::default()
            }
            .into(),
        );
        items
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (refresh, results) = worker();
    let tray = Tray {
        model: Model::new(),
        refresh,
    }
    .spawn()?;
    while !tray.is_closed() {
        match results.recv_timeout(Duration::from_secs(15)) {
            Ok(snapshot) => {
                tray.update(|tray| tray.model.receive(snapshot));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                tray.update(|_| ());
            }
            Err(_) => break,
        }
    }
    Ok(())
}
