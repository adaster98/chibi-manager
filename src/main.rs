use gtk::prelude::*;
use gtk::{
    glib,
    Application, ApplicationWindow, Button, CheckButton, ToggleButton, CssProvider,
    FileDialog, Label, ListBox, ListBoxRow, Orientation, Picture,
    ScrolledWindow, SpinButton, STYLE_PROVIDER_PRIORITY_APPLICATION,
    EventControllerMotion, GestureClick, Entry,
    Box as GtkBox
};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use ksni::{Tray, MenuItem, menu::{StandardItem, CheckmarkItem}, ToolTip};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::{channel, Sender};
use std::time::Duration;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use std::fs;
use uuid::Uuid;

// --- DATA STRUCTURES ---

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ChibiPreset {
    id: String,
    name: String,
    path: PathBuf,
    width: i32,
    x: i32,
    y: i32,
    smart_hide: bool,
    always_on_top: bool,
}

struct ActiveWindowRef {
    preset_id: Option<String>,
    window: glib::WeakRef<gtk::Window>,
    list_row: glib::WeakRef<ListBoxRow>,
}

enum AppMsg {
    Quit,
    ToggleManager,
    ToggleHideAll,
    RefreshPresets,
}

// --- TRAY HANDLER ---

struct ChibiTray {
    sender: Sender<AppMsg>,
    is_hidden: bool,
}

impl Tray for ChibiTray {
    fn id(&self) -> String { "chibi-manager".into() }
    fn category(&self) -> ksni::Category { ksni::Category::ApplicationStatus }
    fn title(&self) -> String { "Chibi Manager".into() }
    fn status(&self) -> ksni::Status { ksni::Status::Active }
    fn icon_name(&self) -> String { "face-smile".into() }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "Chibi Manager".into(),
            description: "Right-click for options".into(),
            icon_name: "face-smile".into(),
            icon_pixmap: Vec::new(),
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: "Open Manager".into(),
                activate: Box::new(move |this: &mut Self| {
                    let _ = this.sender.send(AppMsg::ToggleManager);
                }),
                ..Default::default()
            }.into(),
            CheckmarkItem {
                label: "Hide All Chibis".into(),
                checked: self.is_hidden,
                activate: Box::new(move |this: &mut Self| {
                    this.is_hidden = !this.is_hidden;
                    let _ = this.sender.send(AppMsg::ToggleHideAll);
                }),
                ..Default::default()
            }.into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(move |this: &mut Self| {
                    let _ = this.sender.send(AppMsg::Quit);
                }),
                ..Default::default()
            }.into(),
        ]
    }
}

