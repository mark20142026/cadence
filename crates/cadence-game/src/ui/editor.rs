//! The level editor (`LevelEditorLayer` + `EditorUI`): objects on a 30-unit grid, the build
//! tabs with every object the game has (`editor_tabs::TABS`, dumped from the real editor), the
//! edit tab's move/rotate/flip buttons, delete, undo/redo, zoom, playtest and saving.
//! The level itself is drawn by the game's renderer from `Editor::level_string`; this module
//! draws the UI over it. Positions are from the real editor frame (shots/gui-real/editor/).

use super::canvas::{Canvas, Look};
use super::editor_tabs::TABS;
use super::{Btn, Ctx};

/// One placed object.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct EdObject {
    pub id: u16,
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
    pub flip_x: bool,
    pub flip_y: bool,
    /// Every other key of the object as loaded (groups, colors, trigger settings…), written
    /// back unchanged.
    pub extra: Vec<(u16, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Build,
    Edit,
    Delete,
    View,
}

/// The tab icons (`EditorUI::setupCreateMenu`'s tab sprites, measured scales).
const TAB_ICONS: [(&str, f32); 13] = [
    ("square_01_001.png", 0.45),
    ("lightsquare_01_01_001.png", 0.45),
    ("triangle_a_02_001.png", 0.45),
    ("spike_01_001.png", 0.45),
    ("persp_outline_01_001.png", 0.8),
    ("ring_01_001.png", 0.45),
    ("GJBeast01_01_001.png", 0.3462),
    ("pixelb_03_01_001.png", 1.2273),
    ("pixelitem_001_001.png", 0.8438),
    ("particle_01_001.png", 0.8438),
    ("d_spikes_01_001.png", 0.1875),
    ("sawblade_02_001.png", 0.225),
    ("edit_eTintCol01Btn_001.png", 0.4821),
];

