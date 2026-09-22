mod audio;

use std::{
    collections::VecDeque,
    fs::{self, File},
    sync::mpsc,
};

use anyhow::Result;
use eframe::egui::{self, Color32, Id, Modal, Rect, pos2};
use rodio::{Decoder, MixerDeviceSink, Player};

const MUZIK_CONFIG: &str = "{
    \"theme\":\"nightly\",
}";

const MODAL_WIDGET_SPACE: f32 = 10.0;

const PLAYBACK_SPEEDS: [f32; 4] = [0.5, 1.0, 2.0, 4.0];

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

    if !fs::exists(&config_path)? {
        fs::create_dir_all("/home/risbern/.config/muzik")?;
        fs::write("/home/risbern/.config/muzik/config.json", MUZIK_CONFIG)?;
    }

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
        ..Default::default()
    };
    eframe::run_native(
        "Muzik",
        native_options,
        Box::new(|cc| Ok(Box::new(Muzik::new(cc, input_path)?))),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    Ok(())
}

pub struct Muzik {
    input_path: Option<String>,
    is_playing: bool,
    playback_speed: f32,
    handle: MixerDeviceSink,
    player: Option<Player>,
    frame_receiver: Option<mpsc::Receiver<f32>>,
    sample_history: VecDeque<f32>,
    max_visible_points: usize,
    theme: MuzikTheme,
    visualisation_theme: VisualizationTheme,
    visualisation_type: VisualizationType,
    settings_modal_open: bool,
}

impl Muzik {
    fn new(cc: &eframe::CreationContext<'_>, input_path: Option<String>) -> Result<Muzik> {
        egui_material_icons::initialize(&cc.egui_ctx);
        let handle = rodio::DeviceSinkBuilder::open_default_sink()?;
        let is_playing = input_path.is_some();

        let mut muzik = Muzik {
            input_path,
            is_playing,
            playback_speed: 1.0,
            handle,
            player: None,
            frame_receiver: None,
            sample_history: VecDeque::from(vec![0.0; 1024]),
            max_visible_points: 1024,
            theme: MuzikTheme::Nightly,
            visualisation_theme: VisualizationTheme::Lavender,
            visualisation_type: VisualizationType::Type1,
            settings_modal_open: false,
        };
        muzik.play_audio()?;
        Ok(muzik)
    }

    fn play_audio(&mut self) -> Result<()> {
        // let (pcm_data, sample_rate) = audio::audio::decode_to_pcm(&input_path)?;
        // let target_duration = 0.0464;

        // let window_size = audio::audio::calculate_dynamic_window_size(sample_rate, target_duration);
        // let hop_size = window_size / 2;

        // let windowed_samples = apply_hann_window(&pcm_data, window_size, hop_size);
        // if let Some((transformed, fft_size)) = audio::transform::fft(windowed_samples) {
        //     self.all_audio_bands = transformed
        //         .iter()
        //         .map(|sample| audio::transform::get_audio_bands(sample, sample_rate, fft_size))
        //         .collect();
        if self.is_playing
            && let Some(input_path) = &self.input_path
        {
            let (tx, rx) = mpsc::channel();
            self.frame_receiver = Some(rx);

            let file = File::open(input_path)?;
            let source = Decoder::try_from(file)?;
            let captured_source = audio::source::PcmCaptureSource::new(source, tx);
            let player = Player::connect_new(self.handle.mixer());
            self.playback_speed = 1.0;
            player.append(captured_source);
            self.player = Some(player);
        }
        Ok(())
    }

    fn control_playback_speed(&self) {
        if let Some(player) = &self.player {
            player.set_speed(self.playback_speed);
        }
    }

    fn pause_audio(&self) {
        if let Some(player) = &self.player {
            player.pause();
        }
    }

    fn resume_audio(&self) {
        if let Some(player) = &self.player {
            player.play();
        }
    }

