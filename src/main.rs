extern crate num;

mod audio;
mod fft;
mod shader_widget;
mod window;

use std::{
    collections::VecDeque,
    f32,
    fs::{self, File},
    path::{Path, PathBuf},
    sync::mpsc,
    time,
};

use anyhow::Result;
use bytemuck::Contiguous;
use eframe::{
    egui::{self, Color32, Id, Modal, Rect, pos2},
    egui_wgpu,
    epaint::Hsva,
};
use num::Complex;
use rodio::{Decoder, MixerDeviceSink, Player, Source};

use crate::shader_widget::{TriangleCallback, init_shader};

const N: usize = 1 << 14;

const MUZIK_CONFIG: &str = r#"{
    "theme":"nightly"
}"#;

const MODAL_WIDGET_SPACE: f32 = 10.0;

const PLAYBACK_SPEEDS: [f32; 4] = [0.5, 1.0, 2.0, 4.0];
const SUPPORTED_EXTENSIONS: [&str; 5] = ["mp3", "mp4", "flac", "vorbis", "wav"];

const SATURATION: f32 = 0.8_f32;
const BRIGHTNESS: f32 = 0.8_f32;

const STEP: f32 = 1.06_f32;
const MIN_BIN: f32 = 20.0_f32;
const MAX_BIN: f32 = (N / 2) as f32;

const FLOOR_DB: f32 = -80.0;
const CEIL_DB: f32 = 0.0;

fn main() -> Result<()> {
    if let Ok(env_var) = std::env::var("XDG_SESSION_TYPE")
        && env_var == "wayland"
    {
        unsafe {
            std::env::remove_var("WAYLAND_DISPLAY");
        }
    }

    let config_dir = dirs::config_dir()
        .expect("could not find config directory")
        .join("muzik");
    let config_path = config_dir.join("config.json");

    let muzik_config = if !fs::exists(&config_path)? {
        fs::create_dir_all(config_dir)?;
        fs::write(&config_path, MUZIK_CONFIG)?;
        MUZIK_CONFIG.to_string()
    } else {
        fs::read_to_string(&config_path)?
    };

    let config: MuzikConfig = serde_json::from_str(muzik_config.as_str())?;

    let args: Vec<String> = std::env::args().collect();
    let mut input_path: Option<String> = None;
    let mut iter = args.iter().skip(1);

    while let Some(arg) = iter.next() {
        if arg == "-i" {
            input_path = iter.next().cloned();
        }
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_drag_and_drop(true),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Muzik",
        native_options,
        Box::new(|cc| Ok(Box::new(Muzik::new(cc, config, config_path, input_path)?))),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    Ok(())
}

pub struct Muzik {
    bar_count: usize,
    config_path: PathBuf,
    is_playing: bool,
    is_paused: bool,
    playback_speed: f32,
    handle: MixerDeviceSink,
    input_path: Option<String>,
    sample_rate: Option<u32>,
    total_duration: Option<time::Duration>,
    player: Option<Player>,
    frame_receiver: Option<mpsc::Receiver<f32>>,
    sample_history: VecDeque<f32>,
    max_visible_points: usize,
    theme: MuzikTheme,
    settings_modal_open: bool,
    smoothed: Vec<f32>,
}

impl Muzik {
    fn new(
        cc: &eframe::CreationContext<'_>,
        config: MuzikConfig,
        config_path: PathBuf,
        input_path: Option<String>,
    ) -> Result<Muzik> {
        egui_material_icons::initialize(&cc.egui_ctx);
        init_shader(cc);

        let handle = rodio::DeviceSinkBuilder::open_default_sink()?;
        let is_playing = input_path.is_some();

        let mut bar_count = 0usize;
        let mut f = MIN_BIN;
        while f < MAX_BIN {
            bar_count += 1;
            f *= STEP;
        }

        let mut muzik = Muzik {
            bar_count,
            config_path,
            is_playing,
            is_paused: false,
            playback_speed: 1.0,
            handle,
            input_path,
            sample_rate: None,
            total_duration: None,
            player: None,
            frame_receiver: None,
            sample_history: VecDeque::from(vec![0.0; N]),
            max_visible_points: N,
            theme: config.get_theme(),
            settings_modal_open: false,
            smoothed: vec![0.0_f32; bar_count],
        };
        muzik.play_audio()?;
        Ok(muzik)
    }

    fn play_audio(&mut self) -> Result<(), MuzikError> {
        if !self.is_playing {
            return Ok(());
        }
        let Some(input_path) = &self.input_path else {
            return Ok(());
        };

        let extension = Path::new(input_path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());

        match extension {
            Some(ext) => if SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {},
            _ => return Err(MuzikError::UnsupportedExtension(input_path.clone())),
        }

        let (tx, rx) = mpsc::channel();
        self.frame_receiver = Some(rx);

        let file = File::open(input_path)?;
        let source = Decoder::try_from(file)?;
        let captured_source = audio::source::PcmCaptureSource::new(source, tx);
        self.sample_rate = Some(captured_source.sample_rate().into_integer());
        self.total_duration = captured_source.total_duration();
        let player = Player::connect_new(self.handle.mixer());
        self.playback_speed = 1.0;
        player.append(captured_source);
        self.player = Some(player);
        Ok(())
    }

    fn control_playback_speed(&self) {
        if let Some(player) = &self.player {
            player.set_speed(self.playback_speed);
        }
    }

    fn pause_audio(&mut self) {
        if let Some(player) = &self.player {
            player.pause();
            self.is_paused = true;
            self.is_playing = false;
        }
    }

    fn resume_audio(&mut self) {
        if let Some(player) = &self.player {
            player.play();
            self.is_paused = false;
            self.is_playing = true;
        }
    }

    fn stop_audio(&mut self) {
        if let Some(player) = &self.player {
            player.clear();
        }
        self.sample_history = VecDeque::from(vec![0.0; self.max_visible_points]);
        self.frame_receiver = None;
        self.is_playing = false;
        self.is_paused = false;
        self.player = None;
        self.input_path = None;
    }

    fn save_config(&self) -> Result<(), std::io::Error> {
        let config = format!(
            "{{
                \"theme\":\"{}\",
            }}",
            self.theme.get_name(),
        );
        fs::write(&self.config_path, config)
    }

    fn update(&mut self) {
        if self.is_playing
            && let Some(receiver) = &self.frame_receiver
        {
            for sample in receiver.try_iter() {
                self.sample_history.push_back(sample);
                if self.sample_history.len() > self.max_visible_points {
                    self.sample_history.pop_front();
                }
            }
        }
    }
}

