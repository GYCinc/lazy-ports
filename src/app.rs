// SPDX-License-Identifier: MIT

use crate::config::{Config, ServiceConfig, parse_address};
use crate::fl;
use crate::ports;
use crate::services::{self, Health};
#[path = "cbar.rs"]
mod cbar;
use cosmic::cosmic_config::{ConfigSet, CosmicConfigEntry};
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{futures, Length, Limits, Subscription, window::Id};
use cosmic::prelude::*;
use cosmic::widget::{self, icon, settings, text_input};
use futures::{SinkExt, StreamExt};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const APP_ID: &str = "io.github.hayron.lazy-ports";

const CORRUPT_CONFIG_MSG: &str = "Service config is corrupt — services are read-only until reset.";

const LED_AMBER: cosmic::iced::Color = cosmic::iced::Color { r: 0.961, g: 0.702, b: 0.263, a: 1.0 };
const LED_CYAN: cosmic::iced::Color = cosmic::iced::Color { r: 0.541, g: 0.812, b: 0.839, a: 1.0 };
const LED_PURPLE: cosmic::iced::Color = cosmic::iced::Color { r: 0.718, g: 0.541, b: 0.839, a: 1.0 };
const LED_GREEN: cosmic::iced::Color = cosmic::iced::Color { r: 0.663, g: 0.839, b: 0.541, a: 1.0 };

fn action(m: Message) -> cosmic::Action<Message> {
    cosmic::Action::from(m)
}

fn status_led<'a, M: 'static>(color: cosmic::iced::Color) -> Element<'a, M> {
    widget::container(
        widget::Space::new()
            .width(Length::Fixed(8.0))
            .height(Length::Fixed(8.0)),
    )
    .width(Length::Fixed(8.0))
    .height(Length::Fixed(8.0))
    .class(cosmic::theme::Container::Custom(Box::new(move |_| {
        cosmic::iced::widget::container::Style {
            background: Some(cosmic::iced::Background::Color(color)),
            border: cosmic::iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    })))
    .into()
}

fn tip<'a, M: 'static>(
    content: impl Into<Element<'a, M>>,
    label: impl Into<String> + 'a,
) -> Element<'a, M> {
    widget::tooltip(
        content,
        widget::Text::new(label.into()).size(11.0),
        widget::tooltip::Position::Bottom,
    )
    .gap(6)
    .padding(6)
    .into()
}