/// The edit tab's buttons: (sprite, label for the tooltip-less ones, action).
const EDIT_BUTTONS: [(&str, EditOp); 12] = [
    ("edit_upBtn_001.png", EditOp::Move(0.0, 30.0)),
    ("edit_downBtn_001.png", EditOp::Move(0.0, -30.0)),
    ("edit_leftBtn_001.png", EditOp::Move(-30.0, 0.0)),
    ("edit_rightBtn_001.png", EditOp::Move(30.0, 0.0)),
    ("edit_upBtn2_001.png", EditOp::Move(0.0, 2.0)),
    ("edit_downBtn2_001.png", EditOp::Move(0.0, -2.0)),
    ("edit_leftBtn2_001.png", EditOp::Move(-2.0, 0.0)),
    ("edit_rightBtn2_001.png", EditOp::Move(2.0, 0.0)),
    ("edit_flipXBtn_001.png", EditOp::FlipX),
    ("edit_flipYBtn_001.png", EditOp::FlipY),
    ("edit_cwBtn_001.png", EditOp::Rotate(90.0)),
    ("edit_ccwBtn_001.png", EditOp::Rotate(-90.0)),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EditOp {
    Move(f32, f32),
    FlipX,
    FlipY,
    Rotate(f32),
}

/// Level y is measured from the ground's top, which the game draws at world y = 90.
const GROUND: f32 = 90.0;

/// Buttons a page holds (6 per row, 2 rows by default: Settings > Editor's "Buttons per row"
/// and "Button rows").
const PER_ROW: usize = 6;
const ROWS: usize = 2;

pub struct Editor {
    /// The grid hidden (the top bar's grid button).
    pub hide_grid: bool,
    /// GameManager's `showBPMMarkers` (saved as such, off unless toggled with the edit tab's
    /// `edit_eBPMBtn`): BPM triggers (3642) draw their beat lines.
    pub show_bpm_markers: bool,
    /// Which local level (index in the saved list) this is, if saved.
    pub slot: Option<usize>,
    pub name: String,
    pub objects: Vec<EdObject>,
    /// The level's header (settings, colors) as loaded; empty for a new level (the default).
    pub header: String,
    pub mode: Mode,
    pub tab: usize,
    pub page: usize,
    /// The object id the build tab places.
    pub chosen: u16,
    pub selected: Vec<usize>,
    /// The camera: world position of the view's bottom left, and the zoom.
    pub cam: (f32, f32),
    pub zoom: f32,
    undo: Vec<Vec<EdObject>>,
    redo: Vec<Vec<EdObject>>,
    /// The level changed: the app rebuilds what it draws.
    pub dirty: bool,
    /// The objects are hidden (the eye button).
    pub hide_ui: bool,
    /// Copy Values: the first selected object's keys (`EditorUI::onCopyState`).
    pub state: Option<Vec<(u16, String)>>,
    /// The editor layer shown (−1: all): others are dimmed and can't be picked; new objects go
    /// on it (key 20).
    pub layer: i32,
    pub swipe: bool,
    pub rotate: bool,
    pub snap: bool,
    pub free_move: bool,
    /// A drag of the view: (mouse start, camera start).
    pub drag: Option<((f32, f32), (f32, f32))>,
    /// A swipe in progress (world points): where it started and where it is now.
    pub swipe_at: Option<((f32, f32), (f32, f32))>,
}

impl Default for Editor {
    fn default() -> Editor {
        Editor {
            hide_grid: false,
            show_bpm_markers: false,
            slot: None,
            name: "Unnamed 0".into(),
            objects: Vec::new(),
            header: String::new(),
            mode: Mode::Build,
            tab: 0,
            page: 0,
            // nothing picked until an object button is pressed (EditorUI starts with none)
            chosen: 0,
            selected: Vec::new(),
            cam: (0.0, -156.0),
            zoom: 1.0,
            undo: Vec::new(),
            redo: Vec::new(),
            dirty: true,
            hide_ui: false,
            state: None,
            layer: 0,
            swipe_at: None,
            swipe: false,
            rotate: false,
            snap: false,
            free_move: false,
            drag: None,
        }
    }
}

/// A new level's settings (the game's defaults: blue background, darker ground, normal speed).
const HEADER: &str = "kS38,1_40_2_125_3_255_11_255_12_255_13_255_4_-1_6_1000_7_1_15_1_18_0_8_1|1_0_2_102_3_255_11_255_12_255_13_255_4_-1_6_1001_7_1_15_1_18_0_8_1|1_0_2_102_3_255_11_255_12_255_13_255_4_-1_6_1009_7_1_15_1_18_0_8_1|1_255_2_255_3_255_11_255_12_255_13_255_4_-1_6_1002_7_1_15_1_18_0_8_1|1_255_2_255_3_255_11_255_12_255_13_255_4_-1_6_1004_7_1_15_1_18_0_8_1|,kA13,0,kA15,0,kA16,0,kA14,,kA6,0,kA7,0,kA25,0,kA17,0,kA18,0,kS39,0,kA2,0,kA3,0,kA8,0,kA4,0,kA9,0,kA10,0,kA22,0,kA23,0,kA24,0,kA27,1,kA40,1,kA48,1,kA41,1,kA42,1,kA28,0,kA29,0,kA31,1,kA32,1,kA36,0,kA43,0,kA44,0,kA45,1,kA46,0,kA33,1,kA34,1,kA35,0,kA37,1,kA38,1,kA39,1,kA19,0,kA26,0,kA20,0,kA21,0,kA11,0";

impl Editor {
    /// The level as the game stores it (header; then "1,id,2,x,3,y,..." per object).
    pub fn level_string(&self) -> String {
        let mut s = if self.header.is_empty() { String::from(HEADER) } else { self.header.clone() };
        s.push(';');
        for o in &self.objects {
            s.push_str(&format!("1,{},2,{},3,{}", o.id, o.x, o.y));
            if o.rotation != 0.0 {
                s.push_str(&format!(",6,{}", o.rotation));
            }
            if o.flip_x {
                s.push_str(",4,1");
            }
            if o.flip_y {
                s.push_str(",5,1");
            }
            for (k, v) in &o.extra {
                s.push_str(&format!(",{k},{v}"));
            }
            s.push(';');
        }
        s
    }

    /// Read a level string back: its header and objects, every key kept.
    pub fn from_level_string(name: &str, text: &str) -> Editor {
        let mut e = Editor { name: name.into(), ..Editor::default() };
        e.header = text.split(';').next().filter(|h| h.contains("kA")).unwrap_or("").to_string();
        for part in text.split(';').skip(1) {
            let f: Vec<&str> = part.split(',').collect();
            let mut o = EdObject::default();
            for kv in f.chunks(2) {
                if kv.len() < 2 {
                    continue;
                }
                match kv[0] {
                    "1" => o.id = kv[1].parse().unwrap_or(0),
                    "2" => o.x = kv[1].parse().unwrap_or(0.0),
                    "3" => o.y = kv[1].parse().unwrap_or(0.0),
                    "6" => o.rotation = kv[1].parse().unwrap_or(0.0),
                    "4" => o.flip_x = kv[1] == "1",
                    "5" => o.flip_y = kv[1] == "1",
                    k => {
                        if let Ok(k) = k.parse() {
                            o.extra.push((k, kv[1].to_string()));
                        }
                    }
                }
            }
            if o.id > 0 {
                e.objects.push(o);
            }
        }
        e
    }

    fn checkpoint(&mut self) {
        self.undo.push(self.objects.clone());
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }

    fn pages(&self) -> usize {
        TABS[self.tab].len().div_ceil(PER_ROW * ROWS).max(1)
    }

    /// The object under a world point (the last placed wins).
    fn object_at(&self, wx: f32, wy: f32) -> Option<usize> {
        let wy = wy - GROUND;
        self.objects.iter().rposition(|o| (o.x - wx).abs() <= 15.0 && (o.y - wy).abs() <= 15.0 && self.on_layer(o))
    }

    /// Whether an object is on the shown editor layer (key 20, or its second layer 61).
    pub fn on_layer(&self, o: &EdObject) -> bool {
        let l1 = o.get(20).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
        let l2 = o.get(61).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
        self.layer < 0 || l1 == self.layer || (l2 != 0 && l2 == self.layer)
    }

    /// A click on the level (not on a button): `wx, wy` in world units.
    pub fn click_world(&mut self, wx: f32, wy: f32) {
        match self.mode {
            Mode::Build if self.chosen == 0 => {}
            Mode::Build => {
                // EditorUI::onCreateObject: centered in the grid cell under the click
                let (gx, gy) = ((wx / 30.0).floor() * 30.0 + 15.0, ((wy - GROUND) / 30.0).floor() * 30.0 + 15.0);
                if gy < 0.0 {
                    return;
                }
                self.checkpoint();
                let mut o = EdObject { id: self.chosen, x: gx, y: gy, ..EdObject::default() };
                if self.layer > 0 {
                    o.set(20, Some(self.layer.to_string()));
                }
                self.objects.push(o);
                self.selected = vec![self.objects.len() - 1];
            }
            Mode::Edit => {
                self.selected = self.object_at(wx, wy).into_iter().collect();
            }
            Mode::Delete => {
                if let Some(i) = self.object_at(wx, wy) {
                    self.checkpoint();
                    self.objects.remove(i);
                    self.selected.clear();
                }
            }
            Mode::View => {}
        }
    }

    fn apply(&mut self, op: EditOp) {
        if self.selected.is_empty() {
            return;
        }
        self.checkpoint();
        for &i in &self.selected {
            let Some(o) = self.objects.get_mut(i) else { continue };
            match op {
                EditOp::Move(dx, dy) => {
                    o.x += dx;
                    o.y += dy;
                }
                EditOp::FlipX => o.flip_x = !o.flip_x,
                EditOp::FlipY => o.flip_y = !o.flip_y,
                EditOp::Rotate(a) => o.rotation = (o.rotation + a).rem_euclid(360.0),
            }
        }
    }

    /// A click on one of the editor's buttons. Returns true to playtest, and the app saves.
    pub fn click(&mut self, id: Btn) -> EdEffect {
        match id {
            Btn::EdMode(m) => {
                self.mode = [Mode::Build, Mode::Edit, Mode::Delete, Mode::View][m as usize % 4];
            }
            Btn::EdTab(t) => {
                self.tab = t as usize % TABS.len();
                self.page = 0;
            }
            Btn::EdPage(next) => {
                let n = self.pages();
                self.page = if next { (self.page + 1) % n } else { (self.page + n - 1) % n };
            }
            Btn::EdObject(slot) => {
                if let Some(&id) = TABS[self.tab].get(self.page * PER_ROW * ROWS + slot as usize) {
                    self.chosen = id;
                }
            }
            Btn::EdEdit(k) => self.apply(EDIT_BUTTONS[k as usize].1),
            Btn::EdUndo => {
                if let Some(prev) = self.undo.pop() {
                    self.redo.push(std::mem::replace(&mut self.objects, prev));
                    self.selected.clear();
                    self.dirty = true;
                }
            }
            Btn::EdRedo => {
                if let Some(next) = self.redo.pop() {
                    self.undo.push(std::mem::replace(&mut self.objects, next));
                    self.selected.clear();
                    self.dirty = true;
                }
            }
            Btn::EdTrash => {
                if !self.selected.is_empty() {
                    self.checkpoint();
                    let mut sel = std::mem::take(&mut self.selected);
                    sel.sort_unstable_by(|a, b| b.cmp(a));
                    for i in sel {
                        if i < self.objects.len() {
                            self.objects.remove(i);
                        }
                    }
                }
            }
            Btn::EdDeselect => self.selected.clear(),
            Btn::EdHide => self.hide_ui = !self.hide_ui,
            Btn::EdZoom(inward) => {
                // EditorUI::zoomIn / zoomOut: steps of 0.1 between 0.1 and 4, about the view's middle
                self.zoom = if inward { (self.zoom + 0.1).min(4.0) } else { (self.zoom - 0.1).max(0.1) };
            }
            Btn::EdSwipe => self.swipe = !self.swipe,
            Btn::EdRotateToggle => self.rotate = !self.rotate,
            Btn::EdFreeMove => self.free_move = !self.free_move,
            Btn::EdSnap => self.snap = !self.snap,
            Btn::EdCopy | Btn::EdPaste | Btn::EdDuplicate => {
                if !self.selected.is_empty() {
                    self.checkpoint();
                    let copies: Vec<EdObject> = self.selected.iter().filter_map(|&i| self.objects.get(i).cloned()).collect();
                    let start = self.objects.len();
                    self.objects.extend(copies);
                    self.selected = (start..self.objects.len()).collect();
                }
            }
            Btn::EdPlaytest => return EdEffect::Playtest,
            Btn::EdMusic => return EdEffect::Music,
            Btn::EdPause => return EdEffect::Pause,
            Btn::EdSide(14) if !self.selected.is_empty() => return EdEffect::EditGroup,
            // Go To Layer: show the selected object's layer
            Btn::EdSide(20) if !self.selected.is_empty() => {
                let l = self.selection().next().and_then(|o| o.get(20)).and_then(|v| v.parse().ok()).unwrap_or(0);
                self.layer = l;
            }
            // the layer arrows (0 down, 1 up) and all layers (2)
            Btn::EdLayer(0) => self.layer = (self.layer - 1).max(-1),
            Btn::EdLayer(1) => self.layer = (self.layer + 1).min(9999),
            Btn::EdLayer(_) => self.layer = if self.layer < 0 { 0 } else { -1 },
            Btn::EdSide(0) => return EdEffect::Settings,
            Btn::EdSide(13) if !self.selected.is_empty() => {
                let id = self.selection().next().map_or(0, |o| o.id);
                return EdEffect::EditSpecial(id);
            }
            // Copy Values / Paste State (every key but the object's own position and id) /
            // Paste Color (the color channels and their HSV)
            Btn::EdSide(16) => {
                let state = self.selection().next().map(|o| o.extra.clone());
                self.state = state;
            }
            Btn::EdSide(17) => {
                if let Some(state) = self.state.clone() {
                    self.edit_selected(move |o| o.extra = state.clone());
                }
            }
            Btn::EdSide(18) => {
                if let Some(state) = self.state.clone() {
                    const COLOR_KEYS: [u16; 8] = [21, 22, 41, 42, 43, 44, 45, 46];
                    self.edit_selected(move |o| {
                        for k in COLOR_KEYS {
                            o.set(k, state.iter().find(|(sk, _)| *sk == k).map(|(_, v)| v.clone()));
                        }
                    });
                }
            }
            Btn::EdSide(15) if !self.selected.is_empty() => {
                let id = self.selection().next().map_or(0, |o| o.id);
                return EdEffect::EditObject(id);
            }
            // the color button: the selection's colors (Edit Object on a plain object)
            Btn::EdSide(19) if !self.selected.is_empty() => {
                let id = self.selection().next().map_or(0, |o| o.id);
                return EdEffect::EditObject(id);
            }
            // the grid button: show or hide the grid
            Btn::EdSide(2) => self.hide_grid = !self.hide_grid,
            // the wrench: the editor's options
            Btn::EdSide(1) => return EdEffect::Options,
            Btn::EdSide(_) => {}
            _ => {}
        }
        EdEffect::None
    }

    /// Draw the UI over the level; `view` is the world rectangle the level is drawn in
    /// (x, y, width, height) for the grid and selection marks.
    pub fn draw(&self, c: &mut Canvas, ctx: &mut Ctx, view: (f32, f32, f32, f32)) {
        let (w, h) = (c.width, c.height);
        let to_screen = |x: f32, y: f32| ((x - view.0) * w / view.2, (y - view.1) * h / view.3);
        let unit = w / view.2;
        // the grid (DrawGridLayer): lines every 30 units, darker at the ground
        let x0 = (view.0 / 30.0).floor() * 30.0;
        let mut gx = if self.hide_grid { f32::INFINITY } else { x0 };
        while gx < view.0 + view.2 {
            let (sx, _) = to_screen(gx, 0.0);
            c.rect(sx - 0.25, 0.0, 0.5, h, [0.0; 3], 0.35);
            gx += 30.0;
        }
        let mut gy = if self.hide_grid { f32::INFINITY } else { (view.1 / 30.0).floor() * 30.0 };
        while gy < view.1 + view.3 {
            let (_, sy) = to_screen(0.0, gy);
            c.rect(0.0, sy - 0.25, w, 0.5, [0.0; 3], 0.35);
            gy += 30.0;
        }
        // the start line (x = 0)
        let (sx, _) = to_screen(0.0, 0.0);
        c.rect(sx - 0.5, 0.0, 1.0, h, [1.0, 1.0, 1.0], 0.5);
        // (the guides and the playback line are drawn by the app under the objects:
        // `guide_lines`)
        // selected objects: a green box
        for &i in &self.selected {
            if let Some(o) = self.objects.get(i) {
                let (sx, sy) = to_screen(o.x, o.y + GROUND);
                let r = 15.0 * unit;
                c.rect(sx - r, sy - r, 2.0 * r, 2.0 * r, [0.0, 1.0, 0.0], 0.25);
            }
        }
        // a swipe's selection box (Edit mode)
        if let (Some(((x0, y0), (x1, y1))), Mode::Edit) = (self.swipe_at, self.mode) {
            let (a, b) = (to_screen(x0.min(x1), y0.min(y1)), to_screen(x0.max(x1), y0.max(y1)));
            c.rect(a.0, a.1, b.0 - a.0, b.1 - a.1, [0.0, 1.0, 0.0], 0.15);
        }
        if self.hide_ui {
            ctx.button(c, Btn::EdHide, "sideBtn_hide_001.png", 158.0, h - 20.0, Look::default(), 1.3);
            return;
        }
        // the bottom bar
        // (it reaches behind the tab row: 117 high, measured on a real capture)
        c.rect(0.0, 0.0, w, 117.0, [0.0; 3], 0.85);
        c.sprite("edit_vLine_001.png", w * 0.5 - 188.5, 46.0, Look::default());
        c.sprite("edit_vLine_001.png", w * 0.5 + 188.5, 46.0, Look::default());
        // Build / Edit / Delete / View (toggle sprites on GJ_button_02 when on, 04 off)
        for (k, (icon, label, x, y)) in [("edit_buildBtn_001.png", "Build", 25.0, 67.0), ("edit_editBtn_001.png", "Edit", 69.0, 67.0), ("edit_deleteBtn_001.png", "Delete", 25.0, 24.0), ("edit_viewBtn_001.png", "View", 69.0, 24.0)]
            .into_iter()
            .enumerate()
        {
            let on = self.mode as usize == k;
            let id = Btn::EdMode(k as u8);
            let f = ctx.factor(id);
            c.nine_slice(if on { "GJ_button_02.png" } else { "GJ_button_04.png" }, x, y, 40.0 * f, 40.0 * f, Look::default());
            // (the icons carry their label)
            let _ = label;
            c.sprite(icon, x, y, Look::scaled(1.05 * f * 0.8));
            ctx.area(id, x, y, 40.0, 40.0);
        }
        // Swipe / Rotate / Free Move / Snap
        for (id, icon, x, y, on) in [
            (Btn::EdSwipe, "edit_swipeBtn_001.png", w - 69.0, 68.0, self.swipe),
            (Btn::EdRotateToggle, "edit_enableRotateBtn_001.png", w - 25.0, 68.0, self.rotate),
            (Btn::EdFreeMove, "edit_freeMoveBtn_001.png", w - 69.0, 24.0, self.free_move),
            (Btn::EdSnap, "edit_snapBtn_001.png", w - 25.0, 25.0, self.snap),
        ] {
            let f = ctx.factor(id);
            c.nine_slice(if on { "GJ_button_02.png" } else { "GJ_button_01.png" }, x, y, 40.0 * f, 40.0 * f, Look::default());
            c.sprite(icon, x, y, Look::scaled(f));
            ctx.area(id, x, y, 40.0, 40.0);
        }
        let (cx, _) = (w * 0.5, h * 0.5);
        match self.mode {
            Mode::Build => {
                // the tabs above the bar
                for (t, (icon, s)) in TAB_ICONS.iter().enumerate() {
                    let x = 63.5 + 34.0 * t as f32 + (w - 569.0) * 0.5;
                    let on = t == self.tab;
                    let id = Btn::EdTab(t as u8);
                    c.sprite(if on { "GJ_tabOn_001.png" } else { "GJ_tabOff_001.png" }, x, 100.0, if on { Look::default() } else { Look::default().alpha(150) });
                    let (iw, ih) = c.size(icon);
                    let fit = s.min(14.0 / iw.max(ih).max(1.0) * 1.0 / 0.45 * 0.45);
                    let _ = fit;
                    c.sprite(icon, x, 99.0, Look::scaled(*s));
                    ctx.area(id, x, 100.0, 32.0, 17.5);
                }
                let start = self.page * PER_ROW * ROWS;
                for slot in 0..PER_ROW * ROWS {
                    let Some(&oid) = TABS[self.tab].get(start + slot) else { break };
                    let (col, row) = (slot % PER_ROW, slot / PER_ROW);
                    let x = cx - 102.5 + 41.0 * col as f32;
                    let y = 70.5 - 41.0 * row as f32;
                    let id = Btn::EdObject(slot as u8);
                    let f = ctx.factor(id) * 0.925;
                    let bg = if oid == self.chosen { "GJ_button_02.png" } else { "GJ_button_04.png" };
                    c.nine_slice(bg, x, y, 40.0 * f, 40.0 * f, Look::default());
                    if let Some(v) = cadence_core::objects::visual(oid as i32) {
                        if let Some(sp) = v.sprites.first() {
                            let (fw, fh) = c.size(sp.frame);
                            let s = (30.0 / fw.max(fh).max(1.0)).min(1.0) * f;
                            c.sprite(sp.frame, x, y + 2.0 * f, Look::scaled(s));
                            // its visible children on top, placed from the main sprite's corner
                            for ch in v.sprites.iter().skip(1).filter(|ch| ch.parent == 0 && ch.visible && !ch.frame.is_empty()) {
                                let (dx, dy) = ((ch.x - sp.w * 0.5) * s, (ch.y - sp.h * 0.5) * s);
                                let look = Look::default().scale_xy(ch.scale_x * s * if ch.flip_x { -1.0 } else { 1.0 }, ch.scale_y * s * if ch.flip_y { -1.0 } else { 1.0 });
                                c.sprite(ch.frame, x + dx, y + 2.0 * f + dy, look);
                            }
                        }
                    }
                    ctx.area(id, x, y, 40.0 * 0.925, 40.0 * 0.925);
                }
                ctx.button(c, Btn::EdPage(false), "GJ_arrow_02_001.png", cx - 172.5, 50.0, Look::scaled(0.6), 1.5);
                ctx.button(c, Btn::EdPage(true), "GJ_arrow_02_001.png", cx + 172.5, 50.0, Look::scaled(0.6).flip_x(true), 1.5);
            }
            Mode::Edit => {
                for (k, (icon, _)) in EDIT_BUTTONS.iter().enumerate() {
                    let (col, row) = (k % PER_ROW, k / PER_ROW);
                    let x = cx - 102.5 + 41.0 * col as f32;
                    let y = 70.5 - 41.0 * row as f32;
                    let id = Btn::EdEdit(k as u8);
                    let f = ctx.factor(id) * 0.925;
                    c.nine_slice("GJ_button_04.png", x, y, 40.0 * f, 40.0 * f, Look::default());
                    c.sprite(icon, x, y, Look::scaled(f));
                    ctx.area(id, x, y, 37.0, 37.0);
                }
            }
            Mode::Delete | Mode::View => {}
        }
        // top left: undo, redo, trash, hide; left column: music, playtest, zoom
        for (id, icon, x, y, s) in [
            (Btn::EdUndo, "GJ_undoBtn_001.png", 25.0, h - 20.0, 1.0),
            (Btn::EdRedo, "GJ_redoBtn_001.png", 71.0, h - 20.0, 1.0),
            (Btn::EdTrash, "GJ_trashBtn_001.png", 117.0, h - 20.0, 0.9),
            (Btn::EdHide, "sideBtn_hide_001.png", 158.0, h - 20.0, 1.0),
            (Btn::EdMusic, "GJ_playMusicBtn_001.png", 25.0, h - 65.0, 1.0),
            (Btn::EdPlaytest, "GJ_playEditorBtn_001.png", 25.0, h - 110.0, 1.0),
            (Btn::EdZoom(true), "GJ_zoomInBtn_001.png", 25.0, h - 155.0, 0.8),
            (Btn::EdZoom(false), "GJ_zoomOutBtn_001.png", 25.0, h - 190.0, 0.8),
            // top right: pause, settings, config, grid
            (Btn::EdPause, "GJ_pauseBtn_001.png", w - 20.0, h - 20.0, 1.0),
            (Btn::EdSide(0), "GJ_optionsBtn02_001.png", w - 65.0, h - 20.0, 1.0),
            (Btn::EdSide(1), "sideBtn_config_001.png", w - 106.0, h - 20.0, 1.0),
            (Btn::EdSide(2), "sideBtn_grid_001.png", w - 142.0, h - 20.0, 1.0),
        ] {
            let grey = (id == Btn::EdUndo && self.undo.is_empty()) || (id == Btn::EdRedo && self.redo.is_empty()) || (id == Btn::EdTrash && self.selected.is_empty());
            let look = if grey { Look::scaled(s).alpha(120) } else { Look::scaled(s) };
            ctx.button(c, id, icon, x, y, look, 1.2);
        }
        // the right column: copy, paste, edit, ... (3 × 4)
        for (k, icon) in [
            "GJ_copyBtn2_001.png",
            "GJ_pasteBtn2_001.png",
            "GJ_duplicateObjectBtn2_001.png",
            "GJ_editObjBtn4_001.png",
            "GJ_groupIDBtn2_001.png",
            "GJ_editObjBtn3_001.png",
            "GJ_copyStateBtn_001.png",
            "GJ_pasteStateBtn_001.png",
            "GJ_pasteColorBtn_001.png",
            "GJ_editHSVBtn2_001.png",
            "GJ_goToLayerBtn_001.png",
            "GJ_deSelBtn2_001.png",
        ]
        .into_iter()
        .enumerate()
        {
            let (col, row) = (k % 3, k / 3);
            let x = w - 100.0 + 38.0 * col as f32;
            let y = h - 60.0 - 35.0 * row as f32;
            let id = match k {
                0 => Btn::EdCopy,
                1 => Btn::EdPaste,
                2 => Btn::EdDuplicate,
                11 => Btn::EdDeselect,
                _ => Btn::EdSide(10 + k as u8),
            };
            // Paste needs something copied (Copy Values / a selection), the rest a selection
            let grey = if k == 1 { self.selected.is_empty() && self.state.is_none() } else { self.selected.is_empty() };
            ctx.button(c, id, icon, x, y, if grey { Look::default().alpha(150) } else { Look::default() }, 1.1);
        }
        // the layer between its arrows ("All" with every layer shown); the gray arrow: all layers
        let label = if self.layer < 0 { "All".to_string() } else { self.layer.to_string() };
        let (lw, _) = c.text_size("bigFont", &label);
        c.text("bigFont", &label, w - 46.0, h - 197.0, Look::scaled((40.0 / lw.max(1.0)).min(0.6)));
        ctx.button(c, Btn::EdLayer(1), "GJ_arrow_03_001.png", w - 16.0, h - 197.0, Look::scaled(0.6).flip_x(true), 1.2);
        ctx.button(c, Btn::EdLayer(0), "GJ_arrow_03_001.png", w - 76.0, h - 197.0, Look::scaled(0.6), 1.2);
        ctx.button(c, Btn::EdLayer(2), "GJ_arrow_02_001.png", w - 104.0, h - 197.0, Look::scaled(0.54).alpha(175), 1.2);
        // the position slider at the top
        let pos = (self.cam.0 / 30_000.0).clamp(0.0, 1.0);
        // the level position slider (captured): groove slidergroove_02 at 0.8, the move button
        // as its thumb
        c.sprite("slidergroove_02.png", cx + 8.0, h - 20.0, Look::scaled(0.8).anchor(0.5, 0.5));
        let (gw, _) = c.size("slidergroove_02.png");
        c.sprite("GJ_moveBtn.png", cx + 8.0 - gw * 0.4 + gw * 0.8 * pos, h - 20.0, Look::default());
    }
}

/// What a button asks the app to do.
pub enum EdEffect {
    None,
    Playtest,
    Music,
    Pause,
    /// The Edit Group popup (`EditorUI::editGroup`)
    EditGroup,
    /// Edit Object (`EditorUI::editObject`) on the first selected object, by its id
    EditObject(u16),
    /// The level's settings (`EditorUI::onSettings`)
    Settings,
    /// Edit Special (`EditorUI::editObject2`) on the first selected object, by its id
    EditSpecial(u16),
    /// The editor's options (the top bar's wrench)
    Options,
}

/// The Z Layer buttons' values (`ZLayer`: B5 −5 … T4 11).
pub const Z_LAYERS: [i32; 9] = [-5, -3, -1, 1, 3, 5, 7, 9, 11];

/// The editor's line speeds (units per second) by a speed index (0 normal, 1 slow, 2 fast,
/// 3 faster, 4 fastest), as `DrawGridLayer` keeps them.
const GUIDE_SPEEDS: [f32; 5] = [311.58011, 251.16007, 387.42014, 468.00015, 576.00018];

/// A guide line in level space, drawn by the app under the level's objects (the game's
/// `DrawGridLayer` is under the object layer).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GuideLine {
    pub from: (f32, f32),
    pub to: (f32, f32),
    pub color: [f32; 3],
    pub alpha: f32,
    /// Level units, or framebuffer pixels (`glLineWidth`) when `pixels`.
    pub width: f32,
    pub pixels: bool,
}

