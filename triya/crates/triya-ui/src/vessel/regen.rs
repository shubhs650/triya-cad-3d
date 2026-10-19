// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Regenerate without freezing the window. A vessel with nozzles takes minutes in this kernel
//! (PROGRESS.md "Measurements"), so the panel's Regenerate runs `vessel.regenerate` on a scratch
//! copy of the session on a worker thread, shows `triya_vessel::commands::progress` as a bar,
//! and hands the result back with `Session::accept_computed` (one undo step, as upstream's
//! dialogs do in `dialogs::begin_applying`). If the design changed meanwhile the result is
//! dropped and the panel says so. The browser has no threads: there the command runs on the
//! window thread one frame after the bar is drawn, so the user sees that it started.

use serde_json::{Value, json};
use solidcraft_ui_egui::SolveApp;

#[cfg(not(target_arch = "wasm32"))]
type Outcome = Result<(solidcraft_engine::Session, Value), String>;

/// A regeneration in flight.
#[derive(Default)]
pub struct Regen {
    #[cfg(not(target_arch = "wasm32"))]
    job: Option<(u64, std::sync::mpsc::Receiver<Outcome>)>,
    /// Browser: frames left before the command runs.
    #[cfg(target_arch = "wasm32")]
    countdown: Option<u8>,
    started: f64,
}

/// What a finished regeneration reports to the panel.
pub enum Finished {
    Done(Value),
    Failed(String),
}

impl Regen {
    pub fn busy(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        return self.job.is_some();
        #[cfg(target_arch = "wasm32")]
        return self.countdown.is_some();
    }

    /// Start a regeneration (nothing when one is running).
    pub fn start(&mut self, app: &mut SolveApp) {
        if self.busy() {
            return;
        }
        self.started = solidcraft_ui_egui::now_ms();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut scratch = app.session.scratch();
            let (tx, rx) = std::sync::mpsc::channel();
            let spawned = std::thread::Builder::new().name("vessel-regenerate".into()).spawn(move || {
                let r = scratch.execute("vessel.regenerate", &json!({})).map(|v| (scratch, v)).map_err(|e| e.to_string());
                let _ = tx.send(r);
            });
            match spawned {
                Ok(_) => self.job = Some((app.session.revision, rx)),
                // No thread to be had: run it here.
                Err(_) => {
                    let _ = app.run("vessel.regenerate", json!({}));
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = app;
            self.countdown = Some(2);
        }
    }

    /// Seconds since the regeneration started.
    pub fn elapsed_s(&self) -> f64 {
        (solidcraft_ui_egui::now_ms() - self.started) / 1000.0
    }

    /// Call once a frame: takes in a finished regeneration.
    pub fn poll(&mut self, app: &mut SolveApp, ctx: &egui::Context) -> Option<Finished> {
        if !self.busy() {
            return None;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (rev, rx) = self.job.as_ref()?;
            let outcome = match rx.try_recv() {
                Ok(o) => o,
                Err(std::sync::mpsc::TryRecvError::Empty) => return None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Err("the regeneration stopped unexpectedly".into()),
            };
            let rev = *rev;
            self.job = None;
            Some(match outcome {
                Ok((computed, reply)) if app.session.revision == rev => {
                    app.session.accept_computed(computed);
                    Finished::Done(reply)
                }
                Ok(_) => Finished::Failed("the design changed while the vessel regenerated; Regenerate again".into()),
                Err(e) => Finished::Failed(e),
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            match self.countdown {
                Some(0) => {
                    self.countdown = None;
                    Some(match app.run("vessel.regenerate", json!({})) {
                        Ok(v) => Finished::Done(v),
                        Err(e) => Finished::Failed(e),
                    })
                }
                Some(n) => {
                    self.countdown = Some(n - 1);
                    None
                }
                None => None,
            }
        }
    }

    /// The progress bar (while busy).
    pub fn show(&self, ui: &mut egui::Ui) {
        if !self.busy() {
            return;
        }
        let (done, total, label) = triya_vessel::commands::progress::get();
        let fraction = if total > 0 { done as f32 / total as f32 } else { 0.0 };
        let text = if total > 0 {
            format!("Regenerating: {label} ({done}/{total}) · {:.0} s", self.elapsed_s())
        } else {
            format!("Regenerating… {:.0} s", self.elapsed_s())
        };
        ui.add(egui::ProgressBar::new(fraction).text(text).animate(true));
    }
}