fn opt_str(v: &str) -> Option<String> {
    let t = v.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn parse_services(json: &str) -> Result<Vec<ServiceConfig>, serde_json::Error> {
    if json.trim().is_empty() {
        Ok(Vec::new())
    } else {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_services_json_is_valid() {
        assert!(parse_services("").unwrap().is_empty());
        assert!(parse_services("  \n ").unwrap().is_empty());
    }

    #[test]
    fn corrupt_services_json_is_err() {
        assert!(parse_services("{not json").is_err());
    }
}

#[derive(Debug, Clone, PartialEq)]
enum SvcState {
    Stopped,
    Starting,
    Running,
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    DestroyPopup(Id),
    Tick,
    PortsRefreshed(Vec<ports::ListeningPort>),
    HealthBatch(Vec<(String, Health)>),
    StartService(String),
    StartResult(String, Result<u32, String>),
    StopService(String),
    StopResult(String),
    Exited(String, Option<i32>),
    Open(String),
    KillPort(i32),
    KillProcessGroup(Vec<i32>),
    PinPort(ports::ListeningPort),
    #[allow(dead_code)]
    IgnorePort(ports::ListeningPort),
    ClearIgnored,
    ToggleGroup,
    ToggleColloquial,
    ToggleHideSystem,
    ToggleShowPorts,
    ToggleShowInactive,
    ToggleSettings,
    ToggleExpandService(String),
    ToggleReservePort(String),
    RestartService(String),
    RestartResult(String, Result<(u32, bool), String>),
    ShowAdd,
    HideAdd,
    NameInput(String),
    AddressInput(String),
    CmdInput(String),
    DirInput(String),
    ReservedInput(bool),
    SaveService,
    DeleteService(String),
    MoveServiceUp(String),
    MoveServiceDown(String),
    ResetCorruptConfig,
    EditService(String),
    ShowLogs(String),
    HideLogs,
    LogText { id: String, text: String },
    UpdateConfig(Config),
    QuitApplet,
    FilterInput(String),
    FilterTo(String),
    SetRefresh(u64),
    Event(services::Event),
}

pub struct AppModel {
    core: cosmic::Core,
    popup: Option<Id>,
    popup_closing: Option<Id>,
    config: Config,
    config_ctx: Option<cosmic::cosmic_config::Config>,
    services: Vec<ServiceConfig>,
    services_corrupt: bool,
    ports: Vec<ports::ListeningPort>,
    states: HashMap<String, SvcState>,
    health: HashMap<String, Health>,
    stopping: HashSet<String>,
    logs_open: Option<String>,
    log_text: String,
    show_add: bool,
    show_settings: bool,
    expanded_service: Option<String>,
    show_inactive: bool,
    group_ports: bool,
    edit_id: Option<String>,
    in_name: String,
    in_address: String,
    in_cmd: String,
    in_dir: String,
    in_reserved: bool,
    error: Option<String>,
    filter: String,
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let (config, config_ctx) = match cosmic::cosmic_config::Config::new(APP_ID, Config::VERSION) {
            Ok(ctx) => match Config::get_entry(&ctx) {
                Ok(config) => (config, Some(ctx)),
                Err((_errors, config)) => (config, Some(ctx)),
            },
            Err(_) => (Config::default(), None),
        };
        let (services, services_corrupt) = match parse_services(&config.services_json) {
            Ok(services) => (services, false),
            Err(_) => (Vec::new(), true),
        };

        let mut app = AppModel {
            core,
            popup: None,
            popup_closing: None,
            config,
            config_ctx,
            services,
            services_corrupt,
            ports: Vec::new(),
            states: HashMap::new(),
            health: HashMap::new(),
            stopping: HashSet::new(),
            logs_open: None,
            log_text: String::new(),
            show_add: false,
            show_settings: false,
            expanded_service: None,
            show_inactive: false,
            group_ports: true,
            edit_id: None,
            in_name: String::new(),
            in_address: String::new(),
            in_cmd: String::new(),
            in_dir: String::new(),
            in_reserved: false,
            error: services_corrupt.then(|| CORRUPT_CONFIG_MSG.to_string()),
            filter: String::new(),
        };

        if !app.services_corrupt
            && app.services.is_empty()
            && let Some(imported) = cbar::load_services()
        {
            app.services = imported;
            if let Err(e) = app.persist_services() {
                app.error = Some(e);
            }
        }

        services::sync_reservations(&app.services);

        (
            app,
            Task::perform(ports::scan(), |v| action(Message::PortsRefreshed(v))),
        )
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn view(&self) -> Element<'_, Self::Message> {
        self.core
            .applet
            .icon_button("io.github.hayron.lazy-ports-symbolic")
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        // 1. Log viewer
        if let Some(log_id) = self.logs_open.as_ref() {
            let name = self
                .services
                .iter()
                .find(|s| &s.id == log_id)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| log_id.clone());

            let header = widget::row::with_capacity(2)
                .align_y(cosmic::iced::Alignment::Center)
                .spacing(8.0)
                .width(Length::Fill)
                .push(
                    widget::button::icon(icon::from_name("go-previous-symbolic").size(16))
                        .on_press(Message::HideLogs),
                )
                .push(widget::Text::new(format!("{name} Logs")).size(16.0).font(cosmic::font::semibold()));

            let log_text = if self.log_text.is_empty() {
                fl!("log-empty")
            } else {
                self.log_text.clone()
            };

            let content = widget::column::with_capacity(2)
                .spacing(8.0)
                .width(Length::Fill)
                .push(header)
                .push(
                    widget::scrollable(
                        widget::Text::new(log_text).size(12.0).width(Length::Fill),
                    )
                    .height(Length::Fixed(400.0)),
                );

            return self.core.applet.popup_container(content).into();
        }

        // 2. Settings
        if self.show_settings {
            let header = widget::row::with_capacity(2)
                .align_y(cosmic::iced::Alignment::Center)
                .spacing(8.0)
                .width(Length::Fill)
                .push(
                    widget::button::icon(icon::from_name("go-previous-symbolic").size(16))
                        .on_press(Message::ToggleSettings),
                )
                .push(widget::Text::new(fl!("preferences")).size(16.0).font(cosmic::font::semibold()));

            let mut pref = settings::section();
            pref = pref.add(
                settings::item::builder("Refresh Rate")
                    .description("Background polling frequency for listening ports")
                    .control(
                        widget::row::with_capacity(3)
                            .spacing(4.0)
                            .push(
                                widget::button::standard(fl!("refresh-mode-realtime"))
                                    .class(if self.config.refresh_secs == 1 {
                                        cosmic::theme::Button::Suggested
                                    } else {
                                        cosmic::theme::Button::Standard
                                    })
                                    .on_press(Message::SetRefresh(1)),
                            )
                            .push(
                                widget::button::standard(fl!("refresh-mode-balanced"))
                                    .class(if self.config.refresh_interval() == 2 {
                                        cosmic::theme::Button::Suggested
                                    } else {
                                        cosmic::theme::Button::Standard
                                    })
                                    .on_press(Message::SetRefresh(2)),
                            )
                            .push(
                                widget::button::standard(fl!("refresh-mode-low"))
                                    .class(if self.config.refresh_secs == 10 {
                                        cosmic::theme::Button::Suggested
                                    } else {
                                        cosmic::theme::Button::Standard
                                    })
                                    .on_press(Message::SetRefresh(10)),
                            ),
                    ),
            );
            pref = pref.add(
                settings::item::builder(fl!("group-ports"))
                    .description("Combine multi-port sockets from the same process")
                    .control(widget::toggler(self.group_ports).on_toggle(|_| Message::ToggleGroup)),
            );
            pref = pref.add(
                settings::item::builder(fl!("hide-system"))
                    .description("Filter out system daemons (systemd, cups, avahi, etc.)")
                    .control(widget::toggler(self.config.hide_system).on_toggle(|_| Message::ToggleHideSystem)),
            );
            pref = pref.add(
                settings::item::builder("Colloquial Slang")
                    .description("Show playful status labels (PIMPIN', SLEEPIN', COOKED)")
                    .control(widget::toggler(self.config.colloquial).on_toggle(|_| Message::ToggleColloquial)),
            );

            let ignored = self.config.ignored_processes();
            if !ignored.is_empty() {
                pref = pref.add(
                    settings::item::builder(format!("Ignored Processes ({})", ignored.len()))
                        .description(ignored.join(", "))
                        .control(
                            widget::button::destructive(fl!("clear-ignored")).on_press(Message::ClearIgnored),
                        ),
                );
            }

            let about = widget::Text::new("Lazy Ports v0.1.0").size(11.0);

            let content = widget::column::with_capacity(4)
                .spacing(4.0)
                .width(Length::Fill)
                .push(header)
                .push(pref)
                .push(about);

            return self.core.applet.popup_container(content).into();
        }

        // 3. Add / Edit form
        if self.show_add {
            let title = if self.edit_id.is_some() {
                fl!("edit-service")
            } else {
                fl!("add-service")
            };

            let header = widget::row::with_capacity(2)
                .align_y(cosmic::iced::Alignment::Center)
                .spacing(8.0)
                .width(Length::Fill)
                .push(
                    widget::button::icon(icon::from_name("go-previous-symbolic").size(16))
                        .on_press(Message::HideAdd),
                )
                .push(widget::Text::new(title).size(16.0).font(cosmic::font::semibold()));

            let mut form = settings::section();
            form = form.add(
                settings::item::builder(fl!("name")).control(
                    text_input(fl!("name-placeholder"), &self.in_name)
                        .on_input(Message::NameInput)
                        .width(Length::Fill),
                ),
            );
            form = form.add(
                settings::item::builder(fl!("address")).control(
                    text_input(fl!("address-placeholder"), &self.in_address)
                        .on_input(Message::AddressInput)
                        .width(Length::Fill),
                ),
            );
            form = form.add(
                settings::item::builder(fl!("start-command")).control(
                    text_input(fl!("start-command-placeholder"), &self.in_cmd)
                        .on_input(Message::CmdInput)
                        .width(Length::Fill),
                ),
            );
            form = form.add(
                settings::item::builder(fl!("working-directory")).control(
                    text_input(fl!("working-directory-placeholder"), &self.in_dir)
                        .on_input(Message::DirInput)
                        .width(Length::Fill),
                ),
            );
            form = form.add(
                settings::item::builder(fl!("reserve-port"))
                    .description("Hold this port with a silent tcp socket when offline")
                    .control(widget::toggler(self.in_reserved).on_toggle(Message::ReservedInput)),
            );

            let mut buttons = widget::row::with_capacity(3)
                .spacing(8.0)
                .width(Length::Fill)
                .push(
                    widget::button::standard(fl!("save"))
                        .width(Length::Fill)
                        .on_press(Message::SaveService),
                )
                .push(
                    widget::button::standard(fl!("cancel"))
                        .width(Length::Fill)
                        .on_press(Message::HideAdd),
                );
            if let Some(edit_id) = &self.edit_id {
                buttons = buttons.push(
                    widget::button::destructive(fl!("delete"))
                        .width(Length::Fill)
                        .on_press(Message::DeleteService(edit_id.clone())),
                );
            }

            let content = widget::column::with_capacity(3)
                .spacing(8.0)
                .width(Length::Fill)
                .push(header)
                .push(form)
                .push(buttons);

            return self.core.applet.popup_container(content).into();
        }

        // 4. Main view
        let filter_field = text_input("Filter…", &self.filter)
            .on_input(Message::FilterInput)
            .width(Length::Fill);

        let header = widget::row::with_capacity(2)
            .align_y(cosmic::iced::Alignment::Center)
            .spacing(8.0)
            .width(Length::Fill)
            .push(filter_field)
            .push(
                widget::button::icon(icon::from_name("emblem-system-symbolic").size(16))
                    .on_press(Message::ToggleSettings),
            );

        let mut scroll_content = widget::column::with_capacity(20).spacing(10.0).width(Length::Fill);

        if let Some(error) = &self.error {
            scroll_content = scroll_content.push(widget::Text::new(error.clone()).size(11.0));
        }
        if self.services_corrupt {
            scroll_content = scroll_content.push(
                widget::button::standard(String::from("Reset config"))
                    .on_press(Message::ResetCorruptConfig),
            );
        }

        let filter_query = self.filter.trim().to_lowercase();
        let matches_filter = |name: &str, needle: &str| -> bool {
            needle.is_empty() || name.to_lowercase().contains(needle)
        };

        let (active_services, inactive_services): (Vec<&ServiceConfig>, Vec<&ServiceConfig>) = self
            .services
            .iter()
            .filter(|s| matches_filter(&format!("{} {}", s.name, s.address), &filter_query))
            .partition(|s| {
                let (st, _) = self.service_status(s, &self.ports);
                matches!(st, SvcState::Running | SvcState::Starting)
            });

        if !active_services.is_empty() {
            let mut list = widget::list_column();
            for cfg in &active_services {
                list = list.add(self.render_service_row(cfg));
            }
            scroll_content = scroll_content.push(list);
        } else if self.services.is_empty() {
            scroll_content = scroll_content.push(self.empty_state(fl!("no-services")));
        } else if filter_query.is_empty() {
            scroll_content = scroll_content.push(self.empty_state(fl!("no-active-services")));
        } else {
            scroll_content = scroll_content.push(self.empty_state("NO ACTIVE SOCKETS MATCHING FILTER".to_string()));
        }

        if !inactive_services.is_empty() {
            scroll_content = scroll_content.push(self.section_rail(
                &format!("{} ({})", fl!("inactive-services"), inactive_services.len()),
                self.show_inactive,
                Message::ToggleShowInactive,
            ));
            if self.show_inactive {
                let mut list = widget::list_column();
                for cfg in &inactive_services {
                    list = list.add(self.render_inactive_service_row(cfg));
                }
                scroll_content = scroll_content.push(list);
            }
        }

        let ignored = self.config.ignored_processes();
        let pinned_ports: HashSet<u16> = self
            .services
            .iter()
            .filter_map(|s| s.host_port().map(|(_, p)| p))
            .collect();
        let unpinned: Vec<&ports::ListeningPort> = self
            .ports
            .iter()
            .filter(|p| !pinned_ports.contains(&p.port))
            .filter(|p| !ignored.contains(&p.process))
            .filter(|p| !services::is_port_reserved(p.port))
            .filter(|p| !self.config.hide_system || !p.is_system)
            .filter(|p| {
                matches_filter(
                    &format!("{} {} {}", p.display_name, p.process, p.port),
                    &filter_query,
                )
            })
            .collect();

        let unpinned_count = unpinned.len();

        scroll_content = scroll_content.push(self.section_rail(
            &format!("{} ({})", fl!("discovered-ports"), unpinned_count),
            self.config.show_ports,
            Message::ToggleShowPorts,
        ));

        if self.config.show_ports {
            if unpinned.is_empty() {
                scroll_content = scroll_content.push(self.empty_state(fl!("no-ports")));
            } else if self.group_ports {
                let mut list = widget::list_column();
                let groups = ports::group_by_process_refs(&unpinned);
                for group in groups.iter().take(15) {
                    let port_nums: Vec<u16> = group.ports.iter().map(|p| p.port).collect();
                    let first_port = group.ports.first().copied().cloned();
                    list = list.add(self.render_discovered_group_row(
                        format!("group-{}-{}", group.process, group.display_name),
                        group.display_name.clone(),
                        group.process.clone(),
                        group.framework.clone(),
                        port_nums,
                        group.pids.clone(),
                        first_port,
                    ));
                }
                scroll_content = scroll_content.push(list);
            } else {
                let mut list = widget::list_column();
                for p in unpinned.iter().take(20) {
                    list = list.add(self.render_discovered_port_row(p));
                }
                scroll_content = scroll_content.push(list);
            }
        }

        let footer = widget::row::with_capacity(4)
            .spacing(8.0)
            .align_y(cosmic::iced::Alignment::Center)
            .width(Length::Fill)
            .push(widget::button::standard("Rescan").on_press(Message::Tick))
            .push(widget::button::suggested("Add Service").on_press(Message::ShowAdd))
            .push(widget::Space::new().width(Length::Fill))
            .push(widget::button::text("Quit").on_press(Message::QuitApplet));

        let main_layout = widget::column::with_capacity(3)
            .spacing(10.0)
            .width(Length::Fill)
            .push(header)
            .push(widget::scrollable(scroll_content).height(Length::Fill))
            .push(footer);

        self.core.applet.popup_container(main_layout).into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let subs = vec![
            Subscription::run(|| {
                cosmic::iced::stream::channel(
                    16,
                    |mut output: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                        let (tx, mut rx) = futures::channel::mpsc::unbounded::<services::Event>();
                        let _ = services::set_event_tx(tx);
                        while let Some(ev) = rx.next().await {
                            _ = output.send(Message::Event(ev)).await;
                        }
                    },
                )
            }),
            Subscription::run_with(
                (self.popup.is_some(), self.config.refresh_interval()),
                |k| {
                    let (open, secs) = *k;
                    cosmic::iced::stream::channel(
                        4,
                        move |mut output: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                            if !open {
                                cosmic::iced::futures::future::pending::<()>().await;
                            }
                            let mut interval = tokio::time::interval(Duration::from_secs(secs));
                            interval
                                .set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                            loop {
                                interval.tick().await;
                                _ = output.send(Message::Tick).await;
                            }
                        },
                    )
                },
            ),
            self.core()
                .watch_config::<Config>(APP_ID)
                .map(|update| Message::UpdateConfig(update.config)),
        ];

        Subscription::batch(subs)
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    self.show_inactive = false;
                    self.logs_open = None;
                    self.popup_closing = Some(p);
                    Task::perform(
                        async {
                            tokio::time::sleep(Duration::from_millis(120)).await;
                        },
                        move |_| action(Message::DestroyPopup(p)),
                    )
                } else if self.popup_closing.is_some() {
                    Task::none()
                } else {
                    self.show_inactive = false;
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(372.0)
                        .min_width(300.0)
                        .min_height(200.0)
                        .max_height(800.0);
                    get_popup(popup_settings)
                };
            }
            Message::DestroyPopup(id) => {
                if self.popup_closing == Some(id) {
                    self.popup_closing = None;
                }
                return destroy_popup(id);
            }
            Message::PopupClosed(id) => {
                if self.popup_closing == Some(id) {
                    self.popup_closing = None;
                }
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                    self.show_inactive = false;
                    self.logs_open = None;
                }
            }
            Message::Tick => {
                let svcs = self.services.clone();
                let task = Task::batch(vec![
                    Task::perform(ports::scan(), |v| action(Message::PortsRefreshed(v))),
                    Task::perform(services::health_all(svcs), |v| {
                        action(Message::HealthBatch(v))
                    }),
                ]);
                if let Some(log_id) = self.logs_open.clone() {
                    let path = services::log_file_path(&log_id)
                        .to_string_lossy()
                        .into_owned();
                    return Task::batch(vec![
                        task,
                        Task::perform(services::read_tail(path), move |t| {
                            action(Message::LogText {
                                id: log_id.clone(),
                                text: t,
                            })
                        }),
                    ]);
                }
                return task;
            }
            Message::PortsRefreshed(list) => self.ports = list,
            Message::HealthBatch(results) => {
                for (id, health) in results {
                    self.health.insert(id, health);
                }
            }
            Message::StartService(id) => {
                if let Some(cfg) = self.services.iter().find(|s| s.id == id).cloned() {
                    self.states.insert(id.clone(), SvcState::Starting);
                    return Task::perform(services::spawn_managed(cfg), move |result| {
                        action(Message::StartResult(id.clone(), result))
                    });
                }
            }
            Message::StartResult(id, result) => match result {
                Ok(_) => {
                    if services::is_managed(&id) {
                        self.states.insert(id, SvcState::Running);
                        self.error = None;
                    }
                    services::sync_reservations(&self.services);
                }
                Err(e) => {
                    self.states.insert(id.clone(), SvcState::Failed(e.clone()));
                    self.error = Some(e);
                    services::sync_reservations(&self.services);
                }
            },
            Message::ToggleGroup => {
                self.group_ports = !self.group_ports;
            }
            Message::ToggleSettings => {
                self.show_settings = !self.show_settings;
            }
            Message::ToggleShowPorts => {
                self.config.show_ports = !self.config.show_ports;
                if let Some(ctx) = &self.config_ctx {
                    let _ = ctx.set("show_ports", self.config.show_ports);
                }
            }
            Message::ToggleShowInactive => {
                self.show_inactive = !self.show_inactive;
            }
            Message::ToggleExpandService(id) => {
                if self.expanded_service.as_deref() == Some(&id) {
                    self.expanded_service = None;
                } else {
                    self.expanded_service = Some(id);
                }
            }
            Message::ToggleReservePort(id) => {
                if let Some(pos) = self.services.iter().position(|s| s.id == id) {
                    self.services[pos].reserved = !self.services[pos].reserved;
                    if let Err(e) = self.persist_services() {
                        self.error = Some(e);
                    }
                    services::sync_reservations(&self.services);
                }
            }
            Message::KillProcessGroup(pids) => {
                let tasks: Vec<_> = pids
                    .into_iter()
                    .map(|pid| Task::perform(services::kill_pid(pid), |_| action(Message::Tick)))
                    .collect();
                return Task::batch(tasks);
            }
            Message::IgnorePort(p) => {
                let mut ignored = self.config.ignored_processes();
                if !ignored.contains(&p.process) {
                    ignored.push(p.process.clone());
                    let json = serde_json::to_string(&ignored).unwrap_or_default();
                    self.config.ignored_json = json.clone();
                    if let Some(ctx) = &self.config_ctx {
                        let _ = ctx.set("ignored_json", json);
                    }
                }
            }
            Message::ClearIgnored => {
                self.config.ignored_json = "[]".to_string();
                if let Some(ctx) = &self.config_ctx {
                    let _ = ctx.set("ignored_json", "[]".to_string());
                }
            }
            Message::RestartService(id) => {
                if let Some(cfg) = self.services.iter().find(|s| s.id == id).cloned() {
                    self.states.insert(id.clone(), SvcState::Starting);
                    self.stopping.insert(id.clone());
                    return Task::perform(services::restart_managed(cfg), move |result| {
                        action(Message::RestartResult(id.clone(), result))
                    });
                }
            }
            Message::RestartResult(id, result) => match result {
                Ok((_pgid, stopped_old)) => {
                    if services::is_managed(&id) {
                        self.states.insert(id.clone(), SvcState::Running);
                        self.error = None;
                    }
                    if !stopped_old {
                        self.stopping.remove(&id);
                    }
                    services::sync_reservations(&self.services);
                }
                Err(e) => {
                    self.states.insert(id.clone(), SvcState::Failed(e.clone()));
                    self.error = Some(e);
                    self.stopping.remove(&id);
                    services::sync_reservations(&self.services);
                }
            },
            Message::StopService(id) => {
                self.stopping.insert(id.clone());
                return Task::perform(services::stop_managed(id.clone()), move |_| {
                    action(Message::StopResult(id))
                });
            }
            Message::StopResult(id) => {
                self.stopping.remove(&id);
                self.states.insert(id, SvcState::Stopped);
                services::sync_reservations(&self.services);
            }
            Message::Exited(id, code) => {
                let crashed = !self.stopping.remove(&id)
                    && code != Some(0)
                    && matches!(
                        self.states.get(&id),
                        Some(SvcState::Running | SvcState::Starting)
                    );
                if !matches!(self.states.get(&id), Some(SvcState::Failed(_))) {
                    self.states.insert(id.clone(), SvcState::Stopped);
                }
                services::sync_reservations(&self.services);
                if crashed && let Some(cfg) = self.services.iter().find(|s| s.id == id).cloned() {
                    return Task::perform(services::notify_crash(cfg.name, code), |_| {
                        action(Message::Tick)
                    });
                }
            }
            Message::Event(services::Event::Exited { id, code }) => {
                return self.update(Message::Exited(id, code));
            }
            Message::Open(url) => {
                if let Err(e) = open::that_detached(&url) {
                    self.error = Some(format!("failed to open {url}: {e}"));
                }
            }
            Message::QuitApplet => {
                std::process::exit(0);
            }
            Message::KillPort(pid) => {
                return Task::perform(services::kill_pid(pid), move |_| action(Message::Tick));
            }
            Message::PinPort(p) => {
                self.show_add = true;
                self.edit_id = None;
                self.in_name = p.process.clone();
                self.in_address = format!("http://localhost:{}", p.port);
                self.in_cmd = ports::process_cmdline(p.pid).unwrap_or_default();
                self.in_dir = ports::process_cwd(p.pid).unwrap_or_default();
                self.in_reserved = false;
            }
            Message::ShowAdd => {
                self.show_add = true;
                self.edit_id = None;
                self.in_name.clear();
                self.in_address.clear();
                self.in_cmd.clear();
                self.in_dir.clear();
                self.in_reserved = false;
            }
            Message::HideAdd => {
                self.show_add = false;
                self.edit_id = None;
                self.in_reserved = false;
            }
            Message::ToggleHideSystem => {
                self.config.hide_system = !self.config.hide_system;
                if let Some(ctx) = &self.config_ctx {
                    let _ = ctx.set("hide_system", self.config.hide_system);
                }
            }
            Message::ToggleColloquial => {
                self.config.colloquial = !self.config.colloquial;
                if let Some(ctx) = &self.config_ctx {
                    let _ = ctx.set("colloquial", self.config.colloquial);
                }
            }
            Message::SetRefresh(secs) => {
                self.config.refresh_secs = secs;
                if let Some(ctx) = &self.config_ctx {
                    let _ = ctx.set("refresh_secs", secs);
                }
            }
            Message::FilterInput(v) => self.filter = v,
            Message::FilterTo(v) => self.filter = v,
            Message::NameInput(v) => self.in_name = v,
            Message::AddressInput(v) => self.in_address = v,
            Message::CmdInput(v) => self.in_cmd = v,
            Message::DirInput(v) => self.in_dir = v,
            Message::ReservedInput(v) => self.in_reserved = v,
            Message::SaveService => {
                if let Err(err) = self.save_service() {
                    self.error = Some(err);
                }
            }
            Message::DeleteService(id) => {
                self.services.retain(|s| s.id != id);
                self.states.remove(&id);
                self.health.remove(&id);
                if self.logs_open.as_deref() == Some(id.as_str()) {
                    self.logs_open = None;
                }
                if let Err(err) = self.persist_services() {
                    self.error = Some(err);
                }
                services::sync_reservations(&self.services);
                return Task::perform(services::stop_managed(id), |_| action(Message::Tick));
            }
            Message::MoveServiceUp(id) => {
                if let Some(pos) = self.services.iter().position(|s| s.id == id) {
                    if pos > 0 {
                        self.services.swap(pos, pos - 1);
                        if let Err(err) = self.persist_services() {
                            self.error = Some(err);
                        }
                    }
                }
            }
            Message::MoveServiceDown(id) => {
                if let Some(pos) = self.services.iter().position(|s| s.id == id) {
                    if pos + 1 < self.services.len() {
                        self.services.swap(pos, pos + 1);
                        if let Err(err) = self.persist_services() {
                            self.error = Some(err);
                        }
                    }
                }
            }
            Message::ResetCorruptConfig => {
                self.services_corrupt = false;
                self.error = None;
                if let Err(err) = self.persist_services() {
                    self.error = Some(err);
                }
                services::sync_reservations(&self.services);
            }
            Message::EditService(id) => {
                if let Some(cfg) = self.services.iter().find(|s| s.id == id).cloned() {
                    self.show_add = true;
                    self.edit_id = Some(cfg.id.clone());
                    self.in_name = cfg.name.clone();
                    self.in_address = cfg.address.clone();
                    self.in_cmd = cfg.start_command.clone().unwrap_or_default();
                    self.in_dir = cfg.working_dir.clone().unwrap_or_default();
                    self.in_reserved = cfg.reserved;
                }
            }
            Message::ShowLogs(id) => {
                self.logs_open = Some(id.clone());
                let path = services::log_file_path(&id).to_string_lossy().into_owned();
                return Task::perform(services::read_tail(path), move |t| {
                    action(Message::LogText {
                        id: id.clone(),
                        text: t,
                    })
                });
            }
            Message::HideLogs => {
                self.logs_open = None;
                self.log_text.clear();
            }
            Message::LogText { id, text } => {
                if self.logs_open.as_deref() == Some(&id) {
                    self.log_text = text;
                }
            }
            Message::UpdateConfig(config) => {
                match parse_services(&config.services_json) {
                    Ok(services) => {
                        self.services = services;
                        self.services_corrupt = false;
                    }
                    Err(_) => {
                        self.services = Vec::new();
                        self.services_corrupt = true;
                        self.error = Some(CORRUPT_CONFIG_MSG.to_string());
                    }
                }
                self.config = config;
            }
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

