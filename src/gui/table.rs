use super::{App, Row, Status, progress};
use easy_media_compress::{cli::mb, media::Kind};
use eframe::egui::{self, RichText, Sense, Ui};
use std::path::PathBuf;

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
            Kind::Image => "Image",
            Kind::Video => "Video",
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
    pub(super) fn table(&mut self, ui: &mut Ui) {
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