impl eframe::App for Muzik {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.update();

        let ctx = ui.ctx();
        if self
            .player
            .as_ref()
            .is_some_and(|p| !p.is_paused() && !p.empty())
        {
            ctx.request_repaint();
        };

        if ui.input(|i| i.key_pressed(egui::Key::Space)) {
            if self.is_playing {
                self.pause_audio();
            } else if self.is_paused {
                self.resume_audio();
            }
        }

        let screen_rect = ctx.content_rect();

        let trigger_zone_height = 30.0;
        let trigger_rect = egui::Rect::from_min_max(
            egui::pos2(
                screen_rect.left(),
                screen_rect.bottom() - trigger_zone_height,
            ),
            screen_rect.max,
        );

        let pointer_near_bottom = ctx
            .input(|i| i.pointer.hover_pos())
            .map(|pos| trigger_rect.contains(pos))
            .unwrap_or(false);

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(self.theme.get_rgb()))
            .show(ui, |ui| {
                let painter = ui.painter();

                let screen_width = screen_rect.width();
                let screen_height = screen_rect.height();

                let max_bar_height = screen_height;

                let input: Vec<Complex<f32>> = match (self.sample_rate, self.total_duration) {
                    (Some(sample_rate), Some(total_duration)) => {
                        let input_samples: Vec<&f32> = self.sample_history.iter().collect();
                        let window_size = window::calculate_dynamic_window_size(
                            sample_rate,
                            total_duration.as_secs_f32(),
                        );
                        let windowed_samples =
                            window::apply_hann_window(&input_samples, window_size, window_size / 2);
                        windowed_samples.iter().map(Complex::from).collect()
                    }
                    _ => Vec::new(),
                };
                let output = fft::transform::fft(&input);

                let rect_width = screen_width / self.bar_count as f32;
                let mut f = MIN_BIN;
                for m in 0..self.bar_count {
                    // take average of skipped frames
                    let f1 = f * STEP;
                    let mut q = f;
                    let mut amp = 0.0_f32;
                    while q < f1 && q < N as f32 && q < output.len() as f32 {
                        amp += output[q as usize].norm();
                        q += 1.0;
                    }

                    amp /= f1 - f + 1.0;

                    // take hann gain (0.5) into account
                    let n = input.len().max(1) as f32;
                    let scale = 2.0 / (n * 0.5);
                    amp *= scale;

                    // normalize into 0..1 range
                    let norm = {
                        let db_amp = 20.0 * (amp + 1e-9).log10();
                        ((db_amp - FLOOR_DB) / (CEIL_DB - FLOOR_DB)).clamp(0.0, 1.0)
                    };

                    // smooth
                    let prev = self.smoothed[m];
                    let s = if norm > prev {
                        prev - (norm - prev) * 0.6
                    } else {
                        prev + (norm - prev) * 0.15
                    };
                    self.smoothed[m] = s;

                    // map
                    let t = norm * max_bar_height;
                    let x = m as f32 * rect_width;
                    let y = screen_height - t;

                    //draw shader bars
                    let bar = Rect::from_min_max(pos2(x, y), pos2(x + rect_width, screen_height));

                    let color: Color32 =
                        Hsva::new(m as f32 / 360.0, SATURATION, BRIGHTNESS, 1.0).into();
                    let stroke_width = t.min(1.0);
                    painter.add(egui_wgpu::Callback::new_paint_callback(
                        bar,
                        TriangleCallback { size: 0.0 },
                    ));
                    painter.rect_stroke(
                        bar,
                        0.5,
                        egui::Stroke::new(stroke_width, color),
                        egui::StrokeKind::Outside,
                    );

                    f *= STEP;
                }

                if pointer_near_bottom {
                    egui::Panel::bottom("status bar")
                        .frame(
                            egui::Frame::default()
                                .fill(self.theme.get_secondary_rgb())
                                .inner_margin(egui::Margin::symmetric(10, 0)),
                        )
                        .exact_size(trigger_zone_height)
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        if ui.button(format!("{}x", self.playback_speed)).clicked()
                                        {
                                            let i = PLAYBACK_SPEEDS
                                                .iter()
                                                .position(|&x| x == self.playback_speed);
                                            if let Some(index) = i {
                                                let playback_speed = PLAYBACK_SPEEDS
                                                    [(index + 1) % PLAYBACK_SPEEDS.len()];
                                                self.playback_speed = playback_speed;
                                                self.control_playback_speed();
                                            }
                                        }
                                    },
                                );

                                if self.is_paused
                                    && ui
                                        .button(egui_material_icons::icons::ICON_PLAY_ARROW)
                                        .clicked()
                                {
                                    self.resume_audio();
                                } else if self.is_playing
                                    && ui.button(egui_material_icons::icons::ICON_PAUSE).clicked()
                                {
                                    self.pause_audio();
                                }
                                if ui.button(egui_material_icons::icons::ICON_STOP).clicked()
                                    && (self.is_playing || self.is_paused)
                                {
                                    self.stop_audio();
                                    ui.request_repaint();
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .button(egui_material_icons::icons::ICON_SETTINGS)
                                            .clicked()
                                        {
                                            self.settings_modal_open = true;
                                        }
                                    },
                                );
                            });
                        });
                    ui.request_repaint();
                }
            });

        if self.settings_modal_open {
            let modal = Modal::new(Id::new("settings modal"))
                .frame(
                    egui::Frame::default()
                        .fill(self.theme.get_secondary_rgb())
                        .corner_radius(8.0)
                        .inner_margin(egui::Margin::same(10)),
                )
                .show(ui.ctx(), |ui| {
                    ui.visuals_mut().widgets.noninteractive.bg_stroke.color =
                        self.theme.get_noninteractive_rgb();
                    ui.set_width(250.0);
                    ui.heading("Settings");
                    ui.separator();

                    ui.label("Background");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.theme, MuzikTheme::Dark, "Dark");
                        ui.selectable_value(&mut self.theme, MuzikTheme::Light, "Light");
                        ui.selectable_value(&mut self.theme, MuzikTheme::Nightly, "Nightly");
                    });
                    ui.end_row();
                    ui.add_space(MODAL_WIDGET_SPACE);

                    ui.end_row();
                    ui.add_space(MODAL_WIDGET_SPACE);

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.settings_modal_open = false;
                        }
                        if ui.button("Save").clicked() {
                            if let Err(e) = self.save_config() {
                                eprintln!("Something went wrong :  {e}");
                            }
                            self.settings_modal_open = false;
                        }
                    });
                });

            if modal.should_close() {
                self.settings_modal_open = false;
            }
        }

        ui.ctx().input(|i| {
            for file in &i.raw.dropped_files {
                let path = file.path();
                self.input_path = Some(path.display().to_string());
                self.is_playing = true;
                if self.play_audio().is_err() {
                    self.stop_audio();
                    self.input_path = None;
                };
            }
        });
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct MuzikConfig {
    pub theme: String,
}

