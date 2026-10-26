// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Triya CAD 3D desktop app: a thin shell around SolidCraft's `SolveApp`.
//!
//! Usage: `triya [--control <port>] [--sample] [design.solidcraft | part.step]…`
//!
//! What it adds to upstream's engine and UI: the `vessel.*` commands ([`triya_ui::register`]),
//! the Triya title, window icon and app id, the Vessel panel and Triya's About window drawn around
//! `SolveApp::ui`, and no update check (`Services::http_get = None`: upstream's checker would offer
//! SolidCraft releases). It is **not** a copy of upstream's 2,700-line `apps/solidcraft`: the
//! Windows native crash filter and resize border, macOS Finder open events, Space Mouse, the
//! startup safe-mode counter, the agent chat's API agents and keychain stay upstream's and are
//! left out of the beta (PLAN T1.3, PROGRESS.md "Decisions"). Panics and unclean exits still
//! leave crash reports (`solidcraft_engine::crash`).
//!
//! `--control <port>` (or `SOLIDCRAFT_CONTROL_PORT`) starts upstream's localhost JSON-lines
//! control server (`apps/solidcraft/src/control_server.rs`, compiled here unchanged); see
//! docs/control-protocol.md.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

#[path = "../../../../apps/solidcraft/src/control_server.rs"]
mod control_server;

use solidcraft_engine::{Session, io};
use solidcraft_ui_egui::{Services, SolveApp};

const DEPTH_BITS: u8 = 24;
/// The section cap marks where it shows in the stencil.
const STENCIL_BITS: u8 = 8;
const MSAA: u16 = 4;
/// Window icon size (pixels), drawn by `triya_brand::mark`.
const ICON_SIZE: u32 = 64;

struct App {
    solve: SolveApp,
    vessel: triya_ui::VesselPanel,
    about: triya_ui::about::About,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.solve.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            solidcraft_ui_egui::documents::request_quit(&mut self.solve);
        }
        self.solve.logic(ctx);
        if self.solve.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.solve.raw_input_hook(raw);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Triya's About replaces upstream's (Help ▸ About): a request from the last frame is taken
        // before the app draws, one made while it draws right after.
        self.about.intercept(&mut self.solve);
        self.solve.ui(ui);
        let ctx = ui.ctx().clone();
        self.vessel.show(&mut self.solve, &ctx);
        self.about.show(&mut self.solve, &ctx);
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(solidcraft_ui_egui::prefs::STORAGE_KEY, self.solve.prefs());
    }
    fn on_exit(&mut self) {
        solidcraft_ui_egui::agent_chat::shutdown(&mut self.solve);
        solidcraft_ui_egui::recovery_ui::close(&mut self.solve);
        solidcraft_engine::crash::end_session();
    }
}

/// Each extension in lower and upper case (Linux dialogs match case), as upstream's shell does.
fn case_variants(exts: &[&str]) -> Vec<String> {
    let mut all: Vec<String> = Vec::new();
    for e in exts {
        for v in [e.to_ascii_lowercase(), e.to_ascii_uppercase()] {
            if !all.contains(&v) {
                all.push(v);
            }
        }
    }
    all
}

fn services(cc: &eframe::CreationContext<'_>) -> Services {
    let parent = rfd::AsyncFileDialog::new().set_parent(cc);
    let open_parent = parent.clone();
    Services {
        pick_open: Some(Box::new(move |first: &[&str]| {
            // Upstream's picker (v0.22): the file type the command reads first, then the format
            // registry's groups, then everything.
            let mut dialog = open_parent.clone();
            if !first.is_empty() {
                let label = first.iter().map(|e| e.to_uppercase()).collect::<Vec<_>>().join(", ");
                dialog = dialog.add_filter(label, &case_variants(first));
            }
            dialog = dialog.add_filter("Designs, CAD files and meshes", &case_variants(&io::formats::open_extensions(true)));
            for (label, exts) in io::formats::import_filters(true) {
                dialog = dialog.add_filter(label, &case_variants(&exts));
            }
            let future = dialog.add_filter("All files", &["*"]).pick_file();
            Box::pin(async move { future.await.map(|p| p.path().to_string_lossy().into_owned()) })
        })),
        pick_save: Some(Box::new(move |name: &str, exts: &[&str]| {
            let dialog = parent.clone().set_file_name(name);
            let dialog = if exts == [io::DESIGN_EXT, "json"] {
                dialog.add_filter("Design container", &[io::DESIGN_EXT]).add_filter("Plain JSON design", &["json"])
            } else {
                dialog.add_filter("File", exts)
            };
            let future = dialog.save_file();
            Box::pin(async move { future.await.map(|p| p.path().to_string_lossy().into_owned()) })
        })),
        // Upstream's update checker polls SolidCraft's releases: never for Triya.
        http_get: None,
        http_download: None,
        // Already an installed app: only the browser build offers to install.
        install_app: None,
        ..Default::default()
    }
}