fn main() {
    let app = Application::builder()
    .application_id("com.example.chibimanager.final_merged")
    .build();

    app.connect_startup(|_| {
        let provider = CssProvider::new();
        provider.load_from_data(".ghost-window { background-color: rgba(0,0,0,0.001); }");
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().expect("Could not connect to a display."),
                                                    &provider,
                                                    STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });

    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let windows = app.windows();
    if let Some(existing_window) = windows.into_iter().find(|w| w.title().as_deref() == Some("Chibi Manager")) {
        existing_window.present();
        return;
    }

    // --- STATE MANAGEMENT ---
    let active_registry: Rc<RefCell<Vec<ActiveWindowRef>>> = Rc::new(RefCell::new(Vec::new()));
    let global_hide_state = Rc::new(Cell::new(false));
    let presets: Rc<RefCell<Vec<ChibiPreset>>> = Rc::new(RefCell::new(load_presets()));

    let (sender, receiver) = channel();
    let tray_sender = sender.clone();

    std::thread::spawn(move || {
        let service = ksni::TrayService::new(ChibiTray {
            sender: tray_sender,
            is_hidden: false,
        });
        let _handle = service.spawn();
        std::thread::park();
    });

    let window = ApplicationWindow::builder()
    .application(app)
    .title("Chibi Manager")
    .default_width(600)
    .default_height(450)
    .build();

    window.connect_close_request(move |win| {
        win.set_visible(false);
        glib::Propagation::Stop
    });

    // --- UI LAYOUT ---
    let main_layout = GtkBox::new(Orientation::Horizontal, 10);
    main_layout.set_margin_top(10);
    main_layout.set_margin_bottom(10);
    main_layout.set_margin_start(10);
    main_layout.set_margin_end(10);

    // Left Column (Controls)
    let controls_vbox = GtkBox::new(Orientation::Vertical, 10);
    controls_vbox.set_width_request(250);

    let file_label = Label::new(Some("No image selected"));
    file_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    let file_btn = Button::with_label("📂 Select Image");
    let selected_path: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));

    let path_c = selected_path.clone();
    let label_c = file_label.clone();
    let win_c = window.clone();
    file_btn.connect_clicked(move |_| {
        let dialog = FileDialog::builder().title("Select Image").modal(true).build();
        let p = path_c.clone();
        let l = label_c.clone();
        dialog.open(Some(&win_c), None::<&gtk::gio::Cancellable>, move |res| {
            if let Ok(file) = res {
                if let Some(path) = file.path() {
                    *p.borrow_mut() = Some(path.clone());
                    l.set_text(path.file_name().unwrap().to_str().unwrap());
                }
            }
        });
    });

    controls_vbox.append(&file_btn);
    controls_vbox.append(&file_label);
    controls_vbox.append(&gtk::Separator::new(Orientation::Horizontal));

    controls_vbox.append(&Label::new(Some("Size (px):")));
    let spin_size = SpinButton::with_range(50.0, 1000.0, 10.0);
    spin_size.set_value(200.0);
    controls_vbox.append(&spin_size);

    controls_vbox.append(&Label::new(Some("Spawn X:")));
    let spin_x = SpinButton::with_range(0.0, 5000.0, 50.0);
    spin_x.set_value(100.0);
    controls_vbox.append(&spin_x);

    controls_vbox.append(&Label::new(Some("Spawn Y:")));
    let spin_y = SpinButton::with_range(0.0, 3000.0, 50.0);
    spin_y.set_value(100.0);
    controls_vbox.append(&spin_y);

    let check_hide = CheckButton::with_label("Smart Hide");
    let check_top = CheckButton::with_label("Always on Top");
    controls_vbox.append(&check_hide);
    controls_vbox.append(&check_top);

    let spawn_btn = Button::with_label("✨ SPAWN ✨");
    spawn_btn.add_css_class("suggested-action");
    spawn_btn.set_margin_top(10);
    controls_vbox.append(&spawn_btn);

    controls_vbox.append(&gtk::Separator::new(Orientation::Horizontal));
    let quit_btn = Button::with_label("Quit Application");
    quit_btn.add_css_class("destructive-action");
    let app_quit_btn = app.clone();
    quit_btn.connect_clicked(move |_| {
        app_quit_btn.quit();
    });
    controls_vbox.append(&quit_btn);

    main_layout.append(&controls_vbox);
    main_layout.append(&gtk::Separator::new(Orientation::Vertical));

    // Right Column (Lists)
    let right_vbox = GtkBox::new(Orientation::Vertical, 10);
    right_vbox.set_hexpand(true);

    right_vbox.append(&Label::new(Some("Active Session")));
    let active_scrolled = ScrolledWindow::builder().min_content_height(150).vexpand(true).build();
    let active_list = ListBox::new();
    active_list.add_css_class("frame");
    active_scrolled.set_child(Some(&active_list));
    right_vbox.append(&active_scrolled);

    right_vbox.append(&Label::new(Some("Saved Presets")));
    let preset_scrolled = ScrolledWindow::builder().min_content_height(150).vexpand(true).build();
    let preset_list = ListBox::new();
    preset_list.add_css_class("frame");
    preset_scrolled.set_child(Some(&preset_list));
    right_vbox.append(&preset_scrolled);

    main_layout.append(&right_vbox);
    window.set_child(Some(&main_layout));

    // --- SHARED REFERENCES ---
    let app_clone = app.clone();
    let active_list_ref = active_list.clone();
    let active_reg_ref = active_registry.clone();
    let presets_data_ref = presets.clone();
    let preset_list_ref = preset_list.clone();
    let parent_win_ref = window.clone();
    let sender_for_spawn = sender.clone();

    // --- ACTIVE ITEM LOGIC ---
    let add_to_active_ui = Rc::new(move |data: ChibiPreset, is_new_arg: bool| {
        let (win, move_ctrl, cur_x, cur_y) = spawn_chibi_window(&app_clone, &data);

        let row = ListBoxRow::new();
        let box_layout = GtkBox::new(Orientation::Horizontal, 5);

        // State tracking
        let is_new_state = Rc::new(Cell::new(is_new_arg));
        let current_id = Rc::new(RefCell::new(data.id.clone()));
        let current_name = Rc::new(RefCell::new(data.name.clone()));

        let display_name = if is_new_arg {
            data.path.file_name().unwrap_or_default().to_string_lossy().to_string()
        } else {
            data.name.clone()
        };

        let name_lbl = Label::new(Some(&display_name));
        name_lbl.set_hexpand(true);
        name_lbl.set_xalign(0.0);
        name_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);

        // Move Toggle Button
        let move_btn = ToggleButton::with_label("✋");
        move_btn.set_tooltip_text(Some("Enable Dragging"));
        let mc = move_ctrl.clone();
        let wt = win.clone();
        move_btn.connect_toggled(move |btn| {
            let a = btn.is_active();
            mc.set(a);
            if a {
                btn.set_label("✊");
                wt.set_visible(true);
                wt.set_can_target(true);
                wt.set_opacity(1.0);
            } else {
                btn.set_label("✋");
                wt.set_can_target(true);
            }
        });

        // Save/Update Button
        let save_btn = Button::with_label("💾");
        let p_store = presets_data_ref.clone();
        let win_parent_dialog = parent_win_ref.clone();
        let data_clone = data.clone();
        let cx = cur_x.clone();
        let cy = cur_y.clone();
        let active_reg_for_save = active_reg_ref.clone();
        let name_label_upd = name_lbl.clone();
        let win_weak_for_save = win.downgrade();
        let id_for_save = current_id.clone();
        let name_for_save = current_name.clone();
        let sender_refresh = sender_for_spawn.clone();

        save_btn.connect_clicked(move |_| {
            let mut final_data = data_clone.clone();
            final_data.x = cx.get() as i32;
            final_data.y = cy.get() as i32;
            final_data.id = id_for_save.borrow().clone();

            if !is_new_state.get() {
                final_data.name = name_for_save.borrow().clone();
            }

            if !is_new_state.get() {
                // Update Existing
                let mut vec = p_store.borrow_mut();
                if let Some(existing) = vec.iter_mut().find(|p| p.id == final_data.id) {
                    *existing = final_data.clone();
                }
                save_presets(&vec);
                let _ = sender_refresh.send(AppMsg::RefreshPresets);
            } else {
                // Save As New
                let dialog = gtk::Window::builder()
                .title("Save Preset")
                .transient_for(&win_parent_dialog)
                .modal(true)
                .default_width(300)
                .build();

                let vb = GtkBox::new(Orientation::Vertical, 10);
                vb.set_margin_top(10); vb.set_margin_bottom(10);
                vb.set_margin_start(10); vb.set_margin_end(10);

                let entry = Entry::new();
                entry.set_placeholder_text(Some("Preset Name..."));
                let hb = GtkBox::new(Orientation::Horizontal, 10);
                let b_cancel = Button::with_label("Cancel");
                let b_save = Button::with_label("Save");
                hb.append(&b_cancel); hb.append(&b_save);
                vb.append(&Label::new(Some("Name:")));
                vb.append(&entry);
                vb.append(&hb);
                dialog.set_child(Some(&vb));

                let d_c = dialog.clone();
                b_cancel.connect_clicked(move |_| d_c.close());

                let p_s = p_store.clone();
                let d_ok = dialog.clone();
                let reg_upd = active_reg_for_save.clone();
                let new_state_setter = is_new_state.clone();
                let id_setter = id_for_save.clone();
                let name_setter = name_for_save.clone();
                let lbl_setter = name_label_upd.clone();
                let w_for_lookup = win_weak_for_save.clone();
                let data_for_save = final_data.clone();
                let sender_ref_inner = sender_refresh.clone();

                b_save.connect_clicked(move |_| {
                    let txt = entry.text().to_string();
                    if !txt.is_empty() {
                        let mut new_preset = data_for_save.clone();
                        new_preset.id = Uuid::new_v4().to_string();
                        new_preset.name = txt.clone();

                        p_s.borrow_mut().push(new_preset.clone());
                        save_presets(&p_s.borrow());

                        new_state_setter.set(false);
                        *id_setter.borrow_mut() = new_preset.id.clone();
                        *name_setter.borrow_mut() = txt.clone();
                        lbl_setter.set_text(&txt);

                        let mut reg = reg_upd.borrow_mut();
                        for entry in reg.iter_mut() {
                            if let (Some(a), Some(b)) = (entry.window.upgrade(), w_for_lookup.upgrade()) {
                                if a == b {
                                    entry.preset_id = Some(new_preset.id.clone());
                                    break;
                                }
                            }
                        }
                        let _ = sender_ref_inner.send(AppMsg::RefreshPresets);
                    }
                    d_ok.close();
                });
                dialog.present();
            }
        });

        // Close Button
        let close_btn = Button::with_label("❌");
        let w_close = win.clone();
        let r_close = row.downgrade();
        let l_close = active_list_ref.downgrade();
        let reg_close = active_reg_ref.clone();
        let id_ref_for_close = current_id.clone();

        close_btn.connect_clicked(move |_| {
            w_close.close();
            if let (Some(l), Some(r)) = (l_close.upgrade(), r_close.upgrade()) {
                l.remove(&r);
            }
            let current_pid = id_ref_for_close.borrow();
            let mut reg = reg_close.borrow_mut();
            if let Some(idx) = reg.iter().position(|x| x.preset_id.as_ref() == Some(&*current_pid)) {
                reg.remove(idx);
            }
        });

        box_layout.append(&name_lbl);
        box_layout.append(&move_btn);
        box_layout.append(&save_btn);
        box_layout.append(&close_btn);
        row.set_child(Some(&box_layout));
        active_list_ref.append(&row);

        active_reg_ref.borrow_mut().push(ActiveWindowRef {
            preset_id: Some(data.id.clone()),
                                         window: win.downgrade(),
                                         list_row: row.downgrade(),
        });
    });

    // --- MAIN MESSAGE LOOP (MERGED) ---
    let app_quit = app.clone();
    let win_recv = window.clone();
    let hide_state_recv = global_hide_state.clone();
    let registry_recv = active_registry.clone();
    let presets_refresh = presets.clone();
    let list_refresh = preset_list_ref.clone();
    let spawner_for_refresh = add_to_active_ui.clone();
    let sender_for_refresh = sender.clone();
    let active_reg_for_delete = active_registry.clone();
    let active_list_for_delete = active_list.clone();

    // Trigger initial load
    let _ = sender.send(AppMsg::RefreshPresets);

    glib::timeout_add_local(Duration::from_millis(100), move || {
        while let Ok(msg) = receiver.try_recv() {
            match msg {
                AppMsg::ToggleManager => {
                    win_recv.set_visible(true);
                    win_recv.present();
                }
                AppMsg::ToggleHideAll => {
                    let new_state = !hide_state_recv.get();
                    hide_state_recv.set(new_state);
                    let mut reg = registry_recv.borrow_mut();
                    reg.retain(|r| {
                        if let Some(w) = r.window.upgrade() {
                            w.set_visible(!new_state);
                            true
                        } else {
                            false
                        }
                    });
                }
                AppMsg::RefreshPresets => {
                    while let Some(child) = list_refresh.first_child() {
                        list_refresh.remove(&child);
                    }

                    let mut data_vec = presets_refresh.borrow_mut();

                    for preset in data_vec.iter_mut() {
                        let row = ListBoxRow::new();
                        let box_layout = GtkBox::new(Orientation::Horizontal, 10);

                        let label = Label::new(Some(&preset.name));
                        label.set_hexpand(true);
                        label.set_xalign(0.0);

                        let play_btn = Button::with_label("Spawn");
                        let spawner = spawner_for_refresh.clone();
                        let p_clone = preset.clone();
                        play_btn.connect_clicked(move |_| {
                            spawner(p_clone.clone(), false);
                        });

                        let del_btn = Button::with_label("🗑️");
                        let p_store = presets_refresh.clone();
                        let pid_target = preset.id.clone();
                        let reg_target = active_reg_for_delete.clone();
                        let al_target = active_list_for_delete.clone();
                        let sender_ref = sender_for_refresh.clone();

                        del_btn.connect_clicked(move |_| {
                            // Find and close associated active windows
                            let mut reg = reg_target.borrow_mut();
                            let mut indices = Vec::new();
                            for (i, entry) in reg.iter().enumerate() {
                                if entry.preset_id.as_ref() == Some(&pid_target) {
                                    if let Some(w) = entry.window.upgrade() { w.close(); }
                                    if let Some(r) = entry.list_row.upgrade() { al_target.remove(&r); }
                                    indices.push(i);
                                }
                            }
                            for i in indices.iter().rev() { reg.remove(*i); }

                            // Remove from storage by ID
                            let mut vec = p_store.borrow_mut();
                            if let Some(pos) = vec.iter().position(|p| p.id == pid_target) {
                                vec.remove(pos);
                                save_presets(&vec);
                            }
                            let _ = sender_ref.send(AppMsg::RefreshPresets);
                        });

                        box_layout.append(&label);
                        box_layout.append(&play_btn);
                        box_layout.append(&del_btn);
                        row.set_child(Some(&box_layout));
                        list_refresh.append(&row);
                    }
                }
                AppMsg::Quit => app_quit.quit(),
            }
        }
        glib::ControlFlow::Continue
    });

    // --- SPAWN NEW BUTTON ---
    let spawner_new = add_to_active_ui.clone();
    spawn_btn.connect_clicked(move |_| {
        let path_borrow = selected_path.borrow();
        if let Some(path) = &*path_borrow {
            let data = ChibiPreset {
                id: Uuid::new_v4().to_string(),
                              name: "New Chibi".into(),
                              path: path.clone(),
                              width: spin_size.value() as i32,
                              x: spin_x.value() as i32,
                              y: spin_y.value() as i32,
                              smart_hide: check_hide.is_active(),
                              always_on_top: check_top.is_active(),
            };
            spawner_new(data, true);
        }
    });

    window.present();
}

