use anyhow::Result;
use diff_excel::{
    cli::Args,
    compare::{self, Kind, Options, Report},
    workbook::{match_all, SheetPair, Workbook},
};
use eframe::egui::{self, Color32, FontFamily, RichText, Stroke, Vec2};
use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    time::{Duration, Instant},
};

const INK: Color32 = Color32::from_rgb(43, 43, 39);
const MUTED: Color32 = Color32::from_rgb(99, 96, 85);
const GREEN: Color32 = Color32::from_rgb(23, 119, 92);
const LINE: Color32 = Color32::from_rgb(209, 201, 182);
const BG: Color32 = Color32::from_rgb(247, 242, 229);
const CONTROL: Color32 = Color32::from_rgb(128, 125, 111);
const BLUE: Color32 = Color32::from_rgb(63, 88, 112);
const PAPER: Color32 = Color32::from_rgb(255, 252, 241);
const YELLOW: Color32 = Color32::from_rgb(232, 188, 79);
const RED: Color32 = Color32::from_rgb(164, 49, 56);

use diff_excel::{
    analysis::{self, Analysis, Cancellation, Page, Query},
    preferences::{self, Preferences},
};
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
    style.visuals.selection.stroke = Stroke::new(1_f32, BLUE);
    style.visuals.widgets.inactive.bg_fill = PAPER;
    style.visuals.widgets.inactive.weak_bg_fill = PAPER;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.5_f32, CONTROL);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1_f32, MUTED);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1_f32, LINE);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1_f32, BLUE);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1_f32, INK);
    style.visuals.widgets.active.bg_stroke = Stroke::new(2_f32, BLUE);
    style.visuals.widgets.active.fg_stroke = Stroke::new(1_f32, INK);
    style.visuals.extreme_bg_color = PAPER;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(227, 236, 243);
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