impl Editor {
    /// The guide objects' overlays, in level space (`view` = x, y, width, height of the view):
    /// - Camera Guide 2016 (`LevelEditorLayer::updateDebugDraw` 0xba678): with preview opacity 506
    ///   above 0, zoom 371 held to 0.4..3: a green 480 × 320 rectangle ÷ zoom (the game's
    ///   reference view) and, when the window isn't 3:2, an orange (255, 128, 0) one of the window's
    ///   size ÷ zoom, both centered on the object moved by offset 28 / 29 and kept above y = half
    ///   their height; a yellow line through the object's x across the green one. All in the
    ///   preview opacity, half a unit wide (CCDrawNode, radius 0.25 ÷ the layer's scale).
    /// - BPM 3642 (`DrawGridLayer::draw` 0xbda9c, with `showBPMMarkers`): unless disabled (500),
    ///   with BPM 498 above 0, a line every speed(499) · 60 / (BPM · max(1, 501)) units from the
    ///   object's x over duration 10 seconds of the speed: every 501st line yellow, the others
    ///   orange (all yellow when 501 < 1); one pixel wide, opaque.
    /// - The music playback line (`DrawGridLayer::draw`, while EditorUI plays the song): green
    ///   (50, 255, 50) at posForTime(song time), 5 pixels wide, from y −3000 up; once stopped it
    ///   stays at its last place, 3 pixels wide at opacity 100. `playback` = (x, playing).
    pub fn guide_lines(&self, view: (f32, f32, f32, f32), playback: Option<(f32, bool)>) -> Vec<GuideLine> {
        let mut out = Vec::new();
        let f = |o: &EdObject, k: u16| o.get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        let world = |from: (f32, f32), to: (f32, f32), color: [f32; 3], alpha: f32| GuideLine { from, to, color, alpha, width: 0.5, pixels: false };
        for o in self.objects.iter().filter(|o| o.id == 2016) {
            let opacity = f(o, 506);
            if opacity <= 0.0 {
                continue;
            }
            let zoom = f(o, 371).min(3.0).max(0.4);
            let (ox, oy) = (f(o, 28), f(o, 29));
            let mut rect = |rw: f32, rh: f32, color: [f32; 3]| {
                let cy = o.y.max(rh * 0.5);
                let (x0, y0) = (o.x + ox - rw * 0.5, cy + oy - rh * 0.5 + GROUND);
                let (x1, y1) = (o.x + ox + rw * 0.5, cy + oy + rh * 0.5 + GROUND);
                out.push(world((x0, y0), (x1, y0), color, opacity));
                out.push(world((x0, y1), (x1, y1), color, opacity));
                out.push(world((x0, y0), (x0, y1), color, opacity));
                out.push(world((x1, y0), (x1, y1), color, opacity));
                (y0, y1)
            };
            // the window (Cadence's view is VIEW_HEIGHT tall, its width the window's aspect)
            let (win_w, win_h) = (view.2 * self.zoom, view.3 * self.zoom);
            if (480.0 / zoom, 320.0 / zoom) != (win_w / zoom, win_h / zoom) {
                rect(win_w / zoom, win_h / zoom, [1.0, 0.5, 0.0]);
            }
            let (y0, y1) = rect(480.0 / zoom, 320.0 / zoom, [0.0, 1.0, 0.0]);
            out.push(world((o.x, y0), (o.x, y1), [1.0, 1.0, 0.0], opacity));
        }
        let (bottom, top) = (view.1, view.1 + view.3);
        if self.show_bpm_markers {
            for o in self.objects.iter().filter(|o| o.id == 3642) {
                let bpm = f(o, 498) as i32;
                if bpm <= 0 || f(o, 500) as i32 != 0 {
                    continue;
                }
                let per_bar = f(o, 501) as i32;
                let speed = GUIDE_SPEEDS.get(f(o, 499) as usize).copied().unwrap_or(GUIDE_SPEEDS[0]);
                let step = speed * (60.0 / (per_bar.max(1) * bpm) as f32);
                let end = (o.x + speed * f(o, 10)).min(view.0 + view.2);
                let (mut x, mut n) = (o.x, 0i32);
                if view.0 > x {
                    let skip = ((view.0 - x) / step) as i32;
                    n = skip;
                    x += step * skip as f32;
                }
                while x < end {
                    let yellow = per_bar < 1 || n % per_bar == 0;
                    let color = if yellow { [1.0, 1.0, 0.0] } else { [1.0, 0.5, 0.0] };
                    out.push(GuideLine { from: (x, bottom), to: (x, top), color, alpha: 1.0, width: 1.0, pixels: true });
                    x += step;
                    n += 1;
                }
            }
        }
        if let Some((x, playing)) = playback.filter(|&(x, _)| 0.0 < x) {
            let (alpha, width) = if playing { (1.0, 5.0) } else { (100.0 / 255.0, 3.0) };
            out.push(GuideLine { from: (x, bottom.max(-3000.0)), to: (x, top), color: [50.0 / 255.0, 1.0, 50.0 / 255.0], alpha, width, pixels: true });
        }
        out
    }

