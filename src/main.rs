mod capture;
mod document;
mod print_screen;
mod record;
mod settings;
use print_screen::PrintScreen;
use settings::{Labels, Language, Preferences, Storage, Theme};

use document::{Document, Point, Tool, Transform};
use gtk::{
    cairo::{Context, FillRule},
    gdk, gio, glib,
    prelude::*,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

struct Editor {
    window: gtk::ApplicationWindow,
    preferences: Cell<Preferences>,
    labels: Labels,
    translating: Cell<bool>,
    settings_button: gtk::MenuButton,
    selected: Cell<Option<usize>>,
    resize_corners: Cell<Option<(Point, Point)>>,
    delete: gtk::Button,
    message: RefCell<(String, String)>,
    tools: gtk::Box,
    area: gtk::DrawingArea,
    empty: gtk::Box,
    status: gtk::Label,
    mode: gtk::DropDown,
    delay: gtk::DropDown,
    doc: RefCell<Option<Document>>,
    /// Captured desktop and its pixels per logical pixel while selecting.
    frozen: RefCell<Option<(Document, f64)>>,
    /// One fullscreen selection window per monitor while selecting.
    overlays: RefCell<Vec<gtk::Window>>,
    /// Index of the overlay that owns the current `drag`.
    active: Cell<usize>,
    tool: Cell<Tool>,
    drag: RefCell<Vec<Point>>,
    busy: Cell<bool>,
    close_allowed: Cell<bool>,
    copy_on_focus: Cell<bool>,
    /// Encoder of the running screen recording.
    recording: RefCell<Option<gio::Subprocess>>,
    record_button: gtk::Button,
    stop_button: gtk::Button,
}

fn add_action(e: &Rc<Editor>, name: &str, run: impl Fn(Rc<Editor>) + 'static) {
    let a = gio::SimpleAction::new(name, None);
    let weak = Rc::downgrade(e);
    a.connect_activate(move |_, _| {
        if let Some(e) = weak.upgrade() {
            run(e)
        }
    });
    e.window.add_action(&a);
}

impl Editor {
    fn build(app: &gtk::Application) -> Rc<Self> {
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Easy SS")
            .default_width(900)
            .default_height(570)
            .build();
        gio::resources_register_include!("icons.gresource").unwrap();
        gtk::IconTheme::for_display(&gtk::prelude::WidgetExt::display(&window))
            .add_resource_path("/io/github/easyss/icons");
        let preferences = Preferences::load();
        let theme_choice = Rc::new(Cell::new(preferences.theme));
        let apply_theme = settings::theme_handler(window.settings(), theme_choice.clone());
        apply_theme();
        let mut labels = Labels::default();
        let header = gtk::HeaderBar::new();
        window.set_titlebar(Some(&header));
        let title = gtk::Label::new(Some("Easy SS"));
        title.add_css_class("heading");
        header.set_title_widget(Some(&title));
        let settings_button = gtk::MenuButton::builder()
            .icon_name("emblem-system-symbolic")
            .build();
        labels.tip(&settings_button, "Pengaturan", "Settings");
        let pop = gtk::Popover::new();
        let grid = gtk::Grid::builder()
            .column_spacing(16)
            .row_spacing(10)
            .margin_top(10)
            .margin_bottom(10)
            .margin_start(10)
            .margin_end(10)
            .build();
        let language = gtk::DropDown::from_strings(&["Bahasa Indonesia", "English"]);
        language.set_selected((preferences.language == Language::English) as u32);
        let theme = labels.dropdown(&["Sistem", "Terang", "Gelap"], &["System", "Light", "Dark"]);
        theme.set_selected(
            Theme::ALL
                .iter()
                .position(|&t| t == preferences.theme)
                .unwrap() as u32,
        );
        let storage = labels.dropdown(
            &["Sementara (/tmp)", "Gambar/Screenshots"],
            &["Temporary (/tmp)", "Pictures/Screenshots"],
        );
        storage.set_selected(
            Storage::ALL
                .iter()
                .position(|&s| s == preferences.storage)
                .unwrap() as u32,
        );
        labels.tip(
            &storage,
            "Berkas di /tmp terhapus saat komputer dimulai ulang. Video permanen disimpan di Video/Screencasts.",
            "Files in /tmp are removed when the computer restarts. Permanent videos go to Videos/Screencasts.",
        );
        for (i, (id, en, control)) in [
            ("Bahasa", "Language", &language),
            ("Tema", "Theme", &theme),
            ("Simpan tangkapan", "Keep captures", &storage),
        ]
        .into_iter()
        .enumerate()
        {
            let label = labels.label(id, en);
            label.set_halign(gtk::Align::Start);
            control
                .update_relation(&[gtk::accessible::Relation::LabelledBy(&[label.upcast_ref()])]);
            grid.attach(&label, 0, i as i32, 1, 1);
            grid.attach(control, 1, i as i32, 1, 1);
        }
        let print_screen = PrintScreen::open().map(Rc::new);
        let print_toggle = gtk::Switch::builder().halign(gtk::Align::End).build();
        print_toggle.set_active(
            print_screen
                .as_ref()
                .is_some_and(|shortcut| shortcut.enabled()),
        );
        let print_label = labels.label("Gunakan Print Screen", "Use Print Screen");
        print_label.set_halign(gtk::Align::Start);
        print_toggle.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
            print_label.upcast_ref()
        ])]);
        grid.attach(&print_label, 0, 3, 1, 1);
        grid.attach(&print_toggle, 1, 3, 1, 1);
        pop.set_child(Some(&grid));
        settings_button.set_popover(Some(&pop));
        header.pack_end(&settings_button);
        for (id, en, icon, action, tip, en_tip) in [
            (
                "Baru",
                "New",
                "camera-photo-symbolic",
                "new",
                "Ambil screenshot (Ctrl+N)",
                "New screenshot (Ctrl+N)",
            ),
            (
                "Simpan",
                "Save",
                "document-save-symbolic",
                "save",
                "Simpan PNG (Ctrl+S)",
                "Save PNG (Ctrl+S)",
            ),
            (
                "Salin",
                "Copy",
                "edit-copy-symbolic",
                "copy",
                "Salin gambar (Ctrl+C)",
                "Copy image (Ctrl+C)",
            ),
        ] {
            let button = gtk::Button::new();
            button.set_child(Some(&labels.button(icon, id, en)));
            labels.tip(&button, tip, en_tip);
            button.set_action_name(Some(&format!("win.{action}")));
            if action == "new" {
                button.add_css_class("suggested-action");
                header.pack_start(&button)
            } else {
                header.pack_end(&button)
            }
        }
        let record_button = gtk::Button::new();
        record_button.set_child(Some(&labels.button(
            "media-record-symbolic",
            "Rekam",
            "Record",
        )));
        labels.tip(
            &record_button,
            "Rekam video layar (Ctrl+R)",
            "Record screen video (Ctrl+R)",
        );
        let stop_button = gtk::Button::new();
        stop_button.set_child(Some(&labels.button(
            "media-playback-stop-symbolic",
            "Berhenti",
            "Stop",
        )));
        labels.tip(
            &stop_button,
            "Hentikan rekaman (Ctrl+R)",
            "Stop recording (Ctrl+R)",
        );
        stop_button.add_css_class("destructive-action");
        for button in [&record_button, &stop_button] {
            button.set_action_name(Some("win.record"));
            header.pack_start(button);
        }
        let mode = labels.dropdown(
            &["Persegi panjang", "Bentuk bebas", "Layar penuh"],
            &["Rectangle", "Freehand", "Full screen"],
        );
        labels.tip(&mode, "Mode tangkap", "Capture mode");
        header.pack_start(&mode);
        let delay = labels.dropdown(
            &["Tanpa jeda", "3 detik", "5 detik", "10 detik"],
            &["No delay", "3 seconds", "5 seconds", "10 seconds"],
        );
        labels.tip(&delay, "Jeda sebelum menangkap", "Capture delay");
        header.pack_start(&delay);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let tools = gtk::Box::new(gtk::Orientation::Horizontal, 5);
        tools.set_halign(gtk::Align::Center);
        tools.set_margin_top(8);
        tools.set_margin_bottom(8);
        let mut buttons = vec![];
        for (id, en, icon, t) in [
            ("Pilih", "Select", "select-symbolic", Tool::Select),
            ("Pena", "Pen", "document-edit-symbolic", Tool::Pen),
            ("Stabilo", "Highlighter", "marker-symbolic", Tool::Highlight),
            ("Penghapus", "Eraser", "eraser-symbolic", Tool::Erase),
            ("Potong", "Crop", "crop-symbolic", Tool::Crop),
        ] {
            let b = gtk::ToggleButton::new();
            b.set_child(Some(&labels.button(icon, id, en)));
            labels.tip(&b, id, en);
            if let Some((first, _)) = buttons.first() {
                b.set_group(Some(first));
            }
            b.set_active(t == Tool::Pen);
            tools.append(&b);
            buttons.push((b, t));
        }
        tools.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        for (icon, name, tip, en) in [
            (
                "edit-undo-symbolic",
                "undo",
                "Urungkan (Ctrl+Z)",
                "Undo (Ctrl+Z)",
            ),
            (
                "edit-redo-symbolic",
                "redo",
                "Ulangi (Ctrl+Shift+Z)",
                "Redo (Ctrl+Shift+Z)",
            ),
        ] {
            let b = gtk::Button::from_icon_name(icon);
            b.set_action_name(Some(&format!("win.{name}")));
            labels.tip(&b, tip, en);
            tools.append(&b);
        }
        let delete = gtk::Button::from_icon_name("edit-delete-symbolic");
        labels.tip(&delete, "Hapus coretan (Delete)", "Delete stroke (Delete)");
        delete.set_action_name(Some("win.delete"));
        tools.append(&delete);
        root.append(&tools);
        let overlay = gtk::Overlay::new();
        overlay.set_vexpand(true);
        let area = gtk::DrawingArea::new();
        area.set_focusable(true);
        area.set_hexpand(true);
        let canvas = area.clone();
        labels.bind(move |l| {
            canvas.update_property(&[gtk::accessible::Property::Label(
                l.text("Kanvas screenshot", "Screenshot canvas"),
            )])
        });
        overlay.set_child(Some(&area));
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 12);
        empty.set_halign(gtk::Align::Center);
        empty.set_valign(gtk::Align::Center);
        let icon = gtk::Image::from_icon_name("camera-photo-symbolic");
        icon.set_pixel_size(44);
        icon.add_css_class("dim-label");
        empty.append(&icon);
        let hint = labels.label(
            "Klik Baru untuk mengambil screenshot atau Rekam untuk merekam video.",
            "Click New to take a screenshot or Record to record a video.",
        );
        empty.append(&hint);
        overlay.add_overlay(&empty);
        root.append(&overlay);
        let status = gtk::Label::new(None);
        status.set_wrap(true);
        status.set_margin_top(7);
        status.set_margin_bottom(7);
        root.append(&status);
        window.set_child(Some(&root));
        let e = Rc::new(Self {
            window,
            preferences: Cell::new(preferences),
            labels,
            settings_button,
            translating: Cell::new(false),
            delete,
            selected: Cell::new(None),
            resize_corners: Cell::new(None),
            message: Default::default(),
            tools,
            area,
            empty,
            status,
            mode,
            delay,
            doc: Default::default(),
            frozen: Default::default(),
            overlays: Default::default(),
            active: Cell::new(0),
            tool: Cell::new(Tool::Pen),
            drag: Default::default(),
            busy: Cell::new(false),
            close_allowed: Cell::new(false),
            copy_on_focus: Cell::new(false),
            recording: Default::default(),
            record_button,
            stop_button,
        });
        e.labels.apply(preferences.language);
        let weak = Rc::downgrade(&e);
        let shortcut = print_screen.clone();
        let handler = print_toggle.connect_state_set(move |toggle, enabled| {
            let Some(e) = weak.upgrade() else {
                return glib::Propagation::Stop;
            };
            let previous = shortcut.as_ref().is_some_and(|shortcut| shortcut.enabled());
            if enabled == previous {
                return glib::Propagation::Proceed;
            }
            let result = shortcut.as_ref().map_or_else(
                || {
                    Err(e
                        .text(
                            "Pengaturan pintasan GNOME tidak tersedia.",
                            "GNOME shortcut settings are unavailable.",
                        )
                        .into())
                },
                |shortcut| shortcut.set_enabled(enabled, e.preferences.get().language),
            );
            if let Err(err) = result {
                let toggle = toggle.clone();
                // GTK updates the accessible state after emitting state-set.
                glib::idle_add_local_once(move || toggle.set_active(previous));
                e.error(
                    "Gagal mengatur Print Screen.",
                    "Could not configure Print Screen.",
                    err,
                );
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        pop.connect_show(move |_| {
            print_toggle.block_signal(&handler);
            print_toggle.set_active(
                print_screen
                    .as_ref()
                    .is_some_and(|shortcut| shortcut.enabled()),
            );
            print_toggle.unblock_signal(&handler);
        });
        let weak = Rc::downgrade(&e);
        language.connect_selected_notify(move |d| {
            if let Some(e) = weak.upgrade() {
                let language = if d.selected() == 1 {
                    Language::English
                } else {
                    Language::Indonesian
                };
                e.preferences.set(Preferences {
                    language,
                    ..e.preferences.get()
                });
                e.translating.set(true);
                e.labels.apply(language);
                e.translating.set(false);
                e.update_status();
                e.save_preferences();
            }
        });
        let weak = Rc::downgrade(&e);
        theme.connect_selected_notify(move |d| {
            if let Some(e) = weak.upgrade()
                && !e.translating.get()
                && let Some(&theme) = Theme::ALL.get(d.selected() as usize)
            {
                e.preferences.set(Preferences {
                    theme,
                    ..e.preferences.get()
                });
                theme_choice.set(theme);
                apply_theme();
                e.save_preferences();
            }
        });
        let weak = Rc::downgrade(&e);
        storage.connect_selected_notify(move |d| {
            if let Some(e) = weak.upgrade()
                && !e.translating.get()
                && let Some(&storage) = Storage::ALL.get(d.selected() as usize)
            {
                e.preferences.set(Preferences {
                    storage,
                    ..e.preferences.get()
                });
                e.save_preferences();
            }
        });
        for (b, t) in buttons {
            let weak = Rc::downgrade(&e);
            b.connect_toggled(move |b| {
                if b.is_active()
                    && let Some(e) = weak.upgrade()
                {
                    e.selected.set(None);
                    e.tool.set(t);
                    e.cancel_drag();
                    e.refresh();
                }
            });
        }
        add_action(&e, "new", |e| e.start(false));
        add_action(&e, "record", |e| e.record());
        add_action(&e, "copy", |e| e.copy());
        add_action(&e, "save", |e| e.save());
        add_action(&e, "delete", |e| {
            if let Some(id) = e.selected.take()
                && let Some(doc) = e.doc.borrow_mut().as_mut()
            {
                doc.remove(id)
            }
            e.cancel_drag();
            e.refresh();
        });
        add_action(&e, "undo", |e| e.history(false));
        add_action(&e, "redo", |e| e.history(true));
        for (action, accel) in [
            ("new", "<Control>n"),
            ("record", "<Control>r"),
            ("delete", "Delete"),
            ("copy", "<Control>c"),
            ("save", "<Control>s"),
            ("undo", "<Control>z"),
            ("redo", "<Control><Shift>z"),
        ] {
            app.set_accels_for_action(&format!("win.{action}"), &[accel]);
        }
        e.attach_canvas();
        let weak = Rc::downgrade(&e);
        e.window.connect_is_active_notify(move |window| {
            if window.is_active()
                && let Some(e) = weak.upgrade()
                && e.copy_on_focus.replace(false)
            {
                e.copy();
            }
        });
        let weak = Rc::downgrade(&e);
        e.window.connect_close_request(move |_| {
            let Some(e) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if e.close_allowed.get() {
                return glib::Propagation::Proceed;
            }
            if e.selecting() {
                e.end_selection();
                return glib::Propagation::Stop;
            }
            if let Some(process) = e.recording.borrow().as_ref() {
                record::Recording::stop(process);
                return glib::Propagation::Stop;
            }
            if e.busy.get() {
                return glib::Propagation::Stop;
            }
            if !e.dirty() {
                return glib::Propagation::Proceed;
            }
            e.set_busy(true);
            glib::spawn_future_local(async move {
                if e.confirm().await {
                    e.close_allowed.set(true);
                    e.window.close();
                } else {
                    e.set_busy(false);
                }
            });
            glib::Propagation::Stop
        });
        e.note("", "");
        e.refresh();
        e
    }
    fn dirty(&self) -> bool {
        self.doc.borrow().as_ref().is_some_and(Document::dirty)
    }
    fn selecting(&self) -> bool {
        !self.overlays.borrow().is_empty()
    }
    fn refresh(&self) {
        let doc = self.doc.borrow();
        let busy = self.busy.get();
        let recording = self.recording.borrow().is_some();
        for (name, enabled) in [
            ("new", true),
            ("record", true),
            ("delete", self.selected.get().is_some()),
            ("save", doc.is_some()),
            ("copy", doc.is_some()),
            ("undo", doc.as_ref().is_some_and(Document::undoable)),
            ("redo", doc.as_ref().is_some_and(Document::redoable)),
        ] {
            self.window
                .lookup_action(name)
                .unwrap()
                .downcast::<gio::SimpleAction>()
                .unwrap()
                .set_enabled(enabled && (!busy || name == "record" && recording));
        }
        self.record_button.set_visible(!recording);
        self.stop_button.set_visible(recording);
        self.delete.set_visible(self.selected.get().is_some());
        self.settings_button.set_sensitive(!busy);
        self.tools.set_visible(doc.is_some());
        self.tools.set_sensitive(!busy);
        self.mode.set_sensitive(!busy);
        self.delay.set_sensitive(!busy);
        self.empty.set_visible(doc.is_none());
        self.area
            .set_cursor_from_name(if self.tool.get() == Tool::Select {
                Some("default")
            } else if doc.is_some() {
                Some("crosshair")
            } else {
                None
            });
        self.area.queue_draw();
    }
    fn text<'a>(&self, id: &'a str, en: &'a str) -> &'a str {
        self.preferences.get().language.text(id, en)
    }
    fn save_preferences(&self) {
        if let Err(err) = self.preferences.get().save() {
            self.error(
                "Gagal menyimpan pengaturan.",
                "Could not save settings.",
                err,
            );
        }
    }
    fn update_status(&self) {
        let message = self.message.borrow();
        let text = self.text(&message.0, &message.1);
        self.status.set_text(text);
        self.status.set_visible(!text.is_empty());
    }
    fn note(&self, id: &str, en: &str) {
        self.message.replace((id.into(), en.into()));
        self.update_status();
    }
    fn error(&self, id: &str, en: &str, error: impl std::fmt::Display) {
        self.note(&format!("{id} {error}"), &format!("{en} {error}"));
    }
    fn set_busy(&self, b: bool) {
        self.busy.set(b);
        self.refresh();
    }
    fn cancel_drag(&self) {
        self.drag.borrow_mut().clear();
        self.area.queue_draw();
    }
    async fn confirm(&self) -> bool {
        if !self.dirty() {
            return true;
        }
        gtk::AlertDialog::builder()
            .message(self.text(
                "Buang perubahan yang belum disimpan?",
                "Discard unsaved changes?",
            ))
            .detail(self.text(
                "Pilih Kembali untuk menyimpan hasil edit.",
                "Choose Back to keep editing and save your changes.",
            ))
            .buttons([
                self.text("Kembali", "Back"),
                self.text("Buang perubahan", "Discard changes"),
            ])
            .cancel_button(0)
            .default_button(0)
            .modal(true)
            .build()
            .choose_future(Some(&self.window))
            .await
            == Ok(1)
    }
    fn start(self: Rc<Self>, rectangle: bool) {
        if self.busy.get() {
            return;
        }
        self.cancel_drag();
        self.set_busy(true);
        if rectangle && self.dirty() {
            self.window.present();
        }
        glib::spawn_future_local(async move {
            if !self.confirm().await {
                self.set_busy(false);
                return;
            }
            let (mode, seconds) = if rectangle {
                (0, 0)
            } else {
                (
                    self.mode.selected(),
                    [0, 3, 5, 10][self.delay.selected() as usize],
                )
            };
            let delay = Duration::from_millis(seconds * 1000 + 180);
            let preferences = self.preferences.get();
            let result = capture::capture(
                &self.window,
                delay,
                preferences.language,
                preferences.storage,
            )
            .await;
            match result {
                Ok(Some(doc)) if mode <= 1 => {
                    // GNOME paints fullscreen windows opaque, so select on the
                    // captured image rather than over the live desktop.
                    self.note("", "");
                    if self.open_overlays(doc, mode == 1) {
                        return;
                    }
                }
                Ok(Some(doc)) => self.accept(doc),
                Ok(None) => self.note("Pengambilan dibatalkan.", "Capture canceled."),
                Err(err) => self.error(
                    "Gagal mengambil gambar. Klik Baru untuk mencoba lagi.",
                    "Capture failed. Click New to try again.",
                    err,
                ),
            }
            self.set_busy(false);
            self.window.present();
        });
    }
    /// Starts a screen recording, or stops the running one.
    fn record(self: Rc<Self>) {
        if let Some(process) = self.recording.borrow().as_ref() {
            record::Recording::stop(process);
            return;
        }
        if self.busy.get() {
            return;
        }
        self.cancel_drag();
        self.set_busy(true);
        glib::spawn_future_local(async move {
            let seconds = [0, 3, 5, 10][self.delay.selected() as usize];
            let delay = Duration::from_millis(seconds * 1000 + 180);
            let preferences = self.preferences.get();
            let result = record::start(
                &self.window,
                delay,
                preferences.language,
                preferences.storage,
            )
            .await;
            match result {
                Ok(Some(recording)) => {
                    self.recording.replace(Some(recording.process.clone()));
                    self.note(
                        "Merekam layar. Klik Berhenti (Ctrl+R) untuk menyelesaikan.",
                        "Recording the screen. Click Stop (Ctrl+R) to finish.",
                    );
                    self.refresh();
                    let result = recording.wait(preferences.language).await;
                    self.recording.take();
                    match result {
                        Ok(path) => {
                            let path = path.display();
                            self.note(
                                &format!("Video disimpan di {path}"),
                                &format!("Video saved to {path}"),
                            );
                        }
                        Err(err) => {
                            self.error("Gagal menyimpan video.", "Could not save the video.", err)
                        }
                    }
                }
                Ok(None) => self.note("Perekaman dibatalkan.", "Recording canceled."),
                Err(err) => self.error(
                    "Gagal memulai perekaman. Klik Rekam untuk mencoba lagi.",
                    "Could not start recording. Click Record to try again.",
                    err,
                ),
            }
            self.set_busy(false);
            self.window.present();
        });
    }
    /// Shows each monitor's part of the capture fullscreen to select on.
    fn open_overlays(self: &Rc<Self>, doc: Document, freehand: bool) -> bool {
        let hint = if freehand {
            self.text(
                "Gambar batas pilihan  ·  Esc untuk batal",
                "Draw a selection outline  ·  Esc to cancel",
            )
        } else {
            self.text(
                "Seret untuk memilih area  ·  Esc untuk batal",
                "Drag to select an area  ·  Esc to cancel",
            )
        };
        let monitors = gtk::prelude::WidgetExt::display(&self.window).monitors();
        let monitors: Vec<_> = (0..monitors.n_items())
            .filter_map(|i| monitors.item(i)?.downcast::<gdk::Monitor>().ok())
            .collect();
        // The portal image spans the bounding box of all monitors at one scale.
        let geometries: Vec<_> = monitors.iter().map(|m| m.geometry()).collect();
        let left = geometries.iter().map(|g| g.x()).min().unwrap_or(0);
        let top = geometries.iter().map(|g| g.y()).min().unwrap_or(0);
        let right = geometries.iter().map(|g| g.x() + g.width()).max();
        let Some(right) = right.filter(|&right| right > left) else {
            return false;
        };
        let scale = doc.rect().w / (right - left) as f64;
        self.frozen.replace(Some((doc, scale)));
        let mut overlays = vec![];
        for (index, (monitor, g)) in monitors.iter().zip(&geometries).enumerate() {
            let offset = ((g.x() - left) as f64, (g.y() - top) as f64);
            let window = gtk::Window::builder()
                .decorated(false)
                .title("Easy SS")
                .build();
            let area = gtk::DrawingArea::new();
            area.set_cursor_from_name(Some("crosshair"));
            let label = gtk::Label::new(Some(hint));
            label.set_halign(gtk::Align::Center);
            label.set_valign(gtk::Align::Start);
            label.set_margin_top(12);
            label.set_can_target(false);
            label.add_css_class("osd");
            let stack = gtk::Overlay::new();
            stack.set_child(Some(&area));
            stack.add_overlay(&label);
            window.set_child(Some(&stack));
            let weak = Rc::downgrade(self);
            area.set_draw_func(move |_, c, w, h| {
                if let Some(e) = weak.upgrade()
                    && let Some((doc, scale)) = e.frozen.borrow().as_ref()
                {
                    let drag = e.drag.borrow();
                    let points: &[Point] = if e.active.get() == index { &drag } else { &[] };
                    let _ = c.save();
                    c.scale(1. / scale, 1. / scale);
                    c.translate(-offset.0 * scale, -offset.1 * scale);
                    let _ = doc.paint(c);
                    let _ = c.restore();
                    c.rectangle(0., 0., w as f64, h as f64);
                    let _ = Self::shade_outside(c, points, freehand, 0.4, 1.5);
                }
            });
            let g = gtk::GestureDrag::new();
            let weak = Rc::downgrade(self);
            g.connect_drag_begin(move |g, x, y| {
                if let Some(e) = weak.upgrade() {
                    e.active.set(index);
                    e.drag.replace(vec![(x, y)]);
                    g.widget().inspect(|w| w.queue_draw());
                }
            });
            let weak = Rc::downgrade(self);
            g.connect_drag_update(move |g, dx, dy| {
                if let Some(e) = weak.upgrade() {
                    e.extend_overlay(g, dx, dy);
                }
            });
            let weak = Rc::downgrade(self);
            g.connect_drag_end(move |g, dx, dy| {
                if let Some(e) = weak.upgrade() {
                    e.extend_overlay(g, dx, dy);
                    let points = e.drag.take();
                    if !points.is_empty() {
                        e.finish_selection(&points, offset, freehand);
                    }
                }
            });
            let weak = Rc::downgrade(self);
            g.connect_cancel(move |g, _| {
                if let Some(e) = weak.upgrade() {
                    e.drag.borrow_mut().clear();
                    g.widget().inspect(|w| w.queue_draw());
                }
            });
            area.add_controller(g);
            let key = gtk::EventControllerKey::new();
            let weak = Rc::downgrade(self);
            key.connect_key_pressed(move |_, k, _, _| {
                if k == gdk::Key::Escape
                    && let Some(e) = weak.upgrade()
                {
                    e.end_selection();
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            window.add_controller(key);
            let weak = Rc::downgrade(self);
            window.connect_close_request(move |_| {
                if let Some(e) = weak.upgrade() {
                    e.end_selection();
                }
                glib::Propagation::Stop
            });
            window.fullscreen_on_monitor(monitor);
            overlays.push(window);
        }
        for window in &overlays {
            window.present();
        }
        self.overlays.replace(overlays);
        true
    }
    fn extend_overlay(&self, g: &gtk::GestureDrag, dx: f64, dy: f64) {
        let (Some((x, y)), Some(area)) = (g.start_point(), g.widget()) else {
            return;
        };
        let p = (
            (x + dx).clamp(0., area.width() as f64),
            (y + dy).clamp(0., area.height() as f64),
        );
        let mut drag = self.drag.borrow_mut();
        if !drag.is_empty() && drag.last() != Some(&p) {
            drag.push(p);
            area.queue_draw();
        }
    }
    fn close_overlays(&self) {
        let overlays = self.overlays.take();
        self.drag.borrow_mut().clear();
        self.frozen.take();
        for window in overlays {
            window.destroy();
        }
    }
    fn end_selection(&self) {
        self.close_overlays();
        self.set_busy(false);
        self.note("", "");
        self.window.present();
    }
    /// Crops the frozen capture to a selection drawn on one monitor overlay.
    fn finish_selection(&self, points: &[Point], offset: Point, freehand: bool) {
        let result = self.frozen.borrow().as_ref().map(|(doc, scale)| {
            let points: Vec<Point> = points
                .iter()
                .map(|&(x, y)| ((x + offset.0) * scale, (y + offset.1) * scale))
                .collect();
            doc.selection(&points, freehand)
        });
        self.end_selection();
        match result {
            Some(Ok(Some(doc))) => self.accept(doc),
            Some(Err(err)) => {
                self.error("Gagal memotong gambar.", "Could not crop the image.", err)
            }
            _ => (),
        }
    }
    fn accept(&self, doc: Document) {
        self.selected.set(None);
        self.doc.replace(Some(doc));
        // Wayland grants clipboard ownership to the focused client.
        if self.window.is_active() {
            self.copy();
        } else {
            self.note("Gambar siap.", "Image ready.");
            self.copy_on_focus.set(true);
        }
    }
    fn copy(&self) {
        if let Some(doc) = self.doc.borrow().as_ref() {
            match doc.pixels(self.preferences.get().language) {
                Ok(p) => {
                    self.window
                        .clipboard()
                        .set_texture(&gdk::Texture::for_pixbuf(&p));
                    self.note("Gambar disalin ke clipboard.", "Image copied to clipboard.");
                }
                Err(e) => self.error(
                    "Gagal menyalin gambar. Klik Salin untuk mencoba lagi.",
                    "Copy failed. Click Copy to try again.",
                    e,
                ),
            }
        }
    }
    fn save(self: Rc<Self>) {
        self.cancel_drag();
        self.set_busy(true);
        glib::spawn_future_local(async move {
            let filter = gtk::FileFilter::new();
            filter.set_name(Some(self.text("Gambar PNG", "PNG image")));
            filter.add_mime_type("image/png");
            let name = glib::DateTime::now_local()
                .and_then(|t| t.format("Screenshot-%Y%m%d-%H%M%S.png"))
                .map(|s| s.to_string())
                .unwrap_or_else(|_| "Screenshot.png".into());
            let dialog = gtk::FileDialog::builder()
                .title(self.text("Simpan PNG", "Save PNG"))
                .initial_name(name)
                .default_filter(&filter)
                .modal(true)
                .build();
            match dialog.save_future(Some(&self.window)).await {
                Ok(file) => {
                    let bytes = self
                        .doc
                        .borrow()
                        .as_ref()
                        .unwrap()
                        .pixels(self.preferences.get().language)
                        .and_then(|p| Ok(p.save_to_bufferv("png", &[])?));
                    let result = match bytes {
                        Ok(b) => file
                            .replace_contents_future(b, None, false, gio::FileCreateFlags::NONE)
                            .await
                            .map(|_| ())
                            .map_err(|(_, e)| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    };
                    match result {
                        Ok(()) => {
                            self.doc.borrow_mut().as_mut().unwrap().mark_saved();
                            self.note("PNG tersimpan.", "PNG saved.");
                        }
                        Err(e) => self.error(
                            "Gagal menyimpan PNG. Pilih Simpan untuk mencoba lagi.",
                            "Save failed. Click Save to try again.",
                            e,
                        ),
                    }
                }
                Err(e)
                    if e.matches(gtk::DialogError::Dismissed)
                        || e.matches(gtk::DialogError::Cancelled) => {}
                Err(e) => self.error(
                    "Gagal membuka dialog simpan.",
                    "Could not open the save dialog.",
                    e,
                ),
            }
            self.set_busy(false);
        });
    }
    fn history(&self, redo: bool) {
        self.selected.set(None);
        self.cancel_drag();
        if let Some(doc) = self.doc.borrow_mut().as_mut() {
            if redo { doc.redo() } else { doc.undo() }
        }
        self.note("", "");
        self.refresh();
    }
    fn with_canvas<T>(&self, f: impl FnOnce(&Document) -> T) -> Option<T> {
        self.doc.borrow().as_ref().map(f)
    }
    fn position(&self, x: f64, y: f64, clamp: bool) -> Option<Point> {
        self.with_canvas(|doc| {
            doc.rect()
                .fit(self.area.width(), self.area.height())
                .point(x, y, clamp)
        })
        .flatten()
    }
    fn transform(&self, points: &[Point]) -> Transform {
        match (points.first(), points.last()) {
            (Some(&start), Some(&end)) if start != end => {
                if let Some((corner, anchor)) = self.resize_corners.get() {
                    let target = (corner.0 + (end.0 - start.0), corner.1 + (end.1 - start.1));
                    Transform::gesture(corner, target, Some(anchor))
                } else {
                    Transform::gesture(start, end, None)
                }
            }
            _ => Transform::IDENTITY,
        }
    }
    fn handles(corners: [Point; 4], view: &document::View) -> [Point; 4] {
        let pad = 6. / view.scale;
        let r = view.rect;
        let offsets = [(-pad, -pad), (pad, -pad), (pad, pad), (-pad, pad)];
        std::array::from_fn(|i| {
            (
                (corners[i].0 + offsets[i].0).clamp(r.x, r.x + r.w),
                (corners[i].1 + offsets[i].1).clamp(r.y, r.y + r.h),
            )
        })
    }
    fn pick(&self, p: Point) {
        let doc = self.doc.borrow();
        let Some(doc) = doc.as_ref() else { return };
        let view = doc.rect().fit(self.area.width(), self.area.height());
        let scale = view.scale;
        self.resize_corners.set(None);
        if let Some(corners) = self.selected.get().and_then(|id| doc.corners(id)) {
            for (i, (x, y)) in Self::handles(corners, &view).into_iter().enumerate() {
                if (p.0 - x).abs().max((p.1 - y).abs()) <= 5. / scale {
                    self.resize_corners
                        .set(Some((corners[i], corners[(i + 2) % 4])));
                    return;
                }
            }
        }
        self.selected.set(doc.hit(p, 3. / scale));
    }
    fn draw(&self, c: &Context, w: i32, h: i32) -> Result<(), gtk::cairo::Error> {
        self.with_canvas(|doc| {
            c.set_source_rgb(0.14, 0.15, 0.16);
            c.paint()?;
            let view = doc.rect().fit(w, h);
            view.apply(c);
            let points = self.drag.borrow();
            let transform = self.transform(&points);
            let selected = self
                .selected
                .get()
                .filter(|_| self.tool.get() == Tool::Select);
            doc.paint_preview(c, selected.map(|id| (id, transform)))?;
            if let Some(corners) = selected.and_then(|id| doc.corners(id)) {
                let h = Self::handles(corners.map(|p| transform.point(p)), &view);
                let unit = 1. / view.scale;
                c.rectangle(h[0].0, h[0].1, h[2].0 - h[0].0, h[2].1 - h[0].1);
                c.set_line_width(3. * unit);
                c.set_source_rgba(1., 1., 1., 0.9);
                c.stroke_preserve()?;
                c.set_source_rgb(0.1, 0.31, 0.6);
                c.set_line_width(unit);
                c.stroke()?;
                for (x, y) in h {
                    c.rectangle(x - 3. * unit, y - 3. * unit, 6. * unit, 6. * unit);
                    c.set_source_rgb(1., 1., 1.);
                    c.fill_preserve()?;
                    c.set_source_rgb(0.1, 0.31, 0.6);
                    c.stroke()?;
                }
            }
            if self.tool.get() == Tool::Crop && !points.is_empty() {
                let r = doc.rect();
                c.rectangle(r.x, r.y, r.w, r.h);
                Self::shade_outside(c, &points, false, 0.48, 1.5 / view.scale)?;
            } else if !points.is_empty() && matches!(self.tool.get(), Tool::Pen | Tool::Highlight) {
                document::stroke(c, self.tool.get(), &points)?;
            }
            Ok(())
        })
        .unwrap_or(Ok(()))
    }
    /// Dims the current path outside the selection and strokes its outline.
    fn shade_outside(
        c: &Context,
        points: &[Point],
        freehand: bool,
        alpha: f64,
        line_width: f64,
    ) -> Result<(), gtk::cairo::Error> {
        let outline = || {
            if freehand {
                document::path(c, points);
                c.close_path();
            } else if let (Some(a), Some(b)) = (points.first(), points.last()) {
                c.rectangle(a.0, a.1, b.0 - a.0, b.1 - a.1);
            }
        };
        outline();
        c.set_fill_rule(FillRule::EvenOdd);
        c.set_source_rgba(0., 0., 0., alpha);
        c.fill()?;
        outline();
        c.set_source_rgb(1., 1., 1.);
        c.set_line_width(line_width);
        c.stroke()
    }
    fn attach_canvas(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.area.set_draw_func(move |_, c, w, h| {
            if let Some(e) = weak.upgrade()
                && let Err(err) = e.draw(c, w, h)
            {
                e.error(
                    "Gagal menampilkan gambar.",
                    "Could not display the image.",
                    err,
                );
            }
        });
        let g = gtk::GestureDrag::new();
        let weak = Rc::downgrade(self);
        g.connect_drag_begin(move |_, x, y| {
            if let Some(e) = weak.upgrade()
                && !e.busy.get()
            {
                let Some(p) = e.position(x, y, false) else {
                    if e.tool.get() == Tool::Select {
                        e.selected.set(None);
                        e.cancel_drag();
                        e.refresh();
                    }
                    return;
                };
                if e.tool.get() == Tool::Select {
                    e.pick(p);
                    e.refresh();
                }
                e.drag.replace(vec![p]);
                e.area.grab_focus();
                e.area.queue_draw();
            }
        });
        let weak = Rc::downgrade(self);
        g.connect_drag_update(move |g, dx, dy| {
            if let Some(e) = weak.upgrade()
                && let Some((x, y)) = g.start_point()
            {
                e.extend(x + dx, y + dy);
            }
        });
        let weak = Rc::downgrade(self);
        g.connect_drag_end(move |g, dx, dy| {
            if let Some(e) = weak.upgrade() {
                if let Some((x, y)) = g.start_point() {
                    e.extend(x + dx, y + dy);
                }
                let points = e.drag.take();
                if points.is_empty() {
                    return;
                }
                if let Some(doc) = e.doc.borrow_mut().as_mut() {
                    if e.tool.get() == Tool::Select {
                        if let Some(id) = e.selected.get() {
                            doc.transform(id, e.transform(&points))
                        }
                    } else {
                        doc.edit(e.tool.get(), points)
                    }
                    e.note("", "");
                }
                e.refresh();
            }
        });
        let weak = Rc::downgrade(self);
        g.connect_cancel(move |_, _| {
            if let Some(e) = weak.upgrade() {
                e.cancel_drag()
            }
        });
        self.area.add_controller(g);
        let key = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(self);
        key.connect_key_pressed(move |_, k, _, _| {
            if k == gdk::Key::Escape
                && let Some(e) = weak.upgrade()
            {
                e.selected.set(None);
                e.cancel_drag();
                e.refresh();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.window.add_controller(key);
    }
    fn extend(&self, x: f64, y: f64) {
        let mut drag = self.drag.borrow_mut();
        if !drag.is_empty()
            && let Some(p) = self.position(x, y, true)
            && drag.last() != Some(&p)
        {
            drag.push(p);
            self.area.queue_draw();
        }
    }
}

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id("io.github.easyss.EasySS")
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let language = Preferences::load().language;
    app.add_main_option(
        "capture",
        glib::Char::from(0),
        glib::OptionFlags::NONE,
        glib::OptionArg::None,
        language.text("Langsung ambil screenshot", "Take a screenshot immediately"),
        None,
    );
    app.add_main_option(
        print_screen::CAPTURE_OPTION,
        glib::Char::from(0),
        glib::OptionFlags::NONE,
        glib::OptionArg::None,
        language.text(
            "Langsung pilih area persegi panjang",
            "Select a rectangular area immediately",
        ),
        None,
    );
    let editor = RefCell::new(None::<Rc<Editor>>);
    app.connect_command_line(move |app, cmd| {
        let mut state = editor.borrow_mut();
        let e = state.get_or_insert_with(|| Editor::build(app));
        let options = cmd.options_dict();
        if options.contains(print_screen::CAPTURE_OPTION) {
            e.clone().start(true);
        } else if options.contains("capture") {
            e.clone().start(false);
        } else if !e.busy.get() {
            e.window.present();
        }
        glib::ExitCode::SUCCESS
    });
    app.run()
}