/// The per-user settings folder (eframe's `app.ron`), separate from SolidCraft's.
fn settings_dir() -> Option<std::path::PathBuf> {
    eframe::storage_dir(triya_brand::APP_NAME)
}

fn viewport() -> egui::ViewportBuilder {
    let builder = egui::ViewportBuilder::default()
        .with_title(triya_brand::APP_NAME)
        .with_inner_size([1600.0, 1000.0])
        .with_min_inner_size([960.0, 600.0])
        .with_drag_and_drop(true)
        .with_app_id(triya_brand::APP_ID);
    match triya_ui::icons::window_icon(ICON_SIZE) {
        Some(icon) => builder.with_icon(icon),
        None => builder,
    }
}

fn main() -> eframe::Result {
    let mut control_port: Option<u16> = solidcraft_engine::geom::env::var("CONTROL_PORT").and_then(|p| p.parse().ok());
    let mut files = Vec::new();
    let mut sample = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--control" => control_port = args.next().and_then(|p| p.parse().ok()),
            "--sample" => sample = true,
            "--version" | "-V" => {
                println!("triya {} (SolidCraft {})", triya_brand::VERSION, triya_brand::UPSTREAM_VERSION);
                return Ok(());
            }
            "--help" | "-h" => {
                println!("usage: triya [--control PORT] [--sample] [design.solidcraft | part.step]…");
                return Ok(());
            }
            _ => files.push(a),
        }
    }
    // Commands first: every session made from here on knows `vessel.*`.
    if let Err(e) = triya_ui::register() {
        eprintln!("{}: {e}", triya_brand::APP_NAME);
    }
    let crashed = solidcraft_engine::crash::default_dir()
        .map(|dir| {
            let build = solidcraft_engine::crash::Build {
                version: solidcraft_ui_egui::help::VERSION,
                commit: solidcraft_ui_egui::help::COMMIT,
                built: solidcraft_ui_egui::help::BUILD_DATE,
            };
            solidcraft_engine::crash::install(dir, build)
        })
        .unwrap_or_default();
    let options = eframe::NativeOptions {
        viewport: viewport(),
        depth_buffer: DEPTH_BITS,
        stencil_buffer: STENCIL_BITS,
        multisampling: MSAA,
        persistence_path: settings_dir().map(|d| d.join("app.ron")),
        ..Default::default()
    };
    let result = eframe::run_native(
        triya_brand::APP_NAME,
        options,
        Box::new(move |cc| {
            solidcraft_ui_egui::fonts::setup_fonts(&cc.egui_ctx);
            let mut app = SolveApp::new(Session::default(), services(cc));
            app.apply_off_thread = Some(solidcraft_ui_egui::dialogs::APPLY_AT_ONCE);
            // The OS title bar shows "Triya CAD 3D"; upstream's custom bar is SolidCraft's chrome.
            app.custom_titlebar = false;
            if let Some(p) = cc.storage.and_then(|s| solidcraft_ui_egui::prefs::stored(|k| s.get_string(k))) {
                app.load_prefs(&p);
            }
            if let Some(rs) = &cc.wgpu_render_state {
                app.set_wgpu(rs, DEPTH_BITS, STENCIL_BITS, rs.msaa_samples);
                solidcraft_ui_egui::gpu::install_picture_renderer(rs);
            }
            // `--control`/the environment name a port; otherwise upstream's preference (on by
            // default) opens the default one (docs/control-protocol.md).
            if let Some(listen_on) = solidcraft_engine::control_auth::plan(control_port, app.preferences.agent_control) {
                let rx = control_server::start(listen_on, cc.egui_ctx.clone());
                app = app.with_control(rx);
            }
            // The agent chat's MCP bridge reaches the app through an endpoint of its own.
            let ctx = cc.egui_ctx.clone();
            app.agent_chat.control_starter = Some(Box::new(move |tx| control_server::listen_agent(tx, ctx.clone())));
            if sample {
                if let Err(e) = app.session.run_script(&solidcraft_engine::sample::script()) {
                    app.set_status(format!("sample: {e}"), true);
                }
                app.session.doc_mut().name = "Sample Plate".into();
                app.session.mark_saved();
                app.session.undo.clear();
            }
            for f in &files {
                app.open_path_async(f);
            }
            if files.is_empty() && !sample {
                solidcraft_ui_egui::prefs::apply_new_design(&mut app);
            }
            if files.is_empty() && !sample && control_port.is_none() {
                let rev = app.session.revision;
                app.home.show(rev);
            }
            if let Some(dir) = solidcraft_engine::recovery::default_dir() {
                let every = if app.autosave_minutes > 0.0 { app.autosave_minutes * 60.0 } else { f64::MAX / 4.0 };
                solidcraft_ui_egui::recovery_ui::start(&mut app, &dir, std::time::Duration::from_secs_f64(every.min(1e9)));
            }
            solidcraft_ui_egui::crash_ui::announce(&mut app, &crashed);
            Ok(Box::new(App { solve: app, vessel: triya_ui::VesselPanel::default(), about: triya_ui::about::About::default() }))
        }),
    );
    control_server::remove_token_file();
    result
}