impl AppModel {
    fn empty_state<'a>(&'a self, msg: String) -> Element<'a, Message> {
        widget::list_column()
            .add(
                widget::container(widget::Text::new(msg).size(13.0))
                    .padding([14, 16])
                    .width(Length::Fill)
                    .center_x(Length::Fill),
            )
            .into()
    }

    fn section_rail(
        &self,
        title: &str,
        open: bool,
        toggle: Message,
    ) -> Element<'_, Message> {
        let icon_name = if open { "go-up-symbolic" } else { "go-down-symbolic" };
        widget::row::with_capacity(3)
            .align_y(cosmic::iced::Alignment::Center)
            .spacing(8.0)
            .width(Length::Fill)
            .push(
                widget::Text::new(title.to_string())
                    .size(13.0)
                    .font(cosmic::font::semibold())
                    .width(Length::Fill),
            )
            .push(
                widget::button::icon(icon::from_name(icon_name).size(14))
                    .on_press(toggle),
            )
            .into()
    }

    fn save_service(&mut self) -> Result<(), String> {
        let name = self.in_name.trim().to_string();
        if name.is_empty() {
            return Err(fl!("error-name-required"));
        }

        let raw = self.in_address.trim().to_string();
        let normalized = if raw.is_empty() {
            return Err(fl!("error-invalid-address"));
        } else if raw.contains("://") {
            raw
        } else {
            format!("http://{raw}")
        };

        let Some((_, port, _, _)) = parse_address(&normalized) else {
            return Err(fl!("error-missing-port"));
        };

        let editing_other = |s: &ServiceConfig| Some(&s.id) != self.edit_id.as_ref();
        if let Some(other) = self
            .services
            .iter()
            .filter(|s| editing_other(s))
            .find(|s| s.host_port().map(|(_, p)| p) == Some(port))
        {
            return Err(fl!(
                "error-duplicate-port",
                port = format!("{port} ({})", other.name)
            ));
        }

        let cfg = ServiceConfig {
            id: self
                .edit_id
                .clone()
                .unwrap_or_else(|| format!("service-{port}")),
            name,
            address: normalized,
            start_command: opt_str(&self.in_cmd),
            working_dir: opt_str(&self.in_dir),
            reserved: self.in_reserved,
        };

        if let Some(edit_id) = &self.edit_id {
            if let Some(pos) = self.services.iter().position(|s| &s.id == edit_id) {
                self.services[pos] = cfg;
            } else {
                self.services.push(cfg);
            }
        } else {
            self.services.push(cfg);
        }

        self.persist_services()?;
        services::sync_reservations(&self.services);
        self.show_add = false;
        self.edit_id = None;
        self.in_name.clear();
        self.in_address.clear();
        self.in_cmd.clear();
        self.in_dir.clear();
        self.in_reserved = false;
        Ok(())
    }

    fn matching_port<'a>(
        &self,
        cfg: &ServiceConfig,
        ports: &'a [ports::ListeningPort],
    ) -> Option<&'a ports::ListeningPort> {
        let (_, port) = cfg.host_port()?;
        ports.iter().find(|p| p.port == port)
    }

    fn service_status<'a>(
        &self,
        cfg: &ServiceConfig,
        ports: &'a [ports::ListeningPort],
    ) -> (SvcState, Option<&'a ports::ListeningPort>) {
        let port_num = cfg.host_port().map(|(_, p)| p).unwrap_or(0);
        let is_reserved = cfg.reserved && services::is_port_reserved(port_num);

        if let Some(state) = self.states.get(&cfg.id) {
            if matches!(state, SvcState::Starting) {
                return (SvcState::Starting, self.matching_port(cfg, ports));
            }
            if matches!(state, SvcState::Running) {
                return (SvcState::Running, self.matching_port(cfg, ports));
            }
        }
        if !is_reserved && let Some(lp) = self.matching_port(cfg, ports) {
            return (SvcState::Running, Some(lp));
        }
        if let Some(state) = self.states.get(&cfg.id)
            && matches!(state, SvcState::Failed(_))
        {
            return (state.clone(), None);
        }
        (SvcState::Stopped, None)
    }

    fn persist_services(&mut self) -> Result<(), String> {
        if self.services_corrupt {
            return Err(CORRUPT_CONFIG_MSG.to_string());
        }
        let json = serde_json::to_string(&self.services).unwrap_or_default();
        self.config.services_json = json.clone();
        match &self.config_ctx {
            Some(ctx) => {
                ctx.set("services_json", json)
                    .map_err(|e| format!("config write failed: {e}"))?;
            }
            None => return Err("config context unavailable".to_string()),
        }
        Ok(())
    }

    fn lingo(&self, straight: impl Into<String>, slang: &str) -> String {
        if self.config.colloquial {
            slang.to_string()
        } else {
            straight.into()
        }
    }

    fn render_service_row<'a>(&'a self, cfg: &'a ServiceConfig) -> Element<'a, Message> {
        let (state, matching_lp) = self.service_status(cfg, &self.ports);
        let is_expanded = self.expanded_service.as_deref() == Some(&cfg.id);
        let port_num = cfg.host_port().map(|(_, p)| p).unwrap_or(0);
        let is_reserved = cfg.reserved && services::is_port_reserved(port_num);
        let is_running = matches!(state, SvcState::Running | SvcState::Starting);

        let led_color = match &state {
            SvcState::Running | SvcState::Starting => LED_GREEN,
            SvcState::Failed(_) => LED_AMBER,
            SvcState::Stopped => {
                if is_reserved {
                    LED_CYAN
                } else {
                    LED_PURPLE
                }
            }
        };

        let state_label = match &state {
            SvcState::Running => match matching_lp {
                Some(lp) => format!("PID {}", lp.pid),
                None => self.lingo("RUNNING".to_string(), "PIMPIN'"),
            },
            SvcState::Starting => self.lingo("STARTING…".to_string(), "WAKIN' UP"),
            SvcState::Stopped => {
                if is_reserved {
                    "RESERVED".to_string()
                } else {
                    self.lingo("STOPPED".to_string(), "SLEEPIN'")
                }
            }
            SvcState::Failed(e) => {
                let short: String = e.chars().take(8).collect();
                let dots = if e.chars().count() > 8 { "…" } else { "" };
                self.lingo(
                    format!("BROKE: {short}{dots}"),
                    &format!("COOKED: {short}{dots}"),
                )
            }
        };

        let mut actions = widget::row::with_capacity(3)
            .spacing(4.0)
            .align_y(cosmic::iced::Alignment::Center);

        if is_running {
            let stop_msg = if services::is_managed(&cfg.id) {
                Message::StopService(cfg.id.clone())
            } else if let Some(lp) = matching_lp {
                Message::KillPort(lp.pid)
            } else {
                Message::StopService(cfg.id.clone())
            };
            actions = actions.push(tip(
                widget::button::icon(icon::from_name("media-playback-stop-symbolic").size(14))
                    .on_press(stop_msg),
                "Stop process",
            ));
        } else if cfg.can_start() {
            actions = actions.push(tip(
                widget::button::icon(icon::from_name("media-playback-start-symbolic").size(14))
                    .on_press(Message::StartService(cfg.id.clone())),
                "Start service",
            ));
        }

        let expand_icon = if is_expanded { "go-up-symbolic" } else { "go-down-symbolic" };
        actions = actions.push(tip(
            widget::button::icon(icon::from_name(expand_icon).size(14))
                .on_press(Message::ToggleExpandService(cfg.id.clone())),
            "Details",
        ));

        let title_row = widget::row::with_capacity(3)
            .align_y(cosmic::iced::Alignment::Center)
            .spacing(8.0)
            .width(Length::Fill)
            .push(status_led(led_color))
            .push(
                widget::button::text(cfg.name.clone())
                    .on_press(Message::Open(cfg.address.clone())),
            )
            .push(widget::Space::new().width(Length::Fill))
            .push(widget::Text::new(state_label).size(11.0))
            .push(actions);

        let port_text = if port_num > 0 {
            format!("localhost:{port_num}")
        } else {
            cfg.address.clone()
        };

        let detail_row = widget::row::with_capacity(2)
            .align_y(cosmic::iced::Alignment::Center)
            .spacing(8.0)
            .width(Length::Fill)
            .push(widget::Text::new(port_text).size(12.0).width(Length::Fill));

        let mut content = widget::column::with_capacity(3)
            .spacing(4.0)
            .width(Length::Fill)
            .push(title_row)
            .push(detail_row);

        if is_expanded {
            if let Some(drawer) = self.render_service_drawer(cfg, is_running) {
                content = content.push(drawer);
            }
        }

        widget::container(content)
            .padding([6, 8])
            .width(Length::Fill)
            .into()
    }

    fn render_service_drawer<'a>(
        &'a self,
        cfg: &'a ServiceConfig,
        is_running: bool,
    ) -> Option<Element<'a, Message>> {
        let mut info = widget::column::with_capacity(3)
            .spacing(2.0)
            .width(Length::Fill);

        if let Some(cmd) = &cfg.start_command {
            info = info.push(
                widget::row::with_capacity(2)
                    .spacing(8.0)
                    .width(Length::Fill)
                    .push(widget::Text::new("CMD").size(10.0))
                    .push(widget::Text::new(cmd.clone()).size(11.0).width(Length::Fill)),
            );
        }
        if let Some(dir) = &cfg.working_dir {
            info = info.push(
                widget::row::with_capacity(2)
                    .spacing(8.0)
                    .width(Length::Fill)
                    .push(widget::Text::new("DIR").size(10.0))
                    .push(widget::Text::new(dir.clone()).size(11.0).width(Length::Fill)),
            );
        }
        if cfg.start_command.is_none() && cfg.working_dir.is_none() {
            info = info.push(widget::Text::new("No start command configured").size(11.0));
        }

        let mut toolbar = widget::row::with_capacity(7)
            .spacing(4.0)
            .align_y(cosmic::iced::Alignment::Center);

        toolbar = toolbar.push(tip(
            widget::button::icon(icon::from_name("go-up-symbolic").size(14))
                .on_press(Message::MoveServiceUp(cfg.id.clone())),
            "Move up",
        ));
        toolbar = toolbar.push(tip(
            widget::button::icon(icon::from_name("go-down-symbolic").size(14))
                .on_press(Message::MoveServiceDown(cfg.id.clone())),
            "Move down",
        ));

        if is_running && services::is_managed(&cfg.id) {
            toolbar = toolbar.push(tip(
                widget::button::icon(icon::from_name("view-refresh-symbolic").size(14))
                    .on_press(Message::RestartService(cfg.id.clone())),
                "Restart",
            ));
        }

        toolbar = toolbar.push(tip(
            widget::button::icon(icon::from_name("utilities-terminal-symbolic").size(14))
                .on_press(Message::ShowLogs(cfg.id.clone())),
            "Logs",
        ));

        let lock_tip = if cfg.reserved {
            "Unlock port"
        } else {
            "Reserve port"
        };
        toolbar = toolbar.push(tip(
            widget::button::icon(icon::from_name("changes-prevent-symbolic").size(14))
                .on_press(Message::ToggleReservePort(cfg.id.clone())),
            lock_tip,
        ));

        toolbar = toolbar.push(tip(
            widget::button::icon(icon::from_name("document-edit-symbolic").size(14))
                .on_press(Message::EditService(cfg.id.clone())),
            "Edit",
        ));

        toolbar = toolbar.push(widget::Space::new().width(Length::Fill));

        toolbar = toolbar.push(tip(
            widget::button::icon(icon::from_name("user-trash-symbolic").size(14))
                .class(cosmic::theme::Button::Destructive)
                .on_press(Message::DeleteService(cfg.id.clone())),
            "Delete",
        ));

        Some(
            widget::column::with_capacity(2)
                .spacing(6.0)
                .width(Length::Fill)
                .push(info)
                .push(toolbar)
                .into(),
        )
    }

    fn render_inactive_service_row<'a>(&'a self, cfg: &'a ServiceConfig) -> Element<'a, Message> {
        let is_expanded = self.expanded_service.as_deref() == Some(&cfg.id);
        let (state, _) = self.service_status(cfg, &self.ports);
        let port_num = cfg.host_port().map(|(_, p)| p).unwrap_or(0);
        let is_reserved = cfg.reserved && services::is_port_reserved(port_num);

        let led_color = match &state {
            SvcState::Failed(_) => LED_AMBER,
            SvcState::Running | SvcState::Starting => LED_GREEN,
            SvcState::Stopped => {
                if is_reserved {
                    LED_CYAN
                } else {
                    LED_PURPLE
                }
            }
        };

        let mut actions = widget::row::with_capacity(3)
            .spacing(4.0)
            .align_y(cosmic::iced::Alignment::Center);

        if cfg.can_start() {
            actions = actions.push(tip(
                widget::button::icon(icon::from_name("media-playback-start-symbolic").size(14))
                    .on_press(Message::StartService(cfg.id.clone())),
                "Start service",
            ));
        }

        let expand_icon = if is_expanded { "go-up-symbolic" } else { "go-down-symbolic" };
        actions = actions.push(tip(
            widget::button::icon(icon::from_name(expand_icon).size(14))
                .on_press(Message::ToggleExpandService(cfg.id.clone())),
            "Details",
        ));

        actions = actions.push(tip(
            widget::button::icon(icon::from_name("document-edit-symbolic").size(14))
                .on_press(Message::EditService(cfg.id.clone())),
            "Edit",
        ));

        let title_row = widget::row::with_capacity(3)
            .align_y(cosmic::iced::Alignment::Center)
            .spacing(8.0)
            .width(Length::Fill)
            .push(status_led(led_color))
            .push(
                widget::button::text(cfg.name.clone())
                    .on_press(Message::Open(cfg.address.clone())),
            )
            .push(widget::Space::new().width(Length::Fill))
            .push(
                widget::Text::new(if port_num > 0 {
                    format!(":{port_num}")
                } else {
                    String::new()
                })
                .size(11.0),
            )
            .push(actions);

        let mut content = widget::column::with_capacity(2)
            .spacing(4.0)
            .width(Length::Fill)
            .push(title_row);

        if is_expanded {
            if let Some(drawer) = self.render_service_drawer(cfg, false) {
                content = content.push(drawer);
            }
        }

        widget::container(content)
            .padding([6, 8])
            .width(Length::Fill)
            .into()
    }

    fn render_discovered_port_row<'a>(
        &'a self,
        p: &'a ports::ListeningPort,
    ) -> Element<'a, Message> {
        let row_id = format!("port-{}", p.port);
        let is_expanded = self.expanded_service.as_deref() == Some(&row_id);
        let port_url = format!("http://localhost:{}", p.port);

        let mut actions = widget::row::with_capacity(4)
            .spacing(4.0)
            .align_y(cosmic::iced::Alignment::Center);

        actions = actions.push(tip(
            widget::button::icon(icon::from_name("external-link-symbolic").size(14))
                .on_press(Message::Open(port_url.clone())),
            format!("Open {port_url}"),
        ));
        actions = actions.push(tip(
            widget::button::icon(icon::from_name("media-playback-stop-symbolic").size(14))
                .on_press(Message::KillPort(p.pid)),
            "Stop process",
        ));
        let expand_icon = if is_expanded { "go-up-symbolic" } else { "go-down-symbolic" };
        actions = actions.push(tip(
            widget::button::icon(icon::from_name(expand_icon).size(14))
                .on_press(Message::ToggleExpandService(row_id.clone())),
            "Details",
        ));
        actions = actions.push(tip(
            widget::button::icon(icon::from_name("view-pin-symbolic").size(14))
                .on_press(Message::PinPort(p.clone())),
            "Pin as service",
        ));

        let title_row = widget::row::with_capacity(3)
            .align_y(cosmic::iced::Alignment::Center)
            .spacing(8.0)
            .width(Length::Fill)
            .push(status_led(LED_CYAN))
            .push(widget::Text::new(format!(":{}", p.port)).size(13.0).font(cosmic::font::semibold()))
            .push(widget::Text::new(p.display_name.clone()).size(13.0).width(Length::Fill))
            .push(widget::Text::new(format!("PID {}", p.pid)).size(11.0))
            .push(actions);

        let mut content = widget::column::with_capacity(2)
            .spacing(4.0)
            .width(Length::Fill)
            .push(title_row);

        if is_expanded {
            let cmd = ports::process_cmdline(p.pid).unwrap_or_else(|| p.process.clone());
            let cwd = ports::process_cwd(p.pid).unwrap_or_default();
            let mut info = widget::column::with_capacity(3)
                .spacing(2.0)
                .width(Length::Fill);

            info = info.push(
                widget::row::with_capacity(2)
                    .spacing(8.0)
                    .push(widget::Text::new("BIND").size(10.0))
                    .push(widget::Text::new(format!("{}:{}", p.host, p.port)).size(11.0)),
            );
            if !cwd.is_empty() {
                info = info.push(
                    widget::row::with_capacity(2)
                        .spacing(8.0)
                        .push(widget::Text::new("DIR").size(10.0))
                        .push(widget::Text::new(cwd).size(11.0).width(Length::Fill)),
                );
            }
            if !cmd.is_empty() {
                info = info.push(
                    widget::row::with_capacity(2)
                        .spacing(8.0)
                        .push(widget::Text::new("CMD").size(10.0))
                        .push(widget::Text::new(cmd).size(11.0).width(Length::Fill)),
                );
            }
            content = content.push(info);
        }

        widget::container(content)
            .padding([6, 8])
            .width(Length::Fill)
            .into()
    }

    fn render_discovered_group_row<'a>(
        &'a self,
        row_id: String,
        display_name: String,
        process_name: String,
        framework: Option<String>,
        port_nums: Vec<u16>,
        pids: Vec<i32>,
        first_port_opt: Option<ports::ListeningPort>,
    ) -> Element<'a, Message> {
        let is_expanded = self.expanded_service.as_deref() == Some(&row_id);
        let first_port = port_nums.first().copied().unwrap_or(0);

        const MAX_INLINE_PORTS: usize = 3;
        let shown: Vec<String> = port_nums
            .iter()
            .take(MAX_INLINE_PORTS)
            .map(|p| format!(":{p}"))
            .collect();
        let mut port_label = shown.join(" · ");
        if port_nums.len() > MAX_INLINE_PORTS {
            port_label.push_str(&format!(" +{}", port_nums.len() - MAX_INLINE_PORTS));
        }

        let pid_spec = if pids.len() > 1 {
            format!("{} PIDs", pids.len())
        } else {
            format!("PID {}", pids.first().copied().unwrap_or(0))
        };

        let mut actions = widget::row::with_capacity(4)
            .spacing(4.0)
            .align_y(cosmic::iced::Alignment::Center);

        if first_port > 0 {
            let url = format!("http://localhost:{first_port}");
            actions = actions.push(tip(
                widget::button::icon(icon::from_name("external-link-symbolic").size(14))
                    .on_press(Message::Open(url.clone())),
                format!("Open {url}"),
            ));
        }
        actions = actions.push(tip(
            widget::button::icon(icon::from_name("media-playback-stop-symbolic").size(14))
                .on_press(Message::KillProcessGroup(pids.clone())),
            "Stop process group",
        ));
        let expand_icon = if is_expanded { "go-up-symbolic" } else { "go-down-symbolic" };
        actions = actions.push(tip(
            widget::button::icon(icon::from_name(expand_icon).size(14))
                .on_press(Message::ToggleExpandService(row_id.clone())),
            "Details",
        ));
        if let Some(fp) = first_port_opt {
            actions = actions.push(tip(
                widget::button::icon(icon::from_name("view-pin-symbolic").size(14))
                    .on_press(Message::PinPort(fp)),
                "Pin as service",
            ));
        }

        let title_row = widget::row::with_capacity(3)
            .align_y(cosmic::iced::Alignment::Center)
            .spacing(8.0)
            .width(Length::Fill)
            .push(status_led(LED_CYAN))
            .push(
                widget::Text::new(display_name.clone())
                    .size(13.0)
                    .font(cosmic::font::semibold())
                    .width(Length::Fill),
            )
            .push(actions);

        let mut detail_row = widget::row::with_capacity(3)
            .align_y(cosmic::iced::Alignment::Center)
            .spacing(8.0)
            .width(Length::Fill)
            .push(widget::Text::new(port_label).size(12.0).width(Length::Fill));

        if let Some(fw) = &framework {
            detail_row = detail_row.push(
                widget::button::text(fw.clone())
                    .on_press(Message::FilterTo(process_name.clone())),
            );
        } else if process_name != display_name {
            detail_row = detail_row.push(
                widget::button::text(process_name.clone())
                    .on_press(Message::FilterTo(process_name.clone())),
            );
        }

        detail_row = detail_row.push(widget::Text::new(pid_spec).size(11.0));

        let mut content = widget::column::with_capacity(3)
            .spacing(4.0)
            .width(Length::Fill)
            .push(title_row)
            .push(detail_row);

        if is_expanded {
            let mut info = widget::column::with_capacity(3)
                .spacing(2.0)
                .width(Length::Fill);

            info = info.push(
                widget::row::with_capacity(2)
                    .spacing(8.0)
                    .push(widget::Text::new("PORTS").size(10.0))
                    .push(
                        widget::Text::new(
                            port_nums
                                .iter()
                                .map(|p| p.to_string())
                                .collect::<Vec<_>>()
                                .join(", "),
                        )
                        .size(11.0),
                    ),
            );
            info = info.push(
                widget::row::with_capacity(2)
                    .spacing(8.0)
                    .push(widget::Text::new("PIDS").size(10.0))
                    .push(
                        widget::Text::new(
                            pids.iter()
                                .map(|p| p.to_string())
                                .collect::<Vec<_>>()
                                .join(", "),
                        )
                        .size(11.0),
                    ),
            );
            content = content.push(info);
        }

        widget::container(content)
            .padding([6, 8])
            .width(Length::Fill)
            .into()
    }
}