    /// `LevelTools::posForTime`: where the level is `t` seconds in, at the start speed (kA4)
    /// and through the speed portals (200 slow, 201 normal, 202 fast, 203 faster, 1334 fastest)
    /// — the music playback line's x.
    pub fn playback_x(&self, t: f32) -> f32 {
        let index = |id: u16| match id {
            200 => Some(1),
            201 => Some(0),
            202 => Some(2),
            203 => Some(3),
            1334 => Some(4),
            _ => None,
        };
        let mut portals: Vec<(f32, usize)> = self.objects.iter().filter_map(|o| index(o.id).map(|i| (o.x, i))).collect();
        portals.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut speed = GUIDE_SPEEDS[self.setting("kA4").and_then(|v| v.parse::<usize>().ok()).filter(|&i| i < 5).unwrap_or(0)];
        let (mut x, mut t) = (0.0f32, t.max(0.0));
        for (px, i) in portals {
            if px <= x {
                speed = GUIDE_SPEEDS[i];
                continue;
            }
            let dt = (px - x) / speed;
            if t < dt {
                break;
            }
            t -= dt;
            x = px;
            speed = GUIDE_SPEEDS[i];
        }
        x + t * speed
    }
}

impl EdObject {
    pub fn get(&self, key: u16) -> Option<&str> {
        self.extra.iter().find(|(k, _)| *k == key).map(|(_, v)| v.as_str())
    }