struct Session {
    old: Arc<Workbook>,
    new: Arc<Workbook>,
    analysis: Analysis,
    revision: u64,
}
enum Event {
    Loaded(usize, u64, std::result::Result<Box<Workbook>, String>),
    Progress(String),
    Analyzed(std::result::Result<Arc<Session>, String>),
    Exported(std::result::Result<Report, String>),
    Queried(u64, std::result::Result<Page, String>),
}
struct Desktop {
    old: Option<Arc<Workbook>>,
    new: Option<Arc<Workbook>>,
    pairs: Vec<(SheetPair, bool)>,
    custom_old: Option<String>,
    custom_new: Option<String>,
    options: Options,
    preferences: Preferences,
    config: PathBuf,
    preference_due: Option<Instant>,
    session: Option<Arc<Session>>,
    report: Option<Report>,
    revision: u64,
    loading: [bool; 2],
    generations: [u64; 2],
    load_cancelled: [bool; 2],
    file_errors: [Option<String>; 2],
    pending: [Option<PathBuf>; 2],
    working: bool,
    exporting: bool,
    cancellation: Cancellation,
    query_cancel: Cancellation,
    query: Query,
    query_epoch: u64,
    query_due: Option<Instant>,
    querying: bool,
    page: Page,
    selected: Option<usize>,
    select_after_query: Option<bool>,
    scroll_reset: bool,
    scroll_selected: bool,
    split: f32,
    view_results: bool,
    color_picker_open: bool,
    error: Option<String>,
    status: String,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    context: egui::Context,
}
impl Desktop {
    fn new(args: Args, context: egui::Context) -> Self {
        let config = preferences::config_path();
        let preferences = Preferences::load(&config);
        let options = Options {
            output: args
                .output
                .clone()
                .unwrap_or_else(|| preferences.output.clone()),
            keep_format: if args.no_format {
                false
            } else {
                preferences.keep_format
            },
            comments: if args.no_comments {
                false
            } else {
                preferences.comments
            },
            color: args
                .color
                .clone()
                .unwrap_or_else(|| preferences.color.clone()),
        };
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            old: None,
            new: None,
            pairs: Vec::new(),
            custom_old: None,
            custom_new: None,
            options,
            preferences,
            config,
            preference_due: None,
            session: None,
            report: None,
            revision: 0,
            loading: [false; 2],
            generations: [0; 2],
            load_cancelled: [false; 2],
            file_errors: [None, None],
            pending: [None, None],
            working: false,
            exporting: false,
            cancellation: Cancellation::default(),
            query_cancel: Cancellation::default(),
            query: Query::default(),
            query_epoch: 0,
            query_due: None,
            querying: false,
            page: Page::default(),
            selected: None,
            select_after_query: None,
            scroll_reset: false,
            scroll_selected: false,
            split: 0.5,
            view_results: false,
            color_picker_open: false,
            error: None,
            status: "选择旧、新版本文件；文件只在本机处理。".into(),
            tx,
            rx,
            context,
        };
        if let Some(path) = args.old {
            app.load(0, path);
        }
        if let Some(path) = args.new {
            app.load(1, path);
        }
        app
    }
    fn busy(&self) -> bool {
        self.working || self.loading.iter().any(|v| *v)
    }
    fn stale(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|s| s.revision != self.revision)
    }
    fn changed(&mut self) {
        self.revision += 1;
    }
    fn selected_pairs(&self) -> Vec<SheetPair> {
        self.pairs
            .iter()
            .filter(|(_, selected)| *selected)
            .map(|(p, _)| p.clone())
            .collect()
    }
    fn ready_reason(&self) -> Option<&str> {
        if self.busy() {
            Some("正在处理，请稍候")
        } else if self.file_errors.iter().any(Option::is_some) {
            Some("文件读取失败，请重新选择对应文件")
        } else if self.old.is_none() {
            Some("还需要选择旧版本文件")
        } else if self.new.is_none() {
            Some("还需要选择新版本文件")
        } else if self.selected_pairs().is_empty() {
            Some("请至少选择一组工作表")
        } else {
            None
        }
    }
    fn load(&mut self, side: usize, path: PathBuf) {
        self.generations[side] += 1;
        let generation = self.generations[side];
        self.loading[side] = true;
        self.load_cancelled[side] = false;
        self.file_errors[side] = None;
        self.pending[side] = Some(path.clone());
        if side == 0 {
            self.old = None;
        } else {
            self.new = None;
        }
        self.pairs.clear();
        self.changed();
        self.error = None;
        self.status = format!("正在读取 {}…", path.display());
        let tx = self.tx.clone();
        let ctx = self.context.clone();
        std::thread::spawn(move || {
            let result = guarded(|| Workbook::load(&path).map(Box::new));
            let _ = tx.send(Event::Loaded(side, generation, result));
            ctx.request_repaint();
        });
    }
    fn reset_pairs(&mut self) {
        self.pairs = match (&self.old, &self.new) {
            (Some(a), Some(b)) => match_all(&a.sheets(), &b.sheets())
                .into_iter()
                .map(|p| (p, true))
                .collect(),
            _ => Vec::new(),
        };
        self.custom_old = self.old.as_ref().and_then(|b| b.sheets().first().cloned());
        self.custom_new = self.new.as_ref().and_then(|b| b.sheets().first().cloned());
    }
    fn remember_settings(&mut self) {
        self.preferences.output = self.options.output.clone();
        self.preferences.keep_format = self.options.keep_format;
        self.preferences.comments = self.options.comments;
        if compare::parse_color(&self.options.color).is_ok() {
            self.preferences.color = self.options.color.clone();
        }
        self.preference_due = Some(Instant::now() + Duration::from_millis(600));
        self.context
            .request_repaint_after(Duration::from_millis(600));
    }
    fn poll(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Loaded(side, generation, result) => {
                    if self.generations[side] != generation {
                        continue;
                    }
                    self.loading[side] = false;
                    if self.load_cancelled[side] {
                        self.file_errors[side] = Some("已取消读取，请重新选择文件。".into());
                        self.status = "已取消读取，未使用取消的文件。".into();
                        continue;
                    }
                    match result {
                        Ok(book) => {
                            self.preferences.remember(&book.path);
                            self.remember_settings();
                            if side == 0 {
                                self.old = Some(Arc::from(book));
                            } else {
                                self.new = Some(Arc::from(book));
                            }
                            self.reset_pairs();
                            self.status = if self.loading.iter().any(|v| *v) {
                                "正在读取另一个文件…"
                            } else if self.file_errors.iter().any(Option::is_some) {
                                "部分文件读取失败，请重新选择。"
                            } else {
                                "文件已加载，可检查配对并开始对比。"
                            }
                            .into();
                        }
                        Err(e) => {
                            self.file_errors[side] = Some(e);
                            self.status = "文件读取失败，请在对应卡片重新选择。".into();
                        }
                    }
                }
                Event::Progress(text) => self.status = text,
                Event::Analyzed(result) => {
                    self.working = false;
                    match result {
                        Ok(session) => {
                            self.status = format!(
                                "对比完成 · {} 处变化 · 尚未导出",
                                session.analysis.summary.total
                            );
                            self.session = Some(session);
                            self.report = None;
                            self.view_results = true;
                            self.query = Query::default();
                            self.page = Page::default();
                            self.selected = None;
                            self.schedule_query(false);
                        }
                        Err(e) => self.finish_error(e),
                    }
                }
                Event::Exported(result) => {
                    self.working = false;
                    self.exporting = false;
                    match result {
                        Ok(report) => {
                            self.status = "Excel 和完整日志已导出，可打开报告或文件夹。".into();
                            self.report = Some(report);
                        }
                        Err(e) => self.finish_error(e),
                    }
                }
                Event::Queried(epoch, result) => {
                    if epoch != self.query_epoch {
                        continue;
                    }
                    self.querying = false;
                    match result {
                        Ok(page) => {
                            self.page = page;
                            self.scroll_selected = true;
                            self.selected = self.select_after_query.take().and_then(|last| {
                                if self.page.rows.is_empty() {
                                    None
                                } else {
                                    Some(if last { self.page.rows.len() - 1 } else { 0 })
                                }
                            });
                        }
                        Err(e) => {
                            if !self.query_cancel.is_cancelled() {
                                self.error = Some(e);
                            }
                        }
                    }
                }
            }
        }
        if self.query_due.is_some_and(|t| Instant::now() >= t) {
            self.run_query();
        }
        if self.preference_due.is_some_and(|t| Instant::now() >= t) {
            self.preference_due = None;
            if let Err(e) = self.preferences.save(&self.config) {
                self.error = Some(format!("偏好设置未保存：{e}"));
            }
        }
    }
    fn finish_error(&mut self, e: String) {
        if self.cancellation.is_cancelled() {
            self.status = "已取消，未发布新的报告文件。".into();
        } else {
            self.error = Some(e);
            self.status = "处理失败，可调整文件或设置后重试。".into();
        }
    }
    fn start(&mut self) {
        if self.ready_reason().is_some() {
            return;
        }
        let old = self.old.clone().unwrap();
        let new = self.new.clone().unwrap();
        let pairs = self.selected_pairs();
        let revision = self.revision;
        self.query_cancel.cancel();
        self.query_epoch += 1;
        self.query_due = None;
        self.querying = false;
        self.session = None;
        self.report = None;
        self.selected = None;
        self.error = None;
        self.working = true;
        self.exporting = false;
        self.color_picker_open = false;
        self.cancellation = Cancellation::default();
        let cancel = self.cancellation.clone();
        self.status = "正在比较数据，完成后可先查看结果，再按需导出。".into();
        let tx = self.tx.clone();
        let ctx = self.context.clone();
        std::thread::spawn(move || {
            let result = guarded(|| {
                let analysis = analysis::analyze(&old, &new, &pairs, &cancel, |s| {
                    let _ = tx.send(Event::Progress(s));
                    ctx.request_repaint();
                })?;
                Ok(Arc::new(Session {
                    old,
                    new,
                    analysis,
                    revision,
                }))
            });
            let _ = tx.send(Event::Analyzed(result));
            ctx.request_repaint();
        });
    }
    fn export(&mut self) {
        if self.busy() {
            return;
        }
        let Some(session) = self.session.clone() else {
            return;
        };
        if let Err(e) = compare::parse_color(&self.options.color) {
            self.error = Some(e.to_string());
            return;
        }
        self.cancellation = Cancellation::default();
        let cancel = self.cancellation.clone();
        self.working = true;
        self.exporting = true;
        self.error = None;
        self.status = "正在按当前导出设置生成报告…".into();
        let options = self.options.clone();
        let tx = self.tx.clone();
        let ctx = self.context.clone();
        std::thread::spawn(move || {
            let result = guarded(|| {
                compare::export(
                    &session.old,
                    &session.new,
                    &session.analysis,
                    &options,
                    &cancel,
                    |s| {
                        let _ = tx.send(Event::Progress(s));
                        ctx.request_repaint();
                    },
                )
            });
            let _ = tx.send(Event::Exported(result));
            ctx.request_repaint();
        });
    }
    fn schedule_query(&mut self, debounce: bool) {
        self.query_cancel.cancel();
        self.query_epoch += 1;
        self.querying = true;
        self.selected = None;
        self.scroll_reset = true;
        self.query_due =
            Some(Instant::now() + Duration::from_millis(if debounce { 220 } else { 0 }));
        self.context
            .request_repaint_after(Duration::from_millis(if debounce { 220 } else { 0 }));
    }
    fn run_query(&mut self) {
        self.query_due = None;
        let Some(session) = self.session.clone() else {
            self.querying = false;
            return;
        };
        self.query_cancel = Cancellation::default();
        let cancel = self.query_cancel.clone();
        let query = self.query.clone();
        let epoch = self.query_epoch;
        let tx = self.tx.clone();
        let ctx = self.context.clone();
        std::thread::spawn(move || {
            let result = guarded(|| session.analysis.query(&query, &cancel));
            let _ = tx.send(Event::Queried(epoch, result));
            ctx.request_repaint();
        });
    }
    fn navigate(&mut self, direction: i32) {
        self.scroll_selected = true;
        if self.selected.is_none() && !self.page.rows.is_empty() {
            self.selected = Some(0);
            return;
        }
        if self.querying || self.page.rows.is_empty() {
            return;
        }
        let index = self.selected.unwrap_or(0) as i32 + direction;
        if index >= 0 && (index as usize) < self.page.rows.len() {
            self.selected = Some(index as usize);
        } else if index < 0 && self.query.offset > 0 {
            self.query.offset = self.query.offset.saturating_sub(self.query.limit);
            self.select_after_query = Some(true);
            self.schedule_query(false);
        } else if index as usize >= self.page.rows.len()
            && self.query.offset + self.page.rows.len() < self.page.total
        {
            self.query.offset += self.query.limit;
            self.select_after_query = Some(false);
            self.schedule_query(false);
        }
    }
    fn files(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("对比文件").size(17.).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.busy() && self.old.is_some() && self.new.is_some(),
                        egui::Button::new("交换旧／新版本").frame(false),
                    )
                    .clicked()
                {
                    std::mem::swap(&mut self.old, &mut self.new);
                    self.pending.swap(0, 1);
                    self.file_errors.swap(0, 1);
                    self.changed();
                    self.reset_pairs();
                    self.status = "已交换旧、新版本，重新对比后更新结果。".into();
                }
            });
        });
        let dropped: Vec<_> = self.context.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .take(2)
                .collect()
        });
        let position = self.context.input(|i| i.pointer.latest_pos());
        let hovering = self.context.input(|i| !i.raw.hovered_files.is_empty());
        let mut picked = None;
        let mut removed = None;
        let mut drop_rects = [egui::Rect::NOTHING; 2];
        ui.columns(2, |columns| {
            for (side, column) in columns.iter_mut().enumerate() {
                let book = if side == 0 {
                    self.old.clone()
                } else {
                    self.new.clone()
                };
                let error = self.file_errors[side].clone();
                let response = egui::Frame::new()
                    .fill(PAPER)
                    .stroke(Stroke::new(
                        1.2_f32,
                        if error.is_some() { RED } else { LINE },
                    ))
                    .corner_radius(12.)
                    .inner_margin(16.)
                    .show(column, |ui| {
                        ui.set_min_height(126.);
                        ui.label(
                            RichText::new(if side == 0 {
                                "旧版本 · BEFORE"
                            } else {
                                "新版本 · AFTER"
                            })
                            .color(BLUE)
                            .size(12.)
                            .strong(),
                        );
                        if self.loading[side] {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("正在读取文件…");
                            });
                        } else if let Some(book) = &book {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(file_name(&book.path)).size(17.).strong(),
                                )
                                .truncate(),
                            )
                            .on_hover_text(book.path.display().to_string());
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
                                    "空版本 · 用于新增／删除文件".into()
                                } else {
                                    format!("{} 张工作表 · 已加载", book.sheets().len())
                                })
                                .size(12.)
                                .color(GREEN),
                            );
                        } else if let Some(error) = &error {
                            ui.label(
                                RichText::new(
                                    self.pending[side]
                                        .as_deref()
                                        .map(file_name)
                                        .unwrap_or_default(),
                                )
                                .strong(),
                            );
                            ui.label(
                                RichText::new(friendly_file_error(error))
                                    .color(RED)
                                    .size(12.),
                            )
                            .on_hover_text(error);
                            ui.label(
                                RichText::new("此侧已清空，不会使用上一次的文件。")
                                    .size(12.)
                                    .color(MUTED),
                            );
                        } else {
                            ui.label(RichText::new("将 .xlsx 拖到这里").size(17.).strong());
                            ui.label(
                                RichText::new("也可以选择文件，或从最近记录中打开")
                                    .size(12.)
                                    .color(MUTED),
                            );
                        }
                        ui.horizontal(|ui| {
                            if ui
                                .button(if error.is_some() {
                                    "重新选择…"
                                } else if book.is_some() {
                                    "更换文件…"
                                } else {
                                    "选择文件…"
                                })
                                .clicked()
                            {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("Excel", &["xlsx"])
                                    .pick_file()
                                {
                                    picked = Some((side, path));
                                }
                            }
                            ui.menu_button("最近文件", |ui| {
                                if self.preferences.recent.is_empty() {
                                    ui.label("还没有最近文件");
                                }
                                for path in &self.preferences.recent {
                                    if ui
                                        .button(file_name(path))
                                        .on_hover_text(path.display().to_string())
                                        .clicked()
                                    {
                                        picked = Some((side, path.clone()));
                                        ui.close_menu();
                                    }
                                }
                                if !self.preferences.recent.is_empty() {
                                    ui.separator();
                                    if ui.button("清空最近记录").clicked() {
                                        self.preferences.recent.clear();
                                        self.remember_settings();
                                        ui.close_menu();
                                    }
                                }
                            });
                            if (book.is_some() || error.is_some())
                                && ui.add(egui::Button::new("移除").frame(false)).clicked()
                            {
                                removed = Some(side);
                            }
                        });
                    });
                drop_rects[side] = response.response.rect;
                if hovering && position.is_some_and(|p| response.response.rect.contains(p)) {
                    column.painter().rect_stroke(
                        response.response.rect,
                        12.,
                        Stroke::new(2_f32, BLUE),
                        egui::StrokeKind::Inside,
                    );
                }
            }
        });
        if let Some(side) = removed {
            if side == 0 {
                self.old = None;
            } else {
                self.new = None;
            }
            self.file_errors[side] = None;
            self.pending[side] = None;
            self.changed();
            self.reset_pairs();
        }
        if let Some((side, path)) = picked {
            self.load(side, path);
        }
        if !self.busy() {
            if dropped.len() == 2 {
                self.load(0, dropped[0].clone());
                self.load(1, dropped[1].clone());
            } else if let Some(path) = dropped.first() {
                let side = position
                    .and_then(|p| drop_rects.iter().position(|r| r.contains(p)))
                    .or_else(|| {
                        if self.old.is_none() {
                            Some(0)
                        } else if self.new.is_none() {
                            Some(1)
                        } else {
                            None
                        }
                    });
                if let Some(side) = side {
                    self.load(side, path.clone());
                } else {
                    self.error =
                        Some("请将文件拖到「旧版本」或「新版本」卡片，指定要替换的一侧。".into());
                }
            }
        }
    }
    fn pairing(&mut self, ui: &mut egui::Ui) {
        let unmatched = self
            .pairs
            .iter()
            .filter(|(p, _)| p.old.is_none() || p.new.is_none())
            .count();
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("工作表配对").size(17.).strong());
                ui.label(
                    RichText::new(format!("已选择 {} 组", self.selected_pairs().len()))
                        .color(MUTED),
                );
                if unmatched > 0 {
                    ui.colored_label(RED, format!("{unmatched} 组仅存在于一侧"));
                } else if !self.pairs.is_empty() {
                    ui.colored_label(GREEN, "同名工作表已自动配对");
                }
            });
            egui::CollapsingHeader::new(if unmatched > 0 {
                "检查未匹配工作表／调整配对"
            } else {
                "检查或手动调整配对"
            })
            .id_salt("pairing_details")
            .default_open(unmatched > 0)
            .show(ui, |ui| {
                ui.label(
                    RichText::new("单侧工作表按新增／删除处理；不同名称可在下方手动配对。")
                        .size(12.)
                        .color(MUTED),
                );
                ui.horizontal(|ui| {
                    if ui.button("恢复自动配对").clicked() {
                        self.reset_pairs();
                        self.changed();
                    }
                    for (label, value) in [("全选", true), ("全不选", false)] {
                        if ui.add(egui::Button::new(label).frame(false)).clicked() {
                            for (_, s) in &mut self.pairs {
                                *s = value;
                            }
                            self.changed();
                        }
                    }
                });
                let mut changed = false;
                let mut remove = None;
                egui::ScrollArea::vertical()
                    .id_salt("pair_list")
                    .max_height(200.)
                    .show(ui, |ui| {
                        for (index, (pair, selected)) in self.pairs.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                changed |= ui.checkbox(selected, "").changed();
                                let width = ((ui.available_width() - 150.) / 2.).max(80.);
                                table_text(
                                    ui,
                                    width,
                                    34.,
                                    pair.old.as_deref().unwrap_or("（无）"),
                                    INK,
                                );
                                ui.label("→");
                                table_text(
                                    ui,
                                    width,
                                    34.,
                                    pair.new.as_deref().unwrap_or("（无）"),
                                    INK,
                                );
                                ui.label(
                                    RichText::new(if pair.old.is_none() {
                                        "新增"
                                    } else if pair.new.is_none() {
                                        "删除"
                                    } else {
                                        "已配对"
                                    })
                                    .color(MUTED),
                                );
                                if ui
                                    .add(egui::Button::new("×").frame(false))
                                    .on_hover_text("移除此配对")
                                    .clicked()
                                {
                                    remove = Some(index);
                                }
                            });
                        }
                    });
                if let Some(index) = remove {
                    self.pairs.remove(index);
                    changed = true;
                }
                if changed {
                    self.changed();
                }
                if let (Some(old), Some(new)) = (&self.old, &self.new) {
                    let old_names = old.sheets();
                    let new_names = new.sheets();
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        sheet_combo(ui, "custom_old", &mut self.custom_old, &old_names);
                        ui.label("→");
                        sheet_combo(ui, "custom_new", &mut self.custom_new, &new_names);
                        if ui.button("添加配对").clicked() {
                            if self.custom_old.is_none() && self.custom_new.is_none() {
                                self.error = Some("请至少选择一侧工作表。".into());
                            } else {
                                let pair = SheetPair {
                                    old: self.custom_old.clone(),
                                    new: self.custom_new.clone(),
                                };
                                self.pairs.retain(|(p, _)| {
                                    !(pair.old.is_some() && p.old == pair.old
                                        || pair.new.is_some() && p.new == pair.new)
                                });
                                self.pairs.push((pair, true));
                                self.changed();
                            }
                        }
                    });
                }
            });
        });
    }
    fn settings(&mut self, ui: &mut egui::Ui) {
        let before = (
            self.options.keep_format,
            self.options.comments,
            self.options.color.clone(),
            self.options.output.clone(),
        );
        let compact = self.view_results;
        option_card(ui, compact, |ui| {
            egui::CollapsingHeader::new(format!(
                "导出设置 · {} · {} · {}",
                if self.options.keep_format {
                    "保留格式"
                } else {
                    "简洁格式"
                },
                if self.options.comments {
                    "含旧内容批注"
                } else {
                    "无批注"
                },
                self.options.color
            ))
            .id_salt("export_settings")
            .show(ui, |ui| {
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
                        let fill =
                            Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
                        let response = ui.add(
                            egui::Button::new(RichText::new(format!("    {name}")).color(INK))
                                .selected(selected)
                                .fill(fill)
                                .stroke(Stroke::new(
                                    if selected { 2_f32 } else { 1_f32 },
                                    if selected { BLUE } else { CONTROL },
                                )),
                        );
                        if selected {
                            let c = response.rect.left_center() + Vec2::new(12., 0.);
                            ui.painter().line_segment(
                                [c + Vec2::new(-4., 0.), c + Vec2::new(-1., 3.)],
                                Stroke::new(2_f32, BLUE),
                            );
                            ui.painter().line_segment(
                                [c + Vec2::new(-1., 3.), c + Vec2::new(5., -4.)],
                                Stroke::new(2_f32, BLUE),
                            );
                        }
                        if response.clicked() {
                            self.options.color = color.into();
                        }
                    }
                    let custom = !["#FFF0BD", "#D6F0DF", "#FBDDDD", "#DCEBFF"]
                        .iter()
                        .any(|color| self.options.color.eq_ignore_ascii_case(color));
                    let response =
                        ui.add(egui::Button::new("    自定义颜色…").stroke(Stroke::new(
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
                    ui.add_sized(
                        [(ui.available_width() - 90.).max(80.), 24.],
                        egui::Label::new(self.options.output.display().to_string()).truncate(),
                    )
                    .on_hover_text(self.options.output.display().to_string());
                    if ui.button("更改…").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.options.output = path;
                        }
                    }
                });
            });
            if !compact {
                ui.label(
                    RichText::new("先查看对比结果，再按需导出；调整这些选项无需重新比较。")
                        .size(12.)
                        .color(MUTED),
                );
            }
        });
        if before
            != (
                self.options.keep_format,
                self.options.comments,
                self.options.color.clone(),
                self.options.output.clone(),
            )
        {
            self.report = None;
            self.remember_settings();
        }
    }

    fn results(&mut self, ui: &mut egui::Ui) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let summary = &session.analysis.summary;
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "{}  →  {}",
                    file_name(&session.old.path),
                    file_name(&session.new.path)
                ))
                .strong(),
            );
            ui.label(
                RichText::new(format!(
                    "{} 组工作表 · 比较 {:.2} 秒",
                    session.analysis.pairs.len(),
                    session.analysis.elapsed.as_secs_f64()
                ))
                .size(12.)
                .color(MUTED),
            );
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(if self.report.is_some() {
                            "重新导出报告…"
                        } else if self.stale() {
                            "导出上次结果…"
                        } else {
                            "导出 Excel 和日志…"
                        })
                        .color(Color32::WHITE),
                    )
                    .fill(BLUE),
                )
                .clicked()
            {
                self.export();
            }
            if let Some(report) = &self.report {
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
            } else {
                ui.label(
                    RichText::new("完整差异已缓存在本机，尚未生成 Excel。")
                        .size(12.)
                        .color(MUTED),
                );
            }
        });
        if self.stale() {
            notice(
                ui,
                "文件或配对已改变：当前显示的是上次结果。返回设置重新对比后更新。",
                BLUE,
            );
        }
        ui.add_space(6.);
        ui.separator();
        let before = self.query.clone();
        let mut text_changed = false;
        ui.horizontal_wrapped(|ui| {
            for (index, label) in [
                (0, format!("全部 {}", summary.total)),
                (1, format!("新增 {}", summary.added)),
                (2, format!("删除 {}", summary.removed)),
                (3, format!("修改 {}", summary.modified)),
            ] {
                if ui
                    .add(
                        egui::Button::new(RichText::new(label).color(
                            if self.query.kind == index {
                                Color32::WHITE
                            } else {
                                INK
                            },
                        ))
                        .fill(if self.query.kind == index {
                            BLUE
                        } else {
                            PAPER
                        }),
                    )
                    .clicked()
                {
                    self.query.kind = index;
                }
            }
            egui::ComboBox::from_id_salt("sheet_filter")
                .width(145.)
                .selected_text(self.query.sheet.as_deref().unwrap_or("全部工作表"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.query.sheet, None, "全部工作表");
                    for pair in &session.analysis.pairs {
                        let label = pair.label();
                        ui.selectable_value(&mut self.query.sheet, Some(label.clone()), label);
                    }
                });
        });
        ui.horizontal_wrapped(|ui| {
            let width = ui.available_width().clamp(180., 400.);
            text_changed = ui
                .add(
                    egui::TextEdit::singleline(&mut self.query.text)
                        .id(egui::Id::new("search"))
                        .hint_text("搜索全部差异：工作表、单元格、内容")
                        .desired_width(width)
                        .margin(egui::Margin::symmetric(12, 9))
                        .min_size(Vec2::new(180., 38.)),
                )
                .changed();
            if ui.button("清除筛选").clicked() {
                self.query = Query::default();
            }
        });
        if before != self.query {
            self.query.offset = 0;
            self.select_after_query = None;
            self.schedule_query(text_changed);
        }
        ui.add_space(4.);
        let widths = {
            let w = (ui.available_width() - 16. - ui.spacing().scroll.allocated_width() - 40.)
                .max(280.);
            let fixed = if w < 700. {
                [56., 85., 50.]
            } else {
                [76., 160., 60.]
            };
            let rest = (w - fixed.iter().sum::<f32>()).max(100.);
            [
                fixed[0],
                fixed[1],
                fixed[2],
                rest * self.split,
                rest * (1. - self.split),
            ]
        };
        let header = egui::Frame::new()
            .fill(Color32::from_rgb(235, 230, 215))
            .inner_margin(egui::Margin::symmetric(8, 0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (label, width) in ["类型", "工作表配对", "单元格", "旧内容", "新内容"]
                        .into_iter()
                        .zip(widths)
                    {
                        table_text(ui, width, 34., label, MUTED);
                    }
                });
            });
        let boundary = header.response.rect.left()
            + 8.
            + widths[0]
            + widths[1]
            + widths[2]
            + widths[3]
            + ui.spacing().item_spacing.x * 3.5;
        let handle = egui::Rect::from_center_size(
            egui::pos2(boundary, header.response.rect.center().y),
            Vec2::new(12., 34.),
        );
        let drag = ui
            .interact(handle, ui.id().with("column_resize"), egui::Sense::drag())
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
            .on_hover_text("拖动调整旧／新内容列宽");
        ui.painter().line_segment(
            [
                handle.center_top() + Vec2::new(0., 8.),
                handle.center_bottom() - Vec2::new(0., 8.),
            ],
            Stroke::new(2_f32, CONTROL),
        );
        if drag.dragged() {
            self.split =
                (self.split + drag.drag_delta().x / (widths[3] + widths[4])).clamp(0.2, 0.8);
        }
        let height = (ui.available_height() - 64.).max(80.);
        let mut scroll = egui::ScrollArea::vertical()
            .id_salt("difference_table")
            .auto_shrink([false, false])
            .max_height(height)
            .min_scrolled_height(height);
        if self.scroll_reset {
            scroll = scroll.vertical_scroll_offset(0.);
            self.scroll_reset = false;
        } else if self.scroll_selected {
            if let Some(index) = self.selected {
                scroll = scroll.vertical_scroll_offset(index as f32 * 50.);
            }
        }
        self.scroll_selected = false;
        scroll.show_rows(ui, 40., self.page.rows.len(), |ui, range| {
            for index in range {
                let diff = &self.page.rows[index];
                let (fg, bg) = kind_colors(&diff.kind);
                let next = egui::Rect::from_min_size(
                    ui.cursor().min,
                    Vec2::new(ui.available_width(), 40.),
                );
                let fill = if self.selected == Some(index) {
                    Color32::from_rgb(223, 233, 240)
                } else if ui.rect_contains_pointer(next) {
                    Color32::from_rgb(237, 242, 245)
                } else if index % 2 == 0 {
                    PAPER
                } else {
                    Color32::from_rgb(247, 244, 234)
                };
                let row = egui::Frame::new()
                    .fill(fill)
                    .inner_margin(egui::Margin::symmetric(8, 0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(
                                Vec2::new(widths[0], 40.),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.set_min_width(widths[0]);
                                    badge(
                                        ui,
                                        match diff.kind {
                                            Kind::SheetAdded => "新增表",
                                            Kind::SheetRemoved => "删除表",
                                            _ => diff.kind.label(),
                                        },
                                        fg,
                                        bg,
                                    );
                                },
                            );
                            table_text(ui, widths[1], 40., &diff.sheet, INK);
                            table_text(ui, widths[2], 40., &diff.cell, MUTED);
                            table_text(
                                ui,
                                widths[3],
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
                                widths[4],
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
                if !self.querying
                    && ui
                        .interact(
                            row.response.rect,
                            ui.id().with(("row", index)),
                            egui::Sense::click(),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text("查看完整内容，支持上一条／下一条")
                        .clicked()
                {
                    self.selected = Some(index);
                }
            }
            if self.page.rows.is_empty() && !self.querying {
                ui.add_space(25.);
                ui.label(
                    RichText::new(if summary.total == 0 {
                        "两个版本的数据一致"
                    } else {
                        "没有匹配的差异"
                    })
                    .size(18.)
                    .strong(),
                );
                ui.label(if summary.total == 0 {
                    "无需处理，也可以导出报告留档。"
                } else {
                    "试试清除筛选，或更换搜索词。"
                });
            }
        });
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "第 {}–{} 条 / 匹配 {} 条 · 全部 {} 条",
                    if self.page.rows.is_empty() {
                        0
                    } else {
                        self.query.offset + 1
                    },
                    self.query.offset + self.page.rows.len(),
                    self.page.total,
                    summary.total
                ))
                .size(12.)
                .color(MUTED),
            );
            if ui
                .add_enabled(
                    !self.querying && self.query.offset > 0,
                    egui::Button::new("← 上一页"),
                )
                .clicked()
            {
                self.query.offset = self.query.offset.saturating_sub(self.query.limit);
                self.schedule_query(false);
            }
            if ui
                .add_enabled(
                    !self.querying && self.query.offset + self.page.rows.len() < self.page.total,
                    egui::Button::new("下一页 →"),
                )
                .clicked()
            {
                self.query.offset += self.query.limit;
                self.schedule_query(false);
            }
        });
    }
    fn details(&mut self, ui: &mut egui::Ui) {
        let Some(index) = self.selected else {
            return;
        };
        let Some(diff) = self.page.rows.get(index).cloned() else {
            return;
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new("差异详情").size(18.).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("关闭 ×").clicked() {
                    self.selected = None;
                }
            });
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.querying && (index > 0 || self.query.offset > 0),
                    egui::Button::new("← 上一条"),
                )
                .clicked()
            {
                self.navigate(-1);
            }
            if ui
                .add_enabled(
                    !self.querying && self.query.offset + index + 1 < self.page.total,
                    egui::Button::new("下一条 →"),
                )
                .clicked()
            {
                self.navigate(1);
            }
        });
        ui.label(RichText::new(format!("{} · {}", diff.sheet, diff.cell)).strong());
        let (fg, bg) = kind_colors(&diff.kind);
        badge(
            ui,
            &format!("{} · {}", diff.kind.label(), diff.reason),
            fg,
            bg,
        );
        ui.add_space(6.);
        egui::ScrollArea::vertical()
            .id_salt("detail_content")
            .show(ui, |ui| {
                for (label, value, other, color, fill) in [
                    (
                        "旧内容",
                        &diff.old,
                        &diff.new,
                        RED,
                        Color32::from_rgb(251, 226, 225),
                    ),
                    (
                        "新内容",
                        &diff.new,
                        &diff.old,
                        GREEN,
                        Color32::from_rgb(222, 240, 226),
                    ),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(label).strong());
                        if ui
                            .add_sized([54., 30.], egui::Button::new("复制"))
                            .clicked()
                        {
                            ui.ctx().copy_text(value.clone());
                            self.status = format!("已复制{label}");
                        }
                    });
                    if diff.reason == "类型变化" {
                        ui.label(
                            RichText::new(format!(
                                "数据类型：{}",
                                if label == "旧内容" {
                                    type_label(&diff.old_type)
                                } else {
                                    type_label(&diff.new_type)
                                }
                            ))
                            .size(12.)
                            .color(MUTED),
                        );
                    }
                    egui::Frame::new()
                        .fill(PAPER)
                        .stroke(Stroke::new(1_f32, LINE))
                        .corner_radius(8.)
                        .inner_margin(12.)
                        .show(ui, |ui| {
                            ui.set_min_width((ui.available_width() - 2.).max(100.));
                            if value.is_empty() {
                                ui.label(RichText::new("（空）").color(MUTED));
                            } else {
                                let (a, b, c) = analysis::changed_parts(value, other);
                                let mut job = egui::text::LayoutJob::default();
                                for (text, changed) in [(a, false), (b, true), (c, false)] {
                                    job.append(
                                        text,
                                        0.,
                                        egui::TextFormat {
                                            font_id: egui::FontId::proportional(14.),
                                            color: if changed { color } else { INK },
                                            background: if changed {
                                                fill
                                            } else {
                                                Color32::TRANSPARENT
                                            },
                                            ..Default::default()
                                        },
                                    );
                                }
                                ui.add(egui::Label::new(job).wrap().selectable(true));
                            }
                        });
                    ui.add_space(10.);
                }
                ui.label(
                    RichText::new(
                        "着色部分为变化范围；类型或公式缓存变化也会单独标记。↑ ↓ 浏览，Esc 关闭。",
                    )
                    .size(12.)
                    .color(MUTED),
                );
            });
    }
}
impl eframe::App for Desktop {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        if self.busy() || self.querying {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        let search_key = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F));
        if search_key && self.view_results {
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("search")));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.selected = None;
            if self.color_picker_open && compare::parse_color(&self.options.color).is_err() {
                self.options.color = self.preferences.color.clone();
            }
            self.color_picker_open = false;
        }
        if !ctx.wants_keyboard_input() && self.view_results && !self.busy() {
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                self.navigate(1);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                self.navigate(-1);
            }
        }
        if !self.view_results
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter))
        {
            self.start();
        }
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::new().fill(PAPER).inner_margin(9.))
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
                    ui.add(
                        egui::Label::new(RichText::new(&self.status).size(12.).color(MUTED))
                            .truncate(),
                    )
                    .on_hover_text(&self.status);
                });
            });
        if !self.view_results && !self.working {
            egui::TopBottomPanel::bottom("compare_action")
                .frame(
                    egui::Frame::new()
                        .fill(BG)
                        .inner_margin(egui::Margin::symmetric(22, 8)),
                )
                .show(ctx, |ui| {
                    let (area, _) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 78.),
                        egui::Sense::hover(),
                    );
                    let circle = egui::Rect::from_center_size(area.center(), Vec2::splat(76.));
                    let ready = self.ready_reason().is_none();
                    if ui
                        .add_enabled_ui(ready, |ui| {
                            ui.spacing_mut().button_padding.x = 6.;
                            ui.put(
                                circle,
                                egui::Button::new(
                                    RichText::new("开始对比").size(14.).strong().color(INK),
                                )
                                .wrap_mode(egui::TextWrapMode::Extend)
                                .fill(YELLOW)
                                .stroke(Stroke::new(1.5_f32, INK))
                                .corner_radius(38.),
                            )
                        })
                        .inner
                        .clicked()
                    {
                        self.start();
                    }
                    if self.session.is_some() {
                        let next = egui::Rect::from_center_size(
                            egui::pos2(area.right() - 90., area.center().y),
                            Vec2::new(180., 38.),
                        );
                        if ui
                            .add_enabled_ui(!self.busy(), |ui| {
                                ui.put(next, egui::Button::new("查看上次结果 →"))
                            })
                            .inner
                            .clicked()
                        {
                            self.view_results = true;
                        }
                    }
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(
                                self.ready_reason()
                                    .unwrap_or("就绪 · ⌘ / Ctrl + Enter 开始对比"),
                            )
                            .size(12.)
                            .color(MUTED),
                        );
                    });
                });
        }
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(22, 12)),
            )
            .show(ctx, |ui| {
                let (row, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), 40.),
                    egui::Sense::hover(),
                );
                let left = egui::Rect::from_min_size(row.min, Vec2::new(160., 40.));
                if self.view_results {
                    if ui
                        .add_enabled_ui(!self.busy(), |ui| {
                            ui.put(left, egui::Button::new("← 返回文件设置"))
                        })
                        .inner
                        .clicked()
                    {
                        self.view_results = false;
                    }
                } else {
                    ui.put(
                        left,
                        egui::Label::new(RichText::new("Diff Excel").size(18.).strong())
                            .halign(egui::Align::Min),
                    );
                }
                ui.put(
                    egui::Rect::from_center_size(
                        row.center(),
                        Vec2::new((row.width() - 340.).max(200.), 40.),
                    ),
                    egui::Label::new(
                        RichText::new(if self.working && !self.exporting {
                            "正在对比"
                        } else if self.view_results {
                            "对比结果"
                        } else {
                            "选择要对比的两个 Excel 文件"
                        })
                        .size(22.)
                        .strong(),
                    ),
                );
                ui.put(
                    egui::Rect::from_min_max(egui::pos2(row.right() - 160., row.top()), row.max),
                    egui::Label::new(RichText::new("本机处理 · 无需上传").size(12.).color(MUTED))
                        .halign(egui::Align::Max),
                );
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(if self.view_results {
                            "搜索覆盖全部差异 · 点击一行查看详情"
                        } else {
                            "选择旧、新文件，确认配对后点击底部圆形按钮"
                        })
                        .size(12.)
                        .color(MUTED),
                    );
                });
                if let Some(error) = self.error.clone() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(error).color(RED));
                        if ui.small_button("关闭提示").clicked() {
                            self.error = None;
                        }
                    });
                }
                if self.loading.iter().any(|v| *v) {
                    ui.horizontal_wrapped(|ui| {
                        ui.spinner();
                        ui.label(if self.load_cancelled.iter().any(|v| *v) {
                            "正在取消读取，当前解析完成后释放文件…"
                        } else {
                            "正在读取工作簿…"
                        });
                        if ui
                            .add_enabled(
                                !self.load_cancelled.iter().any(|v| *v),
                                egui::Button::new("取消读取"),
                            )
                            .clicked()
                        {
                            for side in 0..2 {
                                self.load_cancelled[side] = self.loading[side];
                            }
                        }
                    });
                }
                if self.working {
                    ui.horizontal_wrapped(|ui| {
                        ui.spinner();
                        ui.label(&self.status);
                        if ui
                            .add_enabled(
                                !self.cancellation.is_cancelled(),
                                egui::Button::new("取消操作"),
                            )
                            .clicked()
                        {
                            self.cancellation.cancel();
                            self.status =
                                "正在取消；若正在保存 Excel，将在当前保存阶段结束后清理。".into();
                        }
                    });
                }
            });
        if self.view_results && self.selected.is_some() {
            egui::SidePanel::right("details")
                .resizable(true)
                .default_width(330.)
                .min_width(260.)
                .max_width(480.)
                .frame(
                    egui::Frame::new()
                        .fill(Color32::from_rgb(240, 237, 226))
                        .inner_margin(14.),
                )
                .show(ctx, |ui| self.details(ui));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(18.))
            .show(ctx, |ui| {
                if self.working && !self.exporting {
                    ui.add_space((ui.available_height() * 0.25).max(20.));
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Spinner::new().size(44.).color(BLUE));
                        ui.add_space(16.);
                        ui.label(RichText::new("正在查找两个版本的变化").size(22.).strong());
                        ui.label(&self.status);
                        ui.label(
                            RichText::new("完成后先显示结果，不会自动导出 Excel。").color(MUTED),
                        );
                    });
                } else if self.view_results {
                    ui.add_enabled_ui(!self.working, |ui| {
                        self.settings(ui);
                        ui.add_space(6.);
                        self.results(ui);
                    });
                } else {
                    if self.stale() {
                        notice(
                            ui,
                            "文件或配对已更改，需要重新对比；仍可查看上次结果。",
                            BLUE,
                        );
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("setup")
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        )
                        .show(ui, |ui| {
                            ui.add_enabled_ui(!self.busy(), |ui| {
                                self.files(ui);
                                ui.add_space(12.);
                                self.pairing(ui);
                                ui.add_space(12.);
                                self.settings(ui);
                            });
                            ui.add_space(10.);
                            ui.label(
                                RichText::new(
                                    "按单元格坐标比较内容、类型和公式；不识别移动行和纯样式变化。",
                                )
                                .size(12.)
                                .color(MUTED),
                            );
                        });
                }
            });
        if self.color_picker_open {
            let previous = self.options.color.clone();
            let mut open = true;
            let mut done = false;
            egui::Window::new("自定义高亮颜色")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(310.)
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
            if !self.color_picker_open && compare::parse_color(&self.options.color).is_err() {
                self.options.color = self.preferences.color.clone();
            }
            if self.options.color != previous {
                self.report = None;
                self.remember_settings();
            }
        }
    }
    fn on_exit(&mut self) {
        self.cancellation.cancel();
        self.query_cancel.cancel();
        if self.preference_due.is_some() {
            let _ = self.preferences.save(&self.config);
        }
    }
}
fn guarded<T>(f: impl FnOnce() -> Result<T>) -> std::result::Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .map_err(|_| "处理文件时发生异常，请确认 Excel 有效或尝试另存为 .xlsx。".to_owned())
        .and_then(|r| r.map_err(|e| format!("{e:#}")))
}
fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}
fn notice(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(Color32::from_rgb(228, 236, 241))
        .corner_radius(7.)
        .inner_margin(10.)
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.).color(color));
        });
}
fn card<R>(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui) -> R) {
    egui::Frame::new()
        .fill(PAPER)
        .stroke(Stroke::new(1_f32, LINE))
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
    let mut job = egui::text::LayoutJob::simple(
        text.to_owned(),
        egui::FontId::proportional(14.),
        color,
        width,
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    let galley = ui.painter().layout_job(job);
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

fn type_label(value: &str) -> &str {
    match value {
        "s" | "str" | "inlineStr" => "文本",
        "n" => "数字",
        "b" => "布尔值",
        "e" => "错误值",
        "" => "空值",
        _ => value,
    }
}

fn option_card(ui: &mut egui::Ui, compact: bool, content: impl FnOnce(&mut egui::Ui)) {
    if compact {
        content(ui);
    } else {
        card(ui, content);
    }
}

fn friendly_file_error(error: &str) -> &str {
    if error.contains("无法读取文件") {
        "找不到或无法访问文件，请检查路径与读取权限。"
    } else if error.contains("目前支持") {
        "请选择 .xlsx 格式的文件。"
    } else if error.contains("已取消") {
        "已取消读取，请重新选择文件。"
    } else {
        "无法读取此工作簿，请确认文件有效且未加密，或在 Excel／WPS 中另存为 .xlsx。"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    fn app() -> Desktop {
        Desktop::new(Args::parse_from(["diff-excel"]), egui::Context::default())
    }
    fn book() -> Arc<Workbook> {
        Arc::new(Workbook {
            path: PathBuf::from("old.xlsx"),
            book: umya_spreadsheet::new_file(),
            empty: false,
        })
    }
    #[test]
    fn failed_replacement_clears_the_old_input_and_blocks_comparison() {
        let mut app = app();
        app.old = Some(book());
        app.new = Some(book());
        app.reset_pairs();
        app.load(0, PathBuf::from("/nonexistent/diff-excel-invalid.xlsx"));
        assert!(app.old.is_none());
        assert!(app.pairs.is_empty());
        app.tx
            .send(Event::Loaded(0, app.generations[0], Err("读取失败".into())))
            .unwrap();
        app.poll();
        assert!(!app.loading[0]);
        assert!(app.file_errors[0].is_some());
        assert!(app.ready_reason().is_some());
    }
    #[test]
    fn cancelled_read_does_not_install_the_finished_workbook() {
        let mut app = app();
        app.loading[0] = true;
        app.load_cancelled[0] = true;
        app.generations[0] = 1;
        app.tx
            .send(Event::Loaded(0, 1, Ok(Box::new((*book()).clone()))))
            .unwrap();
        app.poll();
        assert!(app.old.is_none());
        assert!(!app.loading[0]);
        assert!(app.file_errors[0].as_ref().unwrap().contains("取消"));
        assert!(app.preference_due.is_none());
    }
    #[test]
    fn obsolete_file_and_query_events_cannot_replace_current_state() {
        let mut app = app();
        app.generations[0] = 2;
        app.loading[0] = true;
        app.tx
            .send(Event::Loaded(0, 1, Ok(Box::new((*book()).clone()))))
            .unwrap();
        app.query_epoch = 3;
        app.querying = true;
        app.tx
            .send(Event::Queried(
                2,
                Ok(Page {
                    total: 99,
                    rows: Vec::new(),
                }),
            ))
            .unwrap();
        app.poll();
        assert!(app.old.is_none());
        assert!(app.loading[0]);
        assert_eq!(app.page.total, 0);
        assert!(app.querying);
    }
}
