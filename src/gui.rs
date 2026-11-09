use clap::ValueEnum;
use easy_media_compress::{
    APP_NAME, VERSION,
    cli::mb,
    jobs::{Event, Job, Queue},
    media::{Details, IMAGE_EXTS, ImageMode, Options, Update, VIDEO_EXTS, VideoProfile},
    settings::Settings,
};
use eframe::egui::{self, ComboBox, RichText, Sense, Ui, ViewportCommand};
use std::path::PathBuf;

pub fn run(inputs: Vec<PathBuf>, #[cfg(windows)] inbox: crate::inbox::Inbox) -> eframe::Result {
    let settings = Settings::load();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(settings.window_size.unwrap_or([860.0, 450.0]))
        .with_min_inner_size([520.0, 300.0])
        .with_drag_and_drop(true);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../icon/icon.png")) {
        viewport = viewport.with_icon(icon);
    }
    eframe::run_native(
        APP_NAME,
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(move |cc| {
            if cfg!(windows) {
                cc.egui_ctx.set_theme(egui::Theme::Light);
            }
            let wake = cc.egui_ctx.clone();
            #[cfg(windows)]
            let updates = {
                let context = wake.clone();
                crate::updates::check(move || context.request_repaint())
            };
            let mut app = App {
                settings,
                queue: Queue::new(move || wake.request_repaint()),
                rows: Vec::new(),
                finished: 0,
                scans: 0,
                notice: String::new(),
                confirm_close: false,
                stopping: false,
                close_when_stopped: false,
                #[cfg(windows)]
                inbox,
                #[cfg(windows)]
                updates,
                #[cfg(windows)]
                available_update: None,
            };
            app.add(inputs);
            Ok(Box::new(app))
        }),
    )
}

#[derive(Default)]
enum Status {
    #[default]
    Waiting,
    Running(f32),
    Done(u64),
    Error(String),
    Cancelled,
}

struct Row {
    job: Job,
    details: Details,
    status: Status,
    alternate_used: bool,
}

struct App {
    settings: Settings,
    queue: Queue,
    rows: Vec<Row>,
    finished: usize,
    scans: usize,
    notice: String,
    confirm_close: bool,
    stopping: bool,
    close_when_stopped: bool,
    #[cfg(windows)]
    inbox: crate::inbox::Inbox,
    #[cfg(windows)]
    updates: Option<std::sync::mpsc::Receiver<String>>,
    #[cfg(windows)]
    available_update: Option<String>,
}

impl App {
    fn active(&self) -> bool {
        self.scans > 0 || self.finished < self.rows.len()
    }

    fn submit(&mut self, paths: Vec<PathBuf>, options: Options) {
        if paths.is_empty() || self.stopping {
            return;
        }
        self.notice.clear();
        self.queue.submit(paths, options);
        self.scans += 1;
    }

    fn stop(&mut self) {
        self.queue.cancel();
        self.stopping = true;
    }

    fn add(&mut self, paths: Vec<PathBuf>) {
        self.submit(
            paths,
            Options {
                video_profile: self.settings.video_profile,
                image_mode: self.settings.image_mode,
                ..Options::default()
            },
        );
    }

    fn receive(&mut self, ctx: &egui::Context) {
        for _ in 0..512 {
            let Ok(event) = self.queue.events.try_recv() else {
                return;
            };
            match event {
                Event::Queued(job) => {
                    debug_assert_eq!(job.id, self.rows.len());
                    self.rows.push(Row {
                        job,
                        details: Details::default(),
                        status: Status::Waiting,
                        alternate_used: false,
                    });
                }
                Event::Started(id) => self.rows[id].status = Status::Running(0.0),
                Event::Update(id, Update::Progress(value)) => {
                    self.rows[id].status = Status::Running(value)
                }
                Event::Update(id, Update::Details(details)) => self.rows[id].details = details,
                Event::Finished(id, result) => {
                    self.finished += 1;
                    self.rows[id].status = match result {
                        Ok(bytes) => Status::Done(bytes),
                        Err(error) => Status::Error(error),
                    };
                }
                Event::Cancelled(id) => {
                    self.finished += 1;
                    self.rows[id].status = Status::Cancelled;
                }
                Event::ScanProblem(message) => self.notice = message,
                Event::ScanDone => self.scans -= 1,
            }
        }
        ctx.request_repaint();
    }

