use gtk::prelude::*;
use gtk::{
    glib,
    Application, ApplicationWindow, Box, Button, ToggleButton, CheckButton, CssProvider,
    FileDialog, Label, ListBox, ListBoxRow, Orientation, Picture,
    ScrolledWindow, SpinButton, STYLE_PROVIDER_PRIORITY_APPLICATION,
    EventControllerMotion, GestureClick
};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn main() {
    let app = Application::builder()
    .application_id("com.example.chibimanager.final_release")
    .build();

    app.connect_startup(|_| {
        let provider = CssProvider::new();
        // Define a "ghost-window" class that makes the background almost invisible (0.1% opacity)
        // This trick ensures the window is transparent but still capable of receiving mouse events.
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
    let window = ApplicationWindow::builder()
    .application(app)
    .title("Chibi Manager")
    .default_width(700)
    .default_height(450)
    .build();

    let main_layout = Box::new(Orientation::Horizontal, 10);
    main_layout.set_margin_top(10);
    main_layout.set_margin_bottom(10);
    main_layout.set_margin_start(10);
    main_layout.set_margin_end(10);

    // --- LEFT COLUMN: CONTROLS ---
    let controls_vbox = Box::new(Orientation::Vertical, 10);
    controls_vbox.set_width_request(250);

    let file_label = Label::new(Some("No image selected"));
    file_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);

    let file_btn = Button::with_label("📂 Select Image");
    let selected_path: Rc<RefCell<Option<std::path::PathBuf>>> = Rc::new(RefCell::new(None));

    let path_clone = selected_path.clone();
    let label_clone = file_label.clone();
    let window_clone = window.clone();

    file_btn.connect_clicked(move |_| {
        let dialog = FileDialog::builder()
        .title("Select Image")
        .modal(true)
        .build();

        let path_clone = path_clone.clone();
        let label_clone = label_clone.clone();

        dialog.open(Some(&window_clone), None::<&gtk::gio::Cancellable>, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    *path_clone.borrow_mut() = Some(path.clone());
                    label_clone.set_text(path.file_name().unwrap().to_str().unwrap());
                }
            }
        });
    });

    controls_vbox.append(&file_btn);
    controls_vbox.append(&file_label);
    controls_vbox.append(&gtk::Separator::new(Orientation::Horizontal));

    // --- INPUTS ---
    controls_vbox.append(&Label::new(Some("Size (px):")));
    let spin_size = SpinButton::with_range(50.0, 1000.0, 10.0);
    spin_size.set_value(200.0);
    controls_vbox.append(&spin_size);

    controls_vbox.append(&Label::new(Some("Spawn Position X:")));
    let spin_x = SpinButton::with_range(0.0, 5000.0, 50.0);
    spin_x.set_value(100.0);
    controls_vbox.append(&spin_x);

    controls_vbox.append(&Label::new(Some("Spawn Position Y:")));
    let spin_y = SpinButton::with_range(0.0, 3000.0, 50.0);
    spin_y.set_value(100.0);
    controls_vbox.append(&spin_y);

    let check_hide = CheckButton::with_label("Smart Hide (Allow Clicks)");
    let check_top = CheckButton::with_label("Always on Top");

    controls_vbox.append(&check_hide);
    controls_vbox.append(&check_top);

    let spawn_btn = Button::with_label("✨ SPAWN ✨");
    spawn_btn.add_css_class("suggested-action");
    spawn_btn.set_margin_top(10);
    controls_vbox.append(&spawn_btn);

    main_layout.append(&controls_vbox);
    main_layout.append(&gtk::Separator::new(Orientation::Vertical));

    // --- RIGHT COLUMN: ACTIVE LIST ---
    let list_vbox = Box::new(Orientation::Vertical, 5);
    list_vbox.set_hexpand(true);
    list_vbox.append(&Label::new(Some("Active Chibis:")));

    let scrolled = ScrolledWindow::builder()
    .hscrollbar_policy(gtk::PolicyType::Never)
    .vexpand(true)
    .hexpand(true)
    .build();

    let list_box = ListBox::new();
    list_box.add_css_class("frame");
    scrolled.set_child(Some(&list_box));
    list_vbox.append(&scrolled);

    main_layout.append(&list_vbox);
    window.set_child(Some(&main_layout));

    // --- SPAWN LOGIC ---
    let app_clone = app.clone();
    let list_box_clone = list_box.clone();

    spawn_btn.connect_clicked(move |_| {
        let path_borrow = selected_path.borrow();
        if let Some(path) = &*path_borrow {
            spawn_chibi(
                &app_clone,
                path,
                spin_size.value() as i32,
                        spin_x.value() as i32,
                        spin_y.value() as i32,
                        check_hide.is_active(),
                        check_top.is_active(),
                        &list_box_clone
            );
        } else {
            eprintln!("No file selected");
        }
    });

    window.present();
}