    fn stop_audio(&mut self) {
        if let Some(player) = &self.player {
            player.clear();
        }
        self.sample_history = VecDeque::from(vec![0.0; self.max_visible_points]);
        self.frame_receiver = None;
        self.is_playing = false;
        self.player = None;
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

    fn generate_rects(&self, screen_width: f32, screen_height: f32) -> Vec<Rect> {
        let mut rects = Vec::with_capacity(self.sample_history.len());

        let rect_width = screen_width / self.max_visible_points as f32;
        let center_y = screen_height / 2.0;
        let max_amplitude_height = screen_height / 2.0;

        match self.visualisation_type {
            VisualizationType::Type1 => {
                for (index, &sample) in self.sample_history.iter().enumerate() {
                    let x = index as f32 * rect_width;
                    let h = sample * max_amplitude_height;
                    let (top, bottom) = if h > 0.0 {
                        (center_y - h, center_y)
                    } else {
                        (center_y, center_y - h)
                    };

                    rects.push(Rect::from_min_max(
                        pos2(x, top),
                        pos2(x + rect_width.max(1.0), bottom),
                    ));
                }
            }
            VisualizationType::Type2 => {
                for (index, &sample) in self.sample_history.iter().enumerate() {
                    let x = index as f32 * rect_width;
                    let h = sample.abs() * max_amplitude_height;
                    let (top, bottom) = (screen_height - h, screen_height);

                    rects.push(Rect::from_min_max(
                        pos2(x, top),
                        pos2(x + rect_width.max(1.0), bottom),
                    ));
                }
            }
        }

        rects
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

                let visual_rects = self.generate_rects(screen_rect.width(), screen_rect.height());
                for bar in visual_rects {
                    painter.rect_filled(bar, 0.0, self.visualisation_theme.get_rgb());
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

                                if ui
                                    .button(egui_material_icons::icons::ICON_PLAY_ARROW)
                                    .clicked()
                                    && self.is_playing
                                {
                                    self.resume_audio();
                                }
                                if ui.button(egui_material_icons::icons::ICON_PAUSE).clicked()
                                    && self.is_playing
                                {
                                    self.pause_audio();
                                }
                                if ui.button(egui_material_icons::icons::ICON_STOP).clicked()
                                    && self.is_playing
                                {
                                    self.stop_audio();
                                    ui.request_repaint();
                                }

                                let mut scalar = 0.0;
                                ui.add(egui::Slider::new(&mut scalar, 0.0..=360.0).suffix("°"));
                                ui.end_row();

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

                    ui.label("Visualization Type");
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.visualisation_type,
                            VisualizationType::Type1,
                            "Type-1",
                        );
                        ui.selectable_value(
                            &mut self.visualisation_type,
                            VisualizationType::Type2,
                            "Type-2",
                        );
                    });
                    ui.end_row();
                    ui.add_space(MODAL_WIDGET_SPACE);

                    ui.label("Visualization Theme");
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.visualisation_theme,
                            VisualizationTheme::Black,
                            "Black",
                        );
                        ui.selectable_value(
                            &mut self.visualisation_theme,
                            VisualizationTheme::White,
                            "White",
                        );
                        ui.selectable_value(
                            &mut self.visualisation_theme,
                            VisualizationTheme::Lavender,
                            "Lavender",
                        );
                        ui.selectable_value(
                            &mut self.visualisation_theme,
                            VisualizationTheme::Pink,
                            "Pink",
                        );
                        ui.selectable_value(
                            &mut self.visualisation_theme,
                            VisualizationTheme::Sky,
                            "Sky",
                        );
                    });

                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.visualisation_theme,
                            VisualizationTheme::Teal,
                            "Teal",
                        );
                        ui.selectable_value(
                            &mut self.visualisation_theme,
                            VisualizationTheme::Crust,
                            "Crust",
                        );
                        ui.selectable_value(
                            &mut self.visualisation_theme,
                            VisualizationTheme::Mauve,
                            "Mauve",
                        );
                    });
                    ui.end_row();
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
                self.play_audio().unwrap();
            }
        });
    }
}

#[derive(PartialEq)]
enum VisualizationType {
    Type1,
    Type2,
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
}

#[derive(PartialEq)]
enum VisualizationTheme {
    Black,
    White,
    Lavender,
    Pink,
    Sky,
    Teal,
    Crust,
    Mauve,
}

impl VisualizationTheme {
    fn get_rgb(&self) -> Color32 {
        match self {
            Self::Black => Color32::from_rgb(0, 0, 0),
            Self::White => Color32::from_rgb(255, 255, 255),
            Self::Lavender => Color32::from_rgb(183, 189, 248),
            Self::Pink => Color32::from_rgb(245, 194, 231),
            Self::Sky => Color32::from_rgb(4, 165, 229),
            Self::Teal => Color32::from_rgb(139, 213, 202),
            Self::Crust => Color32::from_rgb(220, 224, 232),
            Self::Mauve => Color32::from_rgb(198, 160, 246),
        }
    }
}