    fn save(&mut self) {
        if let Err(error) = self.settings.save() {
            self.notice = format!("Could not save settings: {error}");
        }
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        let before = (self.settings.video_profile, self.settings.image_mode);
        ui.horizontal(|ui| {
            ui.label("Video");
            ComboBox::from_id_salt("video_profile")
                .width(88.0)
                .selected_text(self.settings.video_profile.label())
                .show_ui(ui, |ui| {
                    for &profile in VideoProfile::value_variants() {
                        ui.selectable_value(
                            &mut self.settings.video_profile,
                            profile,
                            profile.label(),
                        );
                    }
                });
            ui.label("Photo");
            ComboBox::from_id_salt("image_mode")
                .width(72.0)
                .selected_text(self.settings.image_mode.label())
                .show_ui(ui, |ui| {
                    for &mode in ImageMode::value_variants() {
                        ui.selectable_value(&mut self.settings.image_mode, mode, mode.label());
                    }
                });
            if ui
                .add_enabled(!self.stopping, egui::Button::new("Add Media"))
                .clicked()
            {
                let extensions = [VIDEO_EXTS, IMAGE_EXTS].concat();
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter("Media", &extensions)
                    .pick_files()
                {
                    self.add(files);
                }
            }
            if self.active()
                && ui
                    .add_enabled(!self.stopping, egui::Button::new("Stop"))
                    .clicked()
            {
                self.stop();
            }
            let total = self.rows.len();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("{}/{}", self.finished, total));
                let fraction = if total == 0 {
                    0.0
                } else {
                    self.finished as f32 / total as f32
                };
                progress(ui, ui.available_width().max(0.0), fraction, None);
            });
        });
        if before != (self.settings.video_profile, self.settings.image_mode) {
            self.save();
        }
    }
}
impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        let ctx = &context;
        self.receive(ctx);
        #[cfg(windows)]
        if let Some(tag) = self
            .updates
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok())
        {
            self.available_update = Some(tag);
            self.updates = None;
        }
        if !self.active() {
            self.confirm_close = false;
            self.stopping = false;
        }
        let dropped = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect::<Vec<_>>()
        });
        if !self.confirm_close && !self.close_when_stopped {
            self.add(dropped);
        }

        if let Some(size) = ctx.input(|i| i.viewport().inner_rect.map(|r| r.size()))
            && size.x >= 500.0
            && size.y >= 250.0
            && !ctx.input(|i| i.viewport().maximized == Some(true))
        {
            self.settings.window_size = Some([size.x, size.y]);
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.active() {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                if self.stopping {
                    self.close_when_stopped = true;
                } else {
                    self.confirm_close = true;
                }
            } else {
                self.save();
                return;
            }
        }
        if self.close_when_stopped && !self.active() {
            self.save();
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        #[cfg(windows)]
        if !self.stopping && !self.confirm_close && !self.close_when_stopped {
            let mut received = false;
            for _ in 0..32 {
                match self.inbox.receive() {
                    Ok(Some(paths)) => {
                        self.add(paths);
                        received = true;
                    }
                    Ok(None) => break,
                    Err(error) => {
                        self.notice = format!("Could not read Explorer request: {error:#}");
                        break;
                    }
                }
            }
            if received {
                ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(ViewportCommand::Focus);
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }

        egui::Panel::top("controls").show_inside(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("footer").show_inside(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.weak(format!("v{VERSION} · Muromtsev Nikita"));
                ui.hyperlink_to("nikmedoed.com", "https://nikmedoed.com/");
                #[cfg(windows)]
                if let Some(tag) = &self.available_update {
                    ui.hyperlink_to(format!("Update {tag}"), crate::updates::RELEASES);
                }
                if self.stopping {
                    ui.label("Stopping conversions…");
                }
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            if self.scans > 0 && !self.stopping {
                ui.label("Scanning folders…");
            }
            if !self.notice.is_empty() {
                ui.colored_label(ui.visuals().error_fg_color, &self.notice);
            }
            self.table(ui);
        });
        if self.confirm_close {
            let response =
                egui::Modal::new(egui::Id::new("confirm_stop_and_close")).show(ctx, |ui| {
                    ui.heading("Conversion in progress");
                    ui.label("Stop the current conversions and clear the waiting queue?");
                    ui.horizontal(|ui| {
                        let stop = ui.button("Stop and close").clicked();
                        let keep = ui.button("Keep open").clicked();
                        (stop, keep)
                    })
                });
            let (stop, keep) = response.inner.inner;
            if stop {
                self.confirm_close = false;
                self.close_when_stopped = true;
                self.stop();
            } else if keep || response.should_close() {
                self.confirm_close = false;
            }
        }
    }
}