    pub fn set(&mut self, key: u16, value: Option<String>) {
        self.extra.retain(|(k, _)| *k != key);
        if let Some(v) = value {
            self.extra.push((key, v));
        }
    }

    /// The groups (key 57, '.'-separated).
    pub fn groups(&self) -> Vec<i32> {
        self.get(57).map(|g| g.split('.').filter_map(|v| v.parse().ok()).collect()).unwrap_or_default()
    }

    pub fn set_groups(&mut self, groups: &[i32]) {
        let text = groups.iter().map(|g| g.to_string()).collect::<Vec<_>>().join(".");
        self.set(57, (!groups.is_empty()).then_some(text));
    }
}

impl Editor {
    /// The selected objects (first one first).
    pub fn selection(&self) -> impl Iterator<Item = &EdObject> {
        self.selected.iter().filter_map(|&i| self.objects.get(i))
    }

    /// Change every selected object (one undo step).
    pub fn edit_selected(&mut self, f: impl Fn(&mut EdObject)) {
        if self.selected.is_empty() {
            return;
        }
        self.checkpoint();
        for &i in &self.selected {
            if let Some(o) = self.objects.get_mut(i) {
                f(o);
            }
        }
    }

    /// A level setting from the header (`kA4` speed, `kA2` mode, `kA22` platformer, `kA1` song…).
    pub fn setting(&self, key: &str) -> Option<String> {
        let header = if self.header.is_empty() { HEADER } else { self.header.as_str() };
        let f: Vec<&str> = header.split(',').collect();
        f.chunks(2).find(|kv| kv[0] == key).and_then(|kv| kv.get(1)).map(|v| v.to_string())
    }