// --- WINDOW SPAWNER ---
fn spawn_chibi_window(app: &Application, data: &ChibiPreset)
-> (gtk::Window, Rc<Cell<bool>>, Rc<Cell<f64>>, Rc<Cell<f64>>)
{
    let window = gtk::Window::builder()
    .application(app)
    .default_width(data.width)
    .default_height(data.width)
    .decorated(false)
    .build();

    window.add_css_class("ghost-window");
    window.init_layer_shell();
    window.set_layer(if data.always_on_top { Layer::Overlay } else { Layer::Bottom });
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Left, true);
    window.set_margin(Edge::Left, data.x);
    window.set_margin(Edge::Top, data.y);

    let container = GtkBox::new(Orientation::Vertical, 0);
    let picture = Picture::for_filename(&data.path);
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_vexpand(true);
    picture.set_hexpand(true);
    picture.set_can_target(true);
    container.append(&picture);
    window.set_child(Some(&container));

    let click = GestureClick::new();
    let motion = EventControllerMotion::new();

    let current_x = Rc::new(Cell::new(data.x as f64));
    let current_y = Rc::new(Cell::new(data.y as f64));
    let anchor_x = Rc::new(Cell::new(0.0));
    let anchor_y = Rc::new(Cell::new(0.0));
    let is_dragging = Rc::new(Cell::new(false));
    let move_mode = Rc::new(Cell::new(false));

    let drag_c = is_dragging.clone();
    let move_c = move_mode.clone();
    let ax = anchor_x.clone();
    let ay = anchor_y.clone();

    click.connect_pressed(move |_, _, x, y| {
        if move_c.get() {
            drag_c.set(true);
            ax.set(x);
            ay.set(y);
        }
    });

    let drag_rel = is_dragging.clone();
    click.connect_released(move |_, _, _, _| drag_rel.set(false));

    let drag_mot = is_dragging.clone();
    let win_weak = window.downgrade();
    let cx = current_x.clone();
    let cy = current_y.clone();
    let ax_m = anchor_x.clone();
    let ay_m = anchor_y.clone();

    motion.connect_motion(move |_, x, y| {
        if !drag_mot.get() { return; }
        if let Some(w) = win_weak.upgrade() {
            let dx = x - ax_m.get();
            let dy = y - ay_m.get();
            let nx = cx.get() + dx;
            let ny = cy.get() + dy;
            w.set_margin(Edge::Left, nx as i32);
            w.set_margin(Edge::Top, ny as i32);
            cx.set(nx);
            cy.set(ny);
        }
    });

    window.add_controller(click);
    window.add_controller(motion);

    if data.smart_hide {
        let hide_ctrl = EventControllerMotion::new();
        let w_weak = window.downgrade();
        let drag_chk = is_dragging.clone();
        let move_chk = move_mode.clone();
        hide_ctrl.connect_enter(move |_, _, _| {
            if move_chk.get() || drag_chk.get() { return; }
            if let Some(w) = w_weak.upgrade() {
                w.set_visible(false);
                let w_tmr = w.downgrade();
                let move_tmr = move_chk.clone();
                glib::timeout_add_seconds_local(3, move || {
                    if let Some(ww) = w_tmr.upgrade() {
                        if !move_tmr.get() { ww.set_visible(true); }
                    }
                    glib::ControlFlow::Break
                });
            }
        });
        window.add_controller(hide_ctrl);
    }

    window.present();
    (window, move_mode, current_x, current_y)
}

// --- PERSISTENCE ---
fn get_config_path() -> PathBuf {
    if let Some(proj_dirs) = directories::ProjectDirs::from("com", "example", "chibimanager") {
        let config_dir = proj_dirs.config_dir();
        if !config_dir.exists() {
            let _ = fs::create_dir_all(config_dir);
        }
        return config_dir.join("presets.json");
    }
    PathBuf::from("presets.json")
}

fn save_presets(presets: &Vec<ChibiPreset>) {
    let path = get_config_path();
    if let Ok(json) = serde_json::to_string_pretty(presets) {
        let _ = fs::write(path, json);
    }
}

fn load_presets() -> Vec<ChibiPreset> {
    let path = get_config_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(data) = serde_json::from_str(&content) {
                return data;
            }
        }
    }
    Vec::new()
}