const HEADINGS: [&str; 10] = [
    "Kind",
    "File",
    "Codec",
    "Duration",
    "Resolution",
    "Size",
    "Result",
    "Mode",
    "Alt",
    "Progress",
];
const WIDTHS: [f32; 10] = [48.0, 160.0, 50.0, 67.0, 80.0, 63.0, 66.0, 67.0, 33.0, 120.0];
const ROW_HEIGHT: f32 = 24.0;

fn progress(ui: &mut Ui, width: f32, fraction: f32, caption: Option<&str>) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 15.0), Sense::hover());
    let visuals = ui.visuals();
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, visuals.extreme_bg_color);
    let filled = rect.with_max_x(rect.left() + rect.width() * fraction.clamp(0.0, 1.0));
    if filled.width() > 0.0 {
        painter.rect_filled(filled, 2.0, visuals.selection.bg_fill);
    }
    painter.rect_stroke(
        rect,
        2.0,
        visuals.widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Inside,
    );
    if let Some(caption) = caption {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            caption,
            egui::FontId::proportional(11.0),
            visuals.text_color(),
        );
    }
}

fn cell(ui: &mut Ui, width: f32, label: impl Into<egui::WidgetText>, left: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, ROW_HEIGHT), Sense::click());
    let galley = label.into().into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        width - 8.0,
        egui::TextStyle::Body,
    );
    let x = if left {
        rect.left() + 4.0
    } else {
        rect.center().x - galley.size().x / 2.0
    };
    let y = rect.center().y - galley.size().y / 2.0;
    ui.painter()
        .with_clip_rect(rect)
        .galley(egui::pos2(x, y), galley, ui.visuals().text_color());
    response
}

