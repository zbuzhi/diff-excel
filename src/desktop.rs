use anyhow::Result;
use diff_excel::{
    cli::{default_output, Args},
    compare::{self, Difference, Kind, Options, Report},
    workbook::{match_all, SheetPair, Workbook},
};
use eframe::egui::{self, Color32, FontFamily, RichText, Stroke, Vec2};
use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    time::Duration,
};

const INK: Color32 = Color32::from_rgb(43, 43, 39);
const MUTED: Color32 = Color32::from_rgb(99, 96, 85);
const GREEN: Color32 = Color32::from_rgb(23, 119, 92);
const LINE: Color32 = Color32::from_rgb(66, 64, 56);
const BG: Color32 = Color32::from_rgb(245, 237, 216);
const CONTROL: Color32 = Color32::from_rgb(83, 81, 71);
const BLUE: Color32 = Color32::from_rgb(63, 88, 112);
const PAPER: Color32 = Color32::from_rgb(255, 252, 241);
const YELLOW: Color32 = Color32::from_rgb(232, 188, 79);
const RED: Color32 = Color32::from_rgb(164, 49, 56);

enum Event {
    Loaded(bool, std::result::Result<Box<Workbook>, String>),
    Progress(String),
    Finished(std::result::Result<Report, String>),
}

pub fn run(args: Args) -> Result<()> {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png"))?;
    let mut gpu = eframe::egui_wgpu::WgpuSetupCreateNew::default();
    // Do not fall back to the OpenGL stack responsible for Windows issue #4.
    #[cfg(target_os = "windows")]
    {
        gpu.instance_descriptor.backends =
            eframe::wgpu::Backends::DX12 | eframe::wgpu::Backends::VULKAN;
    }
    #[cfg(target_os = "macos")]
    {
        gpu.instance_descriptor.backends = eframe::wgpu::Backends::METAL;
    }
    if args.software {
        gpu.native_adapter_selector = Some(Arc::new(|adapters, surface| {
            adapters
                .iter()
                .find(|adapter| {
                    adapter.get_info().device_type == eframe::wgpu::DeviceType::Cpu
                        && surface.is_none_or(|s| adapter.is_surface_supported(s))
                })
                .cloned()
                .ok_or_else(|| "没有可用的软件图形适配器，请改用 --headless 模式".to_owned())
        }));
    }
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(icon)
            .with_inner_size([1180., 860.])
            .with_min_inner_size([900., 680.]),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            wgpu_setup: gpu.into(),
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native(
        "Diff Excel",
        native,
        Box::new(move |cc| {
            setup(&cc.egui_ctx);
            Ok(Box::new(Desktop::new(args, cc.egui_ctx.clone())))
        }),
    )
    .map_err(|e| anyhow::anyhow!("无法创建窗口：{e}"))
}

fn setup(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::light());
    let mut style = (*ctx.style()).clone();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = PAPER;
    style.visuals.widgets.noninteractive.bg_fill = PAPER;
    style.visuals.override_text_color = Some(INK);
    style.visuals.selection.bg_fill = Color32::from_rgb(214, 226, 234);
    style.visuals.selection.stroke = Stroke::new(1_f32, GREEN);
    style.visuals.widgets.inactive.bg_fill = PAPER;
    style.visuals.widgets.inactive.weak_bg_fill = PAPER;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.5_f32, CONTROL);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1_f32, MUTED);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1_f32, LINE);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1_f32, GREEN);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1_f32, INK);
    style.visuals.widgets.active.bg_stroke = Stroke::new(2_f32, GREEN);
    style.visuals.widgets.active.fg_stroke = Stroke::new(1_f32, INK);
    style.visuals.extreme_bg_color = PAPER;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(249, 224, 158);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(225, 233, 237);
    style.spacing.item_spacing = Vec2::new(10., 10.);
    style.spacing.button_padding = Vec2::new(12., 7.);
    style.spacing.interact_size.y = 36.;
    style.spacing.combo_height = 240.;
    style.spacing.scroll = egui::style::ScrollStyle {
        floating: false,
        bar_width: 10.,
        bar_inner_margin: 12.,
        bar_outer_margin: 6.,
        handle_min_length: 40.,
        foreground_color: true,
        ..egui::style::ScrollStyle::solid()
    };
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1_f32, BLUE);
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(14.));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(25.));
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(8);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(8);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(8);
    ctx.set_style(style);
    // Native font discovery supplies Chinese glyphs without shipping OS fonts.
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let families = [
        "PingFang SC",
        "Microsoft YaHei",
        "Noto Sans CJK SC",
        "WenQuanYi Zen Hei",
        "Arial Unicode MS",
    ];
    let mut fonts = egui::FontDefinitions::default();
    for family in families {
        if let Some(id) = db.query(&fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            ..Default::default()
        }) {
            db.with_face_data(id, |data, index| {
                let mut font = egui::FontData::from_owned(data.to_vec());
                font.index = index;
                fonts.font_data.insert("cjk".into(), font.into());
                fonts
                    .families
                    .get_mut(&FontFamily::Proportional)
                    .unwrap()
                    .push("cjk".into());
                fonts
                    .families
                    .get_mut(&FontFamily::Monospace)
                    .unwrap()
                    .push("cjk".into());
            });
            break;
        }
    }
    ctx.set_fonts(fonts);
}