    /// A color channel's starting color in the header (`kS38`: `1_r_2_g_3_b_…_6_<channel>`
    /// entries, `|` apart); white when it isn't there.
    pub fn channel_color(&self, channel: i32) -> [u8; 3] {
        let ks38 = self.setting("kS38").unwrap_or_default();
        for entry in ks38.split('|') {
            let f: Vec<&str> = entry.split('_').collect();
            let get = |k: &str| f.chunks(2).find(|kv| kv[0] == k).and_then(|kv| kv.get(1)).and_then(|v| v.parse::<i32>().ok());
            if get("6") == Some(channel) {
                return [get("1"), get("2"), get("3")].map(|v| v.unwrap_or(255).clamp(0, 255) as u8);
            }
        }
        [255, 255, 255]
    }

    /// Set a channel's starting color (adding its entry when it has none).
    pub fn set_channel_color(&mut self, channel: i32, rgb: [u8; 3]) {
        let ks38 = self.setting("kS38").unwrap_or_default();
        let mut found = false;
        let mut entries: Vec<String> = ks38
            .split('|')
            .filter(|e| !e.is_empty())
            .map(|entry| {
                let mut f: Vec<String> = entry.split('_').map(str::to_string).collect();
                let is = f.chunks(2).any(|kv| kv[0] == "6" && kv.get(1).and_then(|v| v.parse::<i32>().ok()) == Some(channel));
                if is {
                    found = true;
                    for (k, v) in [("1", rgb[0]), ("2", rgb[1]), ("3", rgb[2])] {
                        if let Some(i) = (0..f.len()).step_by(2).find(|&i| f[i] == k) {
                            if i + 1 < f.len() {
                                f[i + 1] = v.to_string();
                            }
                        }
                    }
                }
                f.join("_")
            })
            .collect();
        if !found {
            entries.push(format!("1_{}_2_{}_3_{}_11_255_12_255_13_255_4_-1_6_{channel}_7_1_15_1_18_0_8_1", rgb[0], rgb[1], rgb[2]));
        }
        self.set_setting("kS38", &(entries.join("|") + "|"));
    }