impl Row {
    fn draw(&mut self, ui: &mut Ui, widths: [f32; 10], allow_retry: bool) -> (bool, bool) {
        let color = match (&self.status, ui.visuals().dark_mode) {
            (Status::Waiting | Status::Cancelled, _) => ui.visuals().extreme_bg_color,
            (Status::Running(_), false) => egui::Color32::from_rgb(255, 229, 180),
            (Status::Done(_), false) => egui::Color32::from_rgb(212, 247, 212),
            (Status::Error(_), false) => egui::Color32::from_rgb(247, 212, 212),
            (Status::Running(_), true) => egui::Color32::from_rgb(79, 64, 44),
            (Status::Done(_), true) => egui::Color32::from_rgb(53, 94, 53),
            (Status::Error(_), true) => egui::Color32::from_rgb(85, 51, 51),
        };
        let mut open = false;
        let mut alternate = false;
        let (value, result) = match &self.status {
            Status::Waiting => (0.0, String::new()),
            Status::Running(value) => (*value, String::new()),
            Status::Done(bytes) => (1.0, format!("{:.2} MB", mb(*bytes))),
            Status::Error(_) => (0.0, "Error".into()),
            Status::Cancelled => (0.0, "Cancelled".into()),
        };
        let kind = match self.job.kind {
            easy_media_compress::media::Kind::Image => "Image",
            easy_media_compress::media::Kind::Video => "Video",
        };
        let name = self
            .job
            .input
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let row_start = ui.next_widget_position();
        let width: f32 = widths.iter().sum();
        ui.painter().rect_filled(
            egui::Rect::from_min_size(row_start, egui::vec2(width, ROW_HEIGHT)),
            0.0,
            color,
        );
        ui.horizontal(|ui| {
            open |= cell(ui, widths[0], kind, false).double_clicked();
            open |= cell(ui, widths[1], name.as_ref(), true)
                .on_hover_text(self.job.input.display().to_string())
                .double_clicked();
            open |= cell(ui, widths[2], self.details.codec.as_str(), false).double_clicked();
            open |= cell(ui, widths[3], self.details.duration.as_str(), false).double_clicked();
            open |= cell(ui, widths[4], self.details.resolution.as_str(), false).double_clicked();
            open |= cell(
                ui,
                widths[5],
                format!("{:.2} MB", mb(self.job.input_bytes)),
                false,
            )
            .double_clicked();
            let result_cell = cell(ui, widths[6], result, false);
            open |= result_cell.double_clicked();
            match &self.status {
                Status::Error(error) => {
                    result_cell.on_hover_text(error);
                }
                Status::Done(_) => {
                    result_cell.on_hover_text(self.job.output.display().to_string());
                }
                _ => {}
            }
            open |=
                cell(ui, widths[7], self.job.options.label(self.job.kind), false).double_clicked();
            let alt = cell(
                ui,
                widths[8],
                if self.alternate_used { "OK" } else { "<>" },
                false,
            );
            if allow_retry
                && !self.alternate_used
                && alt
                    .on_hover_text("Run again with the alternate mode")
                    .clicked()
            {
                self.alternate_used = true;
                alternate = true;
            }
            if matches!(self.status, Status::Cancelled) {
                cell(ui, widths[9], "—", false);
            } else {
                ui.add_space(4.0);
                progress(
                    ui,
                    widths[9] - 8.0,
                    value,
                    Some(&format!("{:.0}%", value * 100.0)),
                );
            }
        });
        (open, alternate)
    }
}

impl App {
    fn table(&mut self, ui: &mut Ui) {
        let empty_rect = ui.available_rect_before_wrap();
        let available_width = ui.available_width();
        let mut widths = WIDTHS;
        widths[1] += (available_width - WIDTHS.iter().sum::<f32>() - 12.0).max(0.0);
        let width: f32 = widths.iter().sum();
        let mut alternate = None;
        let mut open_folder = None;
        egui::ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(width);
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                let header = egui::Rect::from_min_size(
                    ui.next_widget_position(),
                    egui::vec2(width, ROW_HEIGHT),
                );
                ui.painter()
                    .rect_filled(header, 0.0, ui.visuals().faint_bg_color);
                ui.painter().hline(
                    header.x_range(),
                    header.bottom(),
                    ui.visuals().widgets.noninteractive.bg_stroke,
                );
                ui.horizontal(|ui| {
                    for (heading, width) in HEADINGS.into_iter().zip(widths) {
                        cell(ui, width, RichText::new(heading).strong(), false);
                    }
                });
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(ui, ROW_HEIGHT, self.rows.len(), |ui, range| {
                        for row in &mut self.rows[range] {
                            ui.push_id(row.job.id, |ui| {
                                let (open, retry) = row.draw(ui, widths, !self.stopping);
                                if open {
                                    open_folder = row.job.input.parent().map(PathBuf::from);
                                }
                                if retry {
                                    alternate = Some((
                                        vec![row.job.input.clone()],
                                        row.job.options.alternate(row.job.kind),
                                    ));
                                }
                            });
                        }
                    });
            });
        if self.rows.is_empty() {
            let body =
                empty_rect.with_min_y((empty_rect.top() + ROW_HEIGHT).min(empty_rect.bottom()));
            ui.painter().text(
                body.center(),
                egui::Align2::CENTER_CENTER,
                "Drop files or folders here",
                egui::FontId::proportional(14.0),
                ui.visuals().text_color(),
            );
        }
        if let Some(folder) = open_folder
            && let Err(error) = open::that_detached(folder)
        {
            self.notice = format!("Could not open folder: {error}");
        }
        if let Some((paths, options)) = alternate {
            self.submit(paths, options);
        }
    }
}