struct Desktop {
    old: Option<Arc<Workbook>>,
    new: Option<Arc<Workbook>>,
    pairs: Vec<(SheetPair, bool)>,
    custom_old: Option<String>,
    custom_new: Option<String>,
    options: Options,
    report: Option<Report>,
    filter: usize,
    search: String,
    error: Option<String>,
    status: String,
    loading: usize,
    comparing: bool,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    context: egui::Context,
    details: Option<Difference>,
    view_results: bool,
    color_picker_open: bool,
}

impl Desktop {
    fn new(args: Args, context: egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            old: None,
            new: None,
            pairs: Vec::new(),
            custom_old: None,
            custom_new: None,
            options: Options {
                output: args.output.unwrap_or_else(default_output),
                keep_format: !args.no_format,
                comments: !args.no_comments,
                color: args.color,
            },
            report: None,
            filter: 0,
            search: String::new(),
            error: None,
            status: "选择两个工作簿，开始发现变化。".into(),
            loading: 0,
            comparing: false,
            tx,
            rx,
            context,
            details: None,
            view_results: false,
            color_picker_open: false,
        };
        if let Some(path) = args.old {
            app.load(true, path);
        }
        if let Some(path) = args.new {
            app.load(false, path);
        }
        app
    }

    fn busy(&self) -> bool {
        self.loading > 0 || self.comparing
    }

    fn load(&mut self, old: bool, path: PathBuf) {
        self.loading += 1;
        self.error = None;
        self.report = None;
        self.status = format!("正在读取 {}…", path.display());
        let tx = self.tx.clone();
        let ctx = self.context.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(|| Workbook::load(&path).map(Box::new))
                .map_err(|_| "Excel 解析异常，请确认文件有效或尝试另存为 .xlsx".to_owned())
                .and_then(|r| r.map_err(|e| format!("{e:#}")));
            let _ = tx.send(Event::Loaded(old, result));
            ctx.request_repaint();
        });
    }

    fn poll(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Loaded(is_old, result) => {
                    self.loading = self.loading.saturating_sub(1);
                    match result {
                        Ok(book) => {
                            if is_old {
                                self.old = Some(Arc::from(book));
                            } else {
                                self.new = Some(Arc::from(book));
                            }
                            self.reset_pairs();
                            if self.loading == 0 {
                                self.status = "工作簿已就绪，检查配对后开始对比。".into();
                            }
                        }
                        Err(error) => {
                            self.error = Some(error);
                            self.status = "文件读取失败，可重新选择。".into();
                        }
                    }
                }
                Event::Progress(text) => self.status = text,
                Event::Finished(result) => {
                    self.comparing = false;
                    match result {
                        Ok(report) => {
                            self.status = format!(
                                "对比完成 · {} 处变化 · {:.2} 秒",
                                report.total,
                                report.elapsed.as_secs_f64()
                            );
                            self.report = Some(report);
                            self.view_results = true;
                            self.filter = 0;
                            self.search.clear();
                        }
                        Err(error) => {
                            self.error = Some(error);
                            self.status = "对比失败，未生成完成报告。".into();
                        }
                    }
                }
            }
        }
    }

    fn reset_pairs(&mut self) {
        self.pairs.clear();
        self.details = None;
        self.custom_old = self.old.as_ref().and_then(|b| b.sheets().first().cloned());
        self.custom_new = self.new.as_ref().and_then(|b| b.sheets().first().cloned());
        if let (Some(old), Some(new)) = (&self.old, &self.new) {
            self.pairs = match_all(&old.sheets(), &new.sheets())
                .into_iter()
                .map(|p| (p, true))
                .collect();
        }
    }

    fn start(&mut self) {
        let (Some(old), Some(new)) = (self.old.clone(), self.new.clone()) else {
            return;
        };
        let pairs: Vec<_> = self
            .pairs
            .iter()
            .filter(|(_, selected)| *selected)
            .map(|(p, _)| p.clone())
            .collect();
        if pairs.is_empty() {
            self.error = Some("请至少选择一组工作表。".into());
            return;
        }
        if let Err(e) = compare::parse_color(&self.options.color) {
            self.error = Some(e.to_string());
            return;
        }
        self.error = None;
        self.report = None;
        self.details = None;
        self.comparing = true;
        self.color_picker_open = false;
        self.status = "正在准备对比…".into();
        let options = self.options.clone();
        let tx = self.tx.clone();
        let ctx = self.context.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                compare::compare(&old, &new, &pairs, &options, |text| {
                    let _ = tx.send(Event::Progress(text));
                    ctx.request_repaint();
                })
            }))
            .map_err(|_| "处理 Excel 时发生异常，请检查文件；原文件未被修改。".to_owned())
            .and_then(|r| r.map_err(|e| format!("{e:#}")));
            let _ = tx.send(Event::Finished(result));
            ctx.request_repaint();
        });
    }

    fn files(&mut self, ui: &mut egui::Ui) {
        let mut picked = None;
        ui.columns(2, |columns| {
            for (index, column) in columns.iter_mut().enumerate() {
                let old = index == 0;
                card(column, |ui| {
                    ui.set_min_height(120.);
                    ui.label(
                        RichText::new(if old {
                            "旧版本 / BEFORE"
                        } else {
                            "新版本 / AFTER"
                        })
                        .size(12.)
                        .color(MUTED),
                    );
                    let book = if old {
                        self.old.clone()
                    } else {
                        self.new.clone()
                    };
                    if let Some(book) = &book {
                        ui.label(
                            RichText::new(
                                book.path
                                    .file_name()
                                    .unwrap_or(book.path.as_os_str())
                                    .to_string_lossy(),
                            )
                            .size(18.)
                            .strong(),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(book.path.display().to_string())
                                    .size(11.)
                                    .color(MUTED),
                            )
                            .truncate(),
                        )
                        .on_hover_text(book.path.display().to_string());
                        ui.label(
                            RichText::new(if book.empty {
                                "空版本 · 用于新增 / 删除文件".into()
                            } else {
                                format!("{} 张工作表 · XLSX", book.sheets().len())
                            })
                            .color(GREEN),
                        );
                    } else {
                        ui.label(RichText::new("选择 Excel 工作簿").size(18.).strong());
                        ui.label(
                            RichText::new("支持 .xlsx，也可以拖入窗口")
                                .size(12.)
                                .color(MUTED),
                        );
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .button(if book.is_some() {
                                "更换文件…"
                            } else {
                                "浏览文件…"
                            })
                            .clicked()
                        {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Excel", &["xlsx"])
                                .pick_file()
                            {
                                picked = Some((old, path));
                            }
                        }
                        if book.is_some()
                            && ui.add(egui::Button::new("移除").frame(false)).clicked()
                        {
                            if old {
                                self.old = None;
                            } else {
                                self.new = None;
                            }
                            self.report = None;
                            self.reset_pairs();
                        }
                    });
                });
            }
        });
        if let Some((old, path)) = picked {
            self.load(old, path);
        }
    }

    fn pairing(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("工作表配对").size(17.).strong());
                ui.label(
                    RichText::new(format!(
                        "{} 组已选择",
                        self.pairs.iter().filter(|(_, s)| *s).count()
                    ))
                    .size(12.)
                    .color(MUTED),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("全部 Sheet").clicked() {
                        self.reset_pairs();
                        self.report = None;
                    }
                    if ui.add(egui::Button::new("全不选").frame(false)).clicked() {
                        for (_, selected) in &mut self.pairs {
                            *selected = false;
                        }
                        self.report = None;
                    }
                });
            });
            ui.label(
                RichText::new("按同名匹配；单侧工作表标记为新增或删除。名称不同？在下方手动配对。")
                    .size(12.)
                    .color(MUTED),
            );
            ui.add_space(4.);
            let mut remove = None;
            ui.vertical(|ui| {
                for (index, (pair, selected)) in self.pairs.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.checkbox(selected, "").changed() {
                            self.report = None;
                        }
                        let width = (ui.available_width() - 160.) / 2.;
                        table_text(
                            ui,
                            width.max(100.),
                            36.,
                            pair.old.as_deref().unwrap_or("—"),
                            INK,
                        );
                        ui.label(RichText::new("→").color(MUTED));
                        table_text(
                            ui,
                            width.max(100.),
                            36.,
                            pair.new.as_deref().unwrap_or("—"),
                            INK,
                        );
                        let (label, fg, bg) = if pair.old.is_none() {
                            ("新增", GREEN, Color32::from_rgb(230, 245, 236))
                        } else if pair.new.is_none() {
                            ("删除", RED, Color32::from_rgb(253, 236, 238))
                        } else {
                            ("已配对", MUTED, Color32::from_rgb(239, 243, 241))
                        };
                        ui.allocate_ui_with_layout(
                            Vec2::new(70., 36.),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                badge(ui, label, fg, bg);
                            },
                        );
                        if ui
                            .add_sized([32., 32.], egui::Button::new("×").frame(false))
                            .on_hover_text("移除此工作表配对")
                            .clicked()
                        {
                            remove = Some(index);
                        }
                    });
                }
                if self.pairs.is_empty() {
                    ui.label(RichText::new("加载两个文件后，会自动生成工作表配对。").color(MUTED));
                }
            });
            if let Some(index) = remove {
                self.pairs.remove(index);
                self.report = None;
            }
            if let (Some(old), Some(new)) = (&self.old, &self.new) {
                let old_names = old.sheets();
                let new_names = new.sheets();
                ui.separator();
                ui.horizontal(|ui| {
                    sheet_combo(ui, "custom_old", &mut self.custom_old, &old_names);
                    ui.label("→");
                    sheet_combo(ui, "custom_new", &mut self.custom_new, &new_names);
                    if ui.button("添加配对").clicked() {
                        if self.custom_old.is_none() && self.custom_new.is_none() {
                            self.error = Some("请至少选择一侧工作表。".into());
                        } else {
                            // A destination has one authoritative baseline. Replace
                            // conflicting auto-pairs instead of silently writing twice.
                            let pair = SheetPair {
                                old: self.custom_old.clone(),
                                new: self.custom_new.clone(),
                            };
                            self.pairs.retain(|(p, _)| {
                                !(pair.old.is_some() && p.old == pair.old
                                    || pair.new.is_some() && p.new == pair.new)
                            });
                            self.pairs.push((pair, true));
                            self.report = None;
                        }
                    }
                });
            }
        });
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("导出设置").size(17.).strong());
                ui.checkbox(&mut self.options.keep_format, "保留新文件格式");
                ui.checkbox(&mut self.options.comments, "批注显示旧内容");
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Excel 高亮").size(12.).color(MUTED));
                for (name, color) in [
                    ("暖黄", "#FFF0BD"),
                    ("浅绿", "#D6F0DF"),
                    ("浅红", "#FBDDDD"),
                    ("浅蓝", "#DCEBFF"),
                ] {
                    let selected = self.options.color.eq_ignore_ascii_case(color);
                    let rgb = u32::from_str_radix(&color[1..], 16).unwrap();
                    let fill = Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
                    let response = ui.add(
                        egui::Button::new(RichText::new(format!("    {name}")).color(INK))
                            .selected(selected)
                            .fill(fill)
                            .stroke(Stroke::new(
                                if selected { 2_f32 } else { 1_f32 },
                                if selected { GREEN } else { CONTROL },
                            )),
                    );
                    if selected {
                        let c = response.rect.left_center() + Vec2::new(12., 0.);
                        ui.painter().line_segment(
                            [c + Vec2::new(-4., 0.), c + Vec2::new(-1., 3.)],
                            Stroke::new(2_f32, GREEN),
                        );
                        ui.painter().line_segment(
                            [c + Vec2::new(-1., 3.), c + Vec2::new(5., -4.)],
                            Stroke::new(2_f32, GREEN),
                        );
                    }
                    if response.clicked() {
                        self.options.color = color.into();
                    }
                }
                let custom = !["#FFF0BD", "#D6F0DF", "#FBDDDD", "#DCEBFF"]
                    .iter()
                    .any(|color| self.options.color.eq_ignore_ascii_case(color));
                let response = ui.add(egui::Button::new("    自定义颜色…").stroke(Stroke::new(
                    if custom { 2_f32 } else { 1_f32 },
                    if custom { BLUE } else { CONTROL },
                )));
                ui.painter().circle_filled(
                    response.rect.left_center() + Vec2::new(13., 0.),
                    6.,
                    highlight_color(&self.options.color),
                );
                if response.clicked() {
                    self.color_picker_open = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("输出目录").size(12.).color(MUTED));
                ui.add(egui::Label::new(self.options.output.display().to_string()).truncate())
                    .on_hover_text(self.options.output.display().to_string());
                if ui.button("更改…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.options.output = path;
                    }
                }
            });
        });
    }

    fn results(&mut self, ui: &mut egui::Ui) {
        let Some(report) = &self.report else {
            return;
        };
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("本次对比").strong());
                let old = self
                    .old
                    .as_ref()
                    .map(|b| {
                        b.path
                            .file_name()
                            .unwrap_or(b.path.as_os_str())
                            .to_string_lossy()
                            .into_owned()
                    })
                    .unwrap_or_default();
                let new = self
                    .new
                    .as_ref()
                    .map(|b| {
                        b.path
                            .file_name()
                            .unwrap_or(b.path.as_os_str())
                            .to_string_lossy()
                            .into_owned()
                    })
                    .unwrap_or_default();
                ui.add(egui::Label::new(format!("{old}  →  {new}")).truncate());
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} 组工作表 · {:.2} 秒 · 原文件未修改",
                        report.sheets,
                        report.elapsed.as_secs_f64()
                    ))
                    .size(12.)
                    .color(MUTED),
                );
            });
        });
        ui.add_space(10.);
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("差异一览").size(19.).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("打开 Excel ↗").clicked() {
                        if let Err(e) = open::that(&report.excel) {
                            self.error = Some(format!("无法打开报告：{e}"));
                        }
                    }
                    if ui
                        .add(egui::Button::new("报告文件夹").frame(false))
                        .clicked()
                    {
                        if let Err(e) = open::that(report.excel.parent().unwrap()) {
                            self.error = Some(e.to_string());
                        }
                    }
                });
            });
            ui.horizontal_wrapped(|ui| {
                for (index, label, color) in [
                    (0, format!("全部 {}", report.total), INK),
                    (1, format!("新增 {}", report.added), GREEN),
                    (2, format!("删除 {}", report.removed), RED),
                    (3, format!("修改 {}", report.modified), BLUE),
                ] {
                    let selected = self.filter == index;
                    if ui
                        .add(
                            egui::Button::new(RichText::new(label).color(color))
                                .fill(if selected {
                                    Color32::from_rgb(241, 216, 151)
                                } else {
                                    PAPER
                                })
                                .stroke(Stroke::new(
                                    if selected { 2_f32 } else { 1_f32 },
                                    if selected { GREEN } else { CONTROL },
                                )),
                        )
                        .clicked()
                    {
                        self.filter = index;
                    }
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("搜索工作表、单元格或内容")
                        .font(egui::TextStyle::Body)
                        .margin(egui::Margin::symmetric(12, 9))
                        .min_size(Vec2::new(260., 38.))
                        .desired_width(260.),
                );
                if !self.search.is_empty()
                    && ui.add(egui::Button::new("清除").frame(false)).clicked()
                {
                    self.search.clear();
                }
            });
            ui.add_space(4.);
            let query = self.search.to_lowercase();
            let rows: Vec<_> = report
                .preview
                .iter()
                .filter(|d| {
                    (self.filter == 0
                        || match self.filter {
                            1 => matches!(d.kind, Kind::Added | Kind::SheetAdded),
                            2 => matches!(d.kind, Kind::Removed | Kind::SheetRemoved),
                            _ => d.kind == Kind::Modified,
                        })
                        && (query.is_empty()
                            || [&d.sheet, &d.cell, &d.old, &d.new]
                                .iter()
                                .any(|s| s.to_lowercase().contains(&query)))
                })
                .collect();
            // The header and every row share exact widths and left alignment.
            // Only this viewport scrolls; setup panels are a separate view.
            let width = ui.available_width() - 16. - ui.spacing().scroll.allocated_width();
            let flexible = (width - 88. - 66. - 40.).max(240.);
            let columns = [88., flexible * 0.30, 66., flexible * 0.35, flexible * 0.35];
            egui::Frame::new()
                .fill(Color32::from_rgb(230, 220, 195))
                .inner_margin(egui::Margin::symmetric(8, 0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (label, width) in ["类型", "工作表配对", "单元格", "旧内容", "新内容"]
                            .into_iter()
                            .zip(columns)
                        {
                            table_text(ui, width, 36., label, MUTED);
                        }
                    });
                });
            let table_height = (ui.available_height() - 65.).max(100.);
            egui::ScrollArea::vertical()
                .id_salt("difference_table")
                .auto_shrink([false, false])
                .min_scrolled_height(table_height)
                .max_height(table_height)
                .show_rows(ui, 40., rows.len(), |ui, range| {
                    for index in range {
                        let diff = rows[index];
                        let (fg, bg) = kind_colors(&diff.kind);
                        let next = egui::Rect::from_min_size(
                            ui.cursor().min,
                            Vec2::new(ui.available_width(), 40.),
                        );
                        let hovered = ui.rect_contains_pointer(next);
                        let fill = if hovered {
                            Color32::from_rgb(227, 235, 239)
                        } else if index % 2 == 0 {
                            PAPER
                        } else {
                            Color32::from_rgb(248, 243, 229)
                        };
                        let row = egui::Frame::new()
                            .fill(fill)
                            .inner_margin(egui::Margin::symmetric(8, 0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.allocate_ui_with_layout(
                                        Vec2::new(columns[0], 40.),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.set_min_width(columns[0]);
                                            badge(ui, diff.kind.label(), fg, bg);
                                        },
                                    );
                                    table_text(ui, columns[1], 40., &diff.sheet, INK);
                                    table_text(ui, columns[2], 40., &diff.cell, MUTED);
                                    table_text(
                                        ui,
                                        columns[3],
                                        40.,
                                        if diff.old.is_empty() {
                                            "—"
                                        } else {
                                            &diff.old
                                        },
                                        INK,
                                    );
                                    table_text(
                                        ui,
                                        columns[4],
                                        40.,
                                        if diff.new.is_empty() {
                                            "—"
                                        } else {
                                            &diff.new
                                        },
                                        INK,
                                    );
                                });
                            });
                        if ui
                            .interact(
                                row.response.rect,
                                ui.id().with(("difference", index)),
                                egui::Sense::click(),
                            )
                            .on_hover_text("点击查看完整旧内容和新内容")
                            .clicked()
                        {
                            self.details = Some(diff.clone());
                        }
                    }
                    if rows.is_empty() {
                        ui.add_space(32.);
                        ui.label(
                            RichText::new(if report.total == 0 {
                                "两个版本的数据一致"
                            } else {
                                "没有匹配的差异"
                            })
                            .size(18.)
                            .color(INK),
                        );
                        ui.label(
                            RichText::new(if report.total == 0 {
                                "已生成完整报告，可打开 Excel 查看。"
                            } else {
                                "试试其他类型，或清除搜索条件。"
                            })
                            .color(MUTED),
                        );
                    }
                });
            ui.separator();
            ui.label(
                RichText::new(format!(
                    "当前显示 {} 条 · 总计 {} 条 · 点击整行查看详情",
                    rows.len(),
                    report.total
                ))
                .size(12.)
                .color(MUTED),
            );
            if report.total > compare::PREVIEW_LIMIT {
                ui.label(
                    RichText::new(format!(
                        "预览及搜索限前 {} 条，完整内容见 changes.txt。",
                        compare::PREVIEW_LIMIT
                    ))
                    .size(12.)
                    .color(MUTED),
                );
            }
        });
    }
}