    /// Change a level setting (one undo step isn't kept for settings, like the game's).
    pub fn set_setting(&mut self, key: &str, value: &str) {
        let header = if self.header.is_empty() { HEADER.to_string() } else { std::mem::take(&mut self.header) };
        let mut f: Vec<String> = header.split(',').map(str::to_string).collect();
        match f.chunks_mut(2).find(|kv| kv[0] == key) {
            Some(kv) if kv.len() == 2 => kv[1] = value.to_string(),
            _ => {
                f.push(key.to_string());
                f.push(value.to_string());
            }
        }
        self.header = f.join(",");
        self.dirty = true;
    }

    /// Swipe (`EditorUI::ccTouchBegan` with swipe on): Edit drags a selection box, Build and
    /// Delete place / delete along the drag.
    pub fn swipe_start(&mut self, wx: f32, wy: f32) {
        self.swipe_at = Some(((wx, wy), (wx, wy)));
        if self.mode != Mode::Edit {
            self.click_world(wx, wy);
        }
    }

    pub fn swipe_move(&mut self, wx: f32, wy: f32) {
        let Some((from, last)) = self.swipe_at else { return };
        self.swipe_at = Some((from, (wx, wy)));
        let cell = |x: f32, y: f32| ((x / 30.0).floor() as i32, ((y - GROUND) / 30.0).floor() as i32);
        if self.mode != Mode::Edit && cell(wx, wy) != cell(last.0, last.1) {
            self.click_world(wx, wy);
        }
    }