fn spawn_chibi(
    app: &Application,
    path: &std::path::Path,
    size: i32,
    start_x: i32,
    start_y: i32,
    smart_hide: bool,
    always_on_top: bool,
    list_box: &ListBox
) {
    let window = gtk::Window::builder()
    .application(app)
    .default_width(size)
    .default_height(size)
    .decorated(false)
    .build();

    // Apply the transparent background class
    window.add_css_class("ghost-window");

    // Track state: Is drag mode enabled? Is the user currently dragging?
    let is_moving_mode = Rc::new(Cell::new(false));
    let is_dragging_active = Rc::new(Cell::new(false));

    // --- LAYER SHELL SETUP ---
    window.init_layer_shell();

    // Choose layer depth
    if always_on_top {
        window.set_layer(Layer::Overlay); // Above all windows
    } else {
        window.set_layer(Layer::Bottom);  // Above wallpaper
    }

    // Anchor to top-left for absolute positioning
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Left, true);
    window.set_margin(Edge::Left, start_x);
    window.set_margin(Edge::Top, start_y);

    // --- IMAGE CONTAINER ---
    let container = Box::new(Orientation::Vertical, 0);
    let picture = Picture::for_filename(path);
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_vexpand(true);
    picture.set_hexpand(true);
    picture.set_can_target(true);

    container.append(&picture);
    window.set_child(Some(&container));

    // --- DRAG LOGIC (Anchor Method) ---
    // We use a combination of Click (to grab) and Motion (to update position).
    // This prevents "rubber-banding" by locking the window to the mouse anchor point.
    let click_gesture = GestureClick::new();
    let motion_controller = EventControllerMotion::new();

    // Track global window position (Float for precision)
    let current_x = Rc::new(Cell::new(start_x as f64));
    let current_y = Rc::new(Cell::new(start_y as f64));

    // Track local click offset (The "Anchor" point)
    let anchor_x = Rc::new(Cell::new(0.0));
    let anchor_y = Rc::new(Cell::new(0.0));

    // 1. Mouse Down: Lock Anchor
    let drag_active_click = is_dragging_active.clone();
    let move_mode_click = is_moving_mode.clone();
    let ax = anchor_x.clone();
    let ay = anchor_y.clone();

    click_gesture.connect_pressed(move |_, _, x, y| {
        // Only allow dragging if "Move Mode" (Fist button) is active
        if move_mode_click.get() {
            drag_active_click.set(true);
            ax.set(x);
            ay.set(y);
        }
    });

    // 2. Mouse Up: Release
    let drag_active_release = is_dragging_active.clone();
    click_gesture.connect_released(move |_, _, _, _| {
        drag_active_release.set(false);
    });

    // 3. Mouse Move: Update Window Position
    let drag_active_motion = is_dragging_active.clone();
    let cx = current_x.clone();
    let cy = current_y.clone();
    let ax_m = anchor_x.clone();
    let ay_m = anchor_y.clone();
    let win_motion_weak = window.downgrade();

    motion_controller.connect_motion(move |_, x, y| {
        if !drag_active_motion.get() { return; }

        if let Some(win) = win_motion_weak.upgrade() {
            // Calculate how far the mouse has drifted from the anchor
            let delta_x = x - ax_m.get();
            let delta_y = y - ay_m.get();

            // Apply that drift to the window's global position
            let new_x = cx.get() + delta_x;
            let new_y = cy.get() + delta_y;

            win.set_margin(Edge::Left, new_x as i32);
            win.set_margin(Edge::Top, new_y as i32);

            cx.set(new_x);
            cy.set(new_y);
        }
    });

    window.add_controller(click_gesture);
    window.add_controller(motion_controller);

    // --- SMART HIDE LOGIC ---
    // If enabled, the window vanishes on hover to allow clicking through to the desktop.
    if smart_hide {
        let hide_controller = EventControllerMotion::new();
        let win_weak = window.downgrade();
        let is_moving_check = is_moving_mode.clone();

        hide_controller.connect_enter(move |_, _, _| {
            // Never hide if the user is in "Move Mode" (Fist active)
            if is_moving_check.get() { return; }

            if let Some(win) = win_weak.upgrade() {
                // Completely unmap the window so clicks pass through
                win.set_visible(false);

                // Start 3-second timer to reappear
                let win_timer_weak = win.downgrade();
                let is_moving_timer = is_moving_check.clone();

                glib::timeout_add_seconds_local(3, move || {
                    if let Some(w) = win_timer_weak.upgrade() {
                        // Only reappear if user didn't somehow enable move mode while hidden
                        if !is_moving_timer.get() {
                            w.set_visible(true);
                        }
                    }
                    glib::ControlFlow::Break
                });
            }
        });

        window.add_controller(hide_controller);
    }

    // --- ADD TO LIST ---
    let row = ListBoxRow::new();
    let row_box = Box::new(Orientation::Horizontal, 5);

    let fname = path.file_name().unwrap_or_default().to_string_lossy();
    let label_text = format!("{}", fname);

    let name_label = Label::new(Some(&label_text));
    name_label.set_hexpand(true);
    name_label.set_xalign(0.0);
    name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);

    // Drag Toggle Button (Hand/Fist)
    let move_btn = ToggleButton::with_label("✋");
    move_btn.set_tooltip_text(Some("Enable Dragging"));

    let is_moving_toggle = is_moving_mode.clone();
    let win_toggle = window.clone();

    move_btn.connect_toggled(move |btn| {
        let active = btn.is_active();
        is_moving_toggle.set(active);

        if active {
            // MODE: DRAGGING ENABLED
            btn.set_label("✊");
            win_toggle.set_visible(true);
            win_toggle.set_can_target(true);
            win_toggle.set_opacity(1.0);
        } else {
            // MODE: STATIONARY (Smart Hide active)
            btn.set_label("✋");
            win_toggle.set_can_target(true);
            is_dragging_active.set(false);
        }
    });

    let delete_btn = Button::with_label("❌");

    let win_close = window.clone();
    let row_weak = row.downgrade();
    let list_box_weak = list_box.downgrade();

    delete_btn.connect_clicked(move |_| {
        win_close.close();
        if let (Some(lb), Some(r)) = (list_box_weak.upgrade(), row_weak.upgrade()) {
            lb.remove(&r);
        }
    });

    row_box.append(&name_label);
    row_box.append(&move_btn);
    row_box.append(&delete_btn);
    row.set_child(Some(&row_box));
    list_box.append(&row);

    window.present();
}