impl MuzikConfig {
    fn get_theme(&self) -> MuzikTheme {
        match self.theme.as_str() {
            "dark" => MuzikTheme::Dark,
            "light" => MuzikTheme::Light,
            _ => MuzikTheme::Nightly,
        }
    }
}

#[derive(PartialEq)]
enum MuzikTheme {
    Dark,
    Light,
    Nightly,
}

impl MuzikTheme {
    fn get_rgb(&self) -> Color32 {
        match self {
            Self::Dark => egui::Color32::from_rgb(0, 0, 0),
            Self::Light => egui::Color32::from_rgb(255, 255, 255),
            Self::Nightly => egui::Color32::from_rgb(30, 32, 48),
        }
    }

    fn get_secondary_rgb(&self) -> Color32 {
        match self {
            Self::Dark => egui::Color32::from_rgb(20, 20, 20),
            Self::Light => egui::Color32::from_rgb(239, 241, 245),
            Self::Nightly => egui::Color32::from_rgb(48, 52, 70),
        }
    }

    fn get_noninteractive_rgb(&self) -> Color32 {
        match self {
            Self::Dark => egui::Color32::from_rgb(108, 112, 134),
            Self::Light => egui::Color32::from_rgb(0, 0, 0),
            Self::Nightly => egui::Color32::from_rgb(108, 112, 134),
        }
    }

    fn get_name(&self) -> String {
        match self {
            Self::Dark => "dark".to_string(),
            Self::Light => "light".to_string(),
            _ => "nightly".to_string(),
        }
    }
}

#[derive(Debug)]
enum MuzikError {
    UnsupportedExtension(String),
    Io(std::io::Error),
    DecodeError(rodio::decoder::DecoderError),
}

impl std::fmt::Display for MuzikError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedExtension(path) => {
                write!(f, "unsupported file type {path:?}")
            }
            Self::Io(e) => {
                write!(f, "failed to open file {e:?}")
            }
            Self::DecodeError(e) => {
                write!(f, "unable to decode file {e:?}")
            }
        }
    }
}

impl std::error::Error for MuzikError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::UnsupportedExtension(_) => None,
            Self::Io(e) => Some(e),
            Self::DecodeError(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for MuzikError {
    fn from(e: std::io::Error) -> Self {
        MuzikError::Io(e)
    }
}

impl From<rodio::decoder::DecoderError> for MuzikError {
    fn from(e: rodio::decoder::DecoderError) -> Self {
        MuzikError::DecodeError(e)
    }
}