    /// The end of a swipe: Edit selects every object whose center is in the box.
    pub fn swipe_end(&mut self) {
        let Some(((x0, y0), (x1, y1))) = self.swipe_at.take() else { return };
        if self.mode != Mode::Edit {
            return;
        }
        let (lx, hx) = (x0.min(x1), x0.max(x1));
        let (ly, hy) = (y0.min(y1) - GROUND, y0.max(y1) - GROUND);
        self.selected = (0..self.objects.len()).filter(|&i| {
            let o = &self.objects[i];
            o.x >= lx && o.x <= hx && o.y >= ly && o.y <= hy
        }).collect();
    }

    /// The lowest group id no object uses (`getNextFreeGroupID`).
    pub fn next_free_group(&self) -> i32 {
        let used: std::collections::HashSet<i32> = self.objects.iter().flat_map(|o| o.groups()).collect();
        (1..10000).find(|g| !used.contains(g)).unwrap_or(9999)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_colors_in_the_header() {
        let mut e = Editor::default();
        assert_eq!(e.channel_color(1000), [40, 125, 255]);
        e.set_channel_color(1000, [1, 2, 3]);
        assert_eq!(e.channel_color(1000), [1, 2, 3]);
        assert_eq!(e.channel_color(1001), [0, 102, 255]);
        e.set_channel_color(1013, [9, 8, 7]);
        assert_eq!(e.channel_color(1013), [9, 8, 7]);
    }

    #[test]
    fn places_on_the_grid_and_round_trips() {
        let mut e = Editor::default();
        // nothing picked: a click places nothing
        e.click_world(20.0, 100.0);
        assert!(e.objects.is_empty());
        e.chosen = 1;
        // a click in the first cell above the ground (world y 90..120)
        e.click_world(20.0, 100.0);
        e.chosen = 8;
        e.click_world(47.0, 131.0);
        assert_eq!(e.objects.len(), 2);
        assert_eq!((e.objects[0].x, e.objects[0].y), (15.0, 15.0));
        assert_eq!((e.objects[1].id, e.objects[1].x, e.objects[1].y), (8, 45.0, 45.0));
        // below the ground: nothing
        e.click_world(20.0, 50.0);
        assert_eq!(e.objects.len(), 2);
        let back = Editor::from_level_string("x", &e.level_string());
        assert_eq!(back.objects, e.objects);
    }

    #[test]
    fn keeps_unknown_keys_and_the_header() {
        let text = "kA13,0,kA2,1,kS38,1_40_2_125;1,901,2,105,3,15,51,7,10,0.5,57,3.4;1,1,2,15,3,15,21,1004;";
        let e = Editor::from_level_string("x", text);
        assert_eq!(e.header, "kA13,0,kA2,1,kS38,1_40_2_125");
        assert_eq!(e.objects[0].extra, vec![(51, "7".into()), (10, "0.5".into()), (57, "3.4".into())]);
        assert_eq!(e.level_string(), text);
    }

    #[test]
    fn level_settings_edit_the_header() {
        let mut e = Editor::default();
        assert_eq!(e.setting("kA4").as_deref(), Some("0"));
        e.set_setting("kA4", "2");
        e.set_setting("kA22", "1");
        assert_eq!(e.setting("kA4").as_deref(), Some("2"));
        assert!(e.level_string().contains(",kA4,2,") && e.level_string().contains("kA22,1"));
    }

    #[test]
    fn swipe_selects_and_places() {
        let mut e = Editor::default();
        e.chosen = 1;
        e.swipe = true;
        // Build: a drag over three cells places three blocks
        e.swipe_start(20.0, 100.0);
        e.swipe_move(50.0, 100.0);
        e.swipe_move(55.0, 100.0);
        e.swipe_move(80.0, 100.0);
        e.swipe_end();
        assert_eq!(e.objects.len(), 3);
        // Edit: a box over the first two selects them
        e.mode = Mode::Edit;
        e.swipe_start(0.0, 91.0);
        e.swipe_move(60.0, 119.0);
        e.swipe_end();
        assert_eq!(e.selected, vec![0, 1]);
    }

    #[test]
    fn layers_filter_picking_and_tag_new_objects() {
        let mut e = Editor::default();
        e.chosen = 1;
        e.click_world(20.0, 100.0);
        let _ = e.click(Btn::EdLayer(1));
        let _ = e.click(Btn::EdLayer(1));
        assert_eq!(e.layer, 2);
        e.click_world(50.0, 100.0);
        assert_eq!(e.objects[1].get(20), Some("2"));
        // on layer 2 the first block (layer 0) can't be picked
        e.mode = Mode::Edit;
        e.click_world(16.0, 104.0);
        assert!(e.selected.is_empty());
        // all layers: it can
        let _ = e.click(Btn::EdLayer(2));
        assert_eq!(e.layer, -1);
        e.click_world(16.0, 104.0);
        assert_eq!(e.selected, vec![0]);
    }

    #[test]
    fn delete_undo_redo_and_moves() {
        let mut e = Editor::default();
        e.chosen = 1;
        e.click_world(20.0, 100.0);
        e.mode = Mode::Delete;
        e.click_world(16.0, 104.0);
        assert!(e.objects.is_empty());
        let _ = e.click(Btn::EdUndo);
        assert_eq!(e.objects.len(), 1);
        let _ = e.click(Btn::EdRedo);
        assert!(e.objects.is_empty());
        let _ = e.click(Btn::EdUndo);
        e.selected = vec![0];
        let _ = e.click(Btn::EdEdit(3)); // right one block
        let _ = e.click(Btn::EdEdit(10)); // rotate 90
        assert_eq!((e.objects[0].x, e.objects[0].rotation), (45.0, 90.0));
        assert!(e.level_string().contains("1,1,2,45,3,15,6,90;"));
    }
}