impl eframe::App for Desktop {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if !self.busy() {
            let paths: Vec<_> = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .iter()
                    .filter_map(|f| f.path.clone())
                    .take(2)
                    .collect()
            });
            if paths.len() == 2 {
                self.load(true, paths[0].clone());
                self.load(false, paths[1].clone());
            } else if let Some(path) = paths.into_iter().next() {
                self.load(self.old.is_none(), path);
            }
        }
        if self.report.is_none() {
            self.view_results = false;
            self.details = None;
        }
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::new().fill(PAPER).inner_margin(10.))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.busy() {
                        ui.spinner();
                    } else {
                        ui.label(RichText::new("●").color(if self.error.is_some() {
                            RED
                        } else {
                            GREEN
                        }));
                    }
                    ui.label(RichText::new(&self.status).size(12.).color(MUTED));
                });
            });
        if !self.view_results && !self.comparing {
            egui::TopBottomPanel::bottom("compare_action")
                .frame(
                    egui::Frame::new()
                        .fill(BG)
                        .inner_margin(egui::Margin::symmetric(22, 12)),
                )
                .show(ctx, |ui| {
                    let ready = !self.busy()
                        && self.old.is_some()
                        && self.new.is_some()
                        && self.pairs.iter().any(|(_, selected)| *selected);
                    let (area, _) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 88.),
                        egui::Sense::hover(),
                    );
                    let circle = egui::Rect::from_center_size(area.center(), Vec2::splat(88.));
                    let clicked = ui
                        .add_enabled_ui(ready, |ui| {
                            ui.put(
                                circle,
                                egui::Button::new(RichText::new("开始对比").strong().color(INK))
                                    .fill(YELLOW)
                                    .stroke(Stroke::new(2_f32, INK))
                                    .corner_radius(44.),
                            )
                        })
                        .inner
                        .clicked();
                    if clicked {
                        self.start();
                    }
                    if self.report.is_some() {
                        let next = egui::Rect::from_center_size(
                            egui::pos2(area.right() - 90., area.center().y),
                            Vec2::new(180., 38.),
                        );
                        if ui.put(next, egui::Button::new("查看上次结果 →")).clicked() {
                            self.view_results = true;
                        }
                    }
                });
        }
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(22, 12)),
            )
            .show(ctx, |ui| {
                let title = if self.comparing {
                    "正在对比"
                } else if self.view_results {
                    "对比结果"
                } else {
                    "选择要对比的两个 Excel 文件"
                };
                let instruction = if self.comparing {
                    "正在处理工作簿并生成报告，请稍候。"
                } else if self.view_results {
                    "在下方筛选或搜索差异，点击一行查看完整内容。"
                } else if self.loading > 0 {
                    "正在读取文件，完成后会自动配对同名工作表。"
                } else if self.old.is_none() && self.new.is_none() {
                    "先选择旧版本和新版本，再点击底部圆形按钮开始对比。"
                } else if self.old.is_none() {
                    "还需要选择旧版本文件，才能开始对比。"
                } else if self.new.is_none() {
                    "还需要选择新版本文件，才能开始对比。"
                } else if !self.pairs.iter().any(|(_, selected)| *selected) {
                    "请勾选至少一组工作表，再点击底部「开始对比」。"
                } else {
                    "文件已就绪。确认工作表配对后，点击底部「开始对比」。"
                };
                let (row, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), 40.),
                    egui::Sense::hover(),
                );
                let left = egui::Rect::from_min_size(row.min, Vec2::new(160., 40.));
                if self.view_results && !self.comparing {
                    if ui.put(left, egui::Button::new("← 返回文件设置")).clicked() {
                        self.view_results = false;
                        self.details = None;
                    }
                } else {
                    ui.put(
                        left,
                        egui::Label::new(RichText::new("Diff Excel").size(18.).strong())
                            .halign(egui::Align::Min),
                    );
                }
                let center =
                    egui::Rect::from_center_size(row.center(), Vec2::new(row.width() - 340., 40.));
                ui.put(
                    center,
                    egui::Label::new(RichText::new(title).size(23.).strong()),
                );
                let right =
                    egui::Rect::from_min_max(egui::pos2(row.right() - 160., row.top()), row.max);
                ui.put(
                    right,
                    egui::Label::new(RichText::new("本机处理 · 无需上传").size(12.).color(MUTED))
                        .halign(egui::Align::Max),
                );
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(instruction).size(13.).color(MUTED));
                });
                if let Some(error) = self.error.clone() {
                    ui.add_space(8.);
                    egui::Frame::new()
                        .fill(Color32::from_rgb(253, 236, 238))
                        .inner_margin(10.)
                        .corner_radius(6.)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(error).color(RED));
                                if ui.add(egui::Button::new("关闭").frame(false)).clicked() {
                                    self.error = None;
                                }
                            });
                        });
                }
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(22.))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let mut x = rect.left();
                while x < rect.right() {
                    let mut y = rect.top();
                    while y < rect.bottom() {
                        ui.painter().circle_filled(
                            egui::pos2(x, y),
                            0.65,
                            Color32::from_rgb(222, 212, 186),
                        );
                        y += 22.;
                    }
                    x += 22.;
                }
                if self.comparing {
                    ui.add_space((ui.available_height() * 0.28).max(20.));
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Spinner::new().size(48.).color(BLUE));
                        ui.add_space(18.);
                        ui.label(RichText::new("正在对比两个工作簿").size(22.).strong());
                        ui.label(RichText::new(&self.status).color(MUTED));
                        ui.add_space(8.);
                        ui.label(
                            RichText::new("完成后将自动显示差异结果")
                                .size(13.)
                                .color(MUTED),
                        );
                    });
                } else if self.view_results {
                    self.results(ui);
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("setup")
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        )
                        .show(ui, |ui| {
                            ui.add_enabled_ui(!self.busy(), |ui| {
                                self.files(ui);
                                ui.add_space(8.);
                                self.pairing(ui);
                                ui.add_space(8.);
                                self.settings(ui);
                            });
                            ui.add_space(12.);
                            ui.label(
                                RichText::new(
                                    "按坐标比较原始值、类型与公式 · 结果导出为 Excel 和完整日志",
                                )
                                .size(12.)
                                .color(MUTED),
                            );
                        });
                }
            });
        if self.color_picker_open {
            let mut open = true;
            let mut done = false;
            egui::Window::new("自定义高亮颜色")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(310.)
                .frame(
                    egui::Frame::new()
                        .fill(PAPER)
                        .stroke(Stroke::new(2_f32, INK))
                        .corner_radius(12.)
                        .inner_margin(14.),
                )
                .show(ctx, |ui| {
                    ui.spacing_mut().slider_width = 220.;
                    ui.label("选择导出 Excel 中差异单元格的高亮颜色。");
                    let mut color = highlight_color(&self.options.color);
                    if egui::color_picker::color_picker_color32(
                        ui,
                        &mut color,
                        egui::color_picker::Alpha::Opaque,
                    ) {
                        self.options.color =
                            format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b());
                    }
                    ui.horizontal(|ui| {
                        ui.label("HEX");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.options.color)
                                .desired_width(110.)
                                .margin(egui::Margin::symmetric(10, 8)),
                        );
                    });
                    let valid = compare::parse_color(&self.options.color).is_ok();
                    if !valid {
                        ui.colored_label(RED, "请输入 #RRGGBB 格式的颜色");
                    }
                    if ui.add_enabled(valid, egui::Button::new("完成")).clicked() {
                        done = true;
                    }
                });
            self.color_picker_open = open && !done;
        }
        if let Some(diff) = self.details.clone() {
            let mut open = true;
            egui::Window::new(format!("{} · {}", diff.sheet, diff.cell))
                .open(&mut open)
                .default_width(550.)
                .show(ctx, |ui| {
                    let (fg, bg) = kind_colors(&diff.kind);
                    badge(ui, diff.kind.label(), fg, bg);
                    ui.label("旧内容");
                    ui.add(
                        egui::TextEdit::multiline(&mut diff.old.clone())
                            .desired_rows(4)
                            .interactive(false),
                    );
                    ui.label("新内容");
                    ui.add(
                        egui::TextEdit::multiline(&mut diff.new.clone())
                            .desired_rows(4)
                            .interactive(false),
                    );
                });
            if !open {
                self.details = None;
            }
        }
    }
}

