use crate::{
    core::{self, Model},
    worker,
};
use std::time::{Duration, Instant};
use tao::{
    event::{Event, StartCause},
    event_loop::{ControlFlow, EventLoop},
    platform::macos::{ActivationPolicy, EventLoopExtMacOS},
};
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, Submenu},
    Icon, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

struct NativeMenu {
    menu: Menu,
    rows: Vec<MenuItem>,
    showing: usize,
    status: MenuItem,
    refresh: MenuItem,
    source: MenuItem,
    quit: MenuItem,
    more: Submenu,
    english: CheckMenuItem,
    french: CheckMenuItem,
}
impl NativeMenu {
    fn new(model: &Model) -> Self {
        let menu = Menu::new();
        let title = MenuItem::new("2 · Technopôle → Liberté", false, None);
        let status = MenuItem::new("Chargement…", false, None);
        let refresh = MenuItem::new(model.text("Actualiser", "Refresh"), true, None);
        let source = MenuItem::new("Source · Bibus / Brest Métropole", true, None);
        let quit = MenuItem::new("Quitter", true, None);
        let more = Submenu::new(model.text("Plus", "More"), true);
        let english = CheckMenuItem::new("English", true, model.english, None);
        let french = CheckMenuItem::new("Français", true, !model.english, None);
        more.append_items(&[&refresh, &english, &french, &source, &quit])
            .expect("options");
        menu.append_items(&[&title, &status, &more]).expect("menu");
        let rows = (0..3).map(|_| MenuItem::new("", true, None)).collect();
        let mut native = Self {
            menu,
            rows,
            showing: 0,
            status,
            refresh,
            source,
            quit,
            more,
            english,
            french,
        };
        native.update(model);
        native
    }
    fn update(&mut self, model: &Model) {
        let lines = model.lines(chrono::Utc::now().timestamp());
        for (index, line) in lines.iter().enumerate() {
            self.rows[index].set_text(line);
            if index >= self.showing {
                self.menu
                    .insert(&self.rows[index], 1 + index)
                    .expect("insert departure");
            }
        }
        for index in lines.len()..self.showing {
            self.menu
                .remove(&self.rows[index])
                .expect("remove departure");
        }
        self.showing = lines.len();
        self.status.set_text(model.status());
        self.more.set_text(model.text("Plus", "More"));
        self.refresh.set_text(model.text("Actualiser", "Refresh"));
        self.quit.set_text(model.text("Quitter", "Quit"));
        self.english.set_checked(model.english);
        self.french.set_checked(!model.english);
    }
}

pub fn run() {
    let mut event_loop = EventLoop::new();
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    let (refresh, results) = worker();
    let mut model = Model::new();
    let mut native: Option<NativeMenu> = None;
    let mut tray: Option<TrayIcon> = None;
    let proxy = event_loop.create_proxy();
    let click_proxy = proxy.clone();
    let click_refresh = refresh.clone();
    MenuEvent::set_event_handler(Some(move |_| {
        let _ = proxy.send_event(());
    }));
    TrayIconEvent::set_event_handler(Some(move |event| {
        if matches!(
            event,
            TrayIconEvent::Click {
                button_state: MouseButtonState::Up,
                ..
            }
        ) {
            let _ = click_refresh.try_send(());
            let _ = click_proxy.send_event(());
        }
    }));
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(1));
        if let Event::NewEvents(StartCause::Init) = event {
            let menu = NativeMenu::new(&model);
            tray = Some(
                TrayIconBuilder::new()
                    .with_tooltip("Bibus · Technopôle")
                    .with_icon(Icon::from_rgba(core::icon_rgba(), 32, 32).expect("icon"))
                    .with_menu(Box::new(menu.menu.clone()))
                    .with_menu_on_left_click(true)
                    .build()
                    .expect("tray"),
            );
            native = Some(menu);
        }
        let _keep_alive = &tray;
        if let Some(menu) = native.as_mut() {
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id == *menu.quit.id() {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                if event.id == *menu.refresh.id() {
                    let _ = refresh.try_send(());
                }
                if event.id == *menu.source.id() {
                    core::open_source();
                }
                if event.id == *menu.english.id() {
                    model.set_english(true);
                    menu.update(&model);
                }
                if event.id == *menu.french.id() {
                    model.set_english(false);
                    menu.update(&model);
                }
            }
            while let Ok(snapshot) = results.try_recv() {
                model.receive(snapshot);
            }
            if matches!(event, Event::NewEvents(_) | Event::UserEvent(_)) {
                menu.update(&model);
            }
        }
    });
}