fn card<R>(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui) -> R) {
    egui::Frame::new()
        .fill(PAPER)
        .stroke(Stroke::new(2_f32, INK))
        .shadow(egui::epaint::Shadow {
            offset: [3, 4],
            blur: 0,
            spread: 0,
            color: INK,
        })
        .corner_radius(12.)
        .inner_margin(16.)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            content(ui);
        });
}

fn sheet_combo(ui: &mut egui::Ui, id: &str, selected: &mut Option<String>, names: &[String]) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected.as_deref().unwrap_or("（无工作表）"))
        .width(180.)
        .show_ui(ui, |ui| {
            ui.selectable_value(selected, None, "（无工作表）");
            for name in names {
                ui.selectable_value(selected, Some(name.clone()), name);
            }
        });
}

fn kind_colors(kind: &Kind) -> (Color32, Color32) {
    match kind {
        Kind::Added | Kind::SheetAdded => (GREEN, Color32::from_rgb(230, 245, 236)),
        Kind::Removed | Kind::SheetRemoved => (RED, Color32::from_rgb(253, 236, 238)),
        Kind::Modified => (BLUE, Color32::from_rgb(234, 240, 255)),
    }
}

fn badge(ui: &mut egui::Ui, text: &str, foreground: Color32, background: Color32) {
    egui::Frame::new()
        .fill(background)
        .stroke(Stroke::new(1_f32, foreground))
        .corner_radius(7.)
        .inner_margin(egui::Margin::symmetric(7, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.).color(foreground));
        });
}

fn table_text(ui: &mut egui::Ui, width: f32, height: f32, text: &str, color: Color32) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), text));
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_owned(), egui::FontId::proportional(14.), color);
    let position = egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.);
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .galley(position, galley, color);
    response.on_hover_text(text);
}

fn highlight_color(value: &str) -> Color32 {
    let hex = value.strip_prefix('#').unwrap_or(value);
    let rgb = if hex.len() == 6 {
        u32::from_str_radix(hex, 16).ok()
    } else {
        None
    }
    .unwrap_or(0xFFF0BD);
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}
