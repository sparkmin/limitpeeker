//! What was chosen last, from the command line, the popup or the settings
//! window: the boxes on the taskbar, each with one look or all of them in
//! turn, its background, the services it shows and which of each service's
//! limits, and its place; the fill mode; and the popup summary's own picks.
//! The tray thread writes it and the controller's worker reads it every
//! tick; the worker leaves the limits it last saw for the popup and the
//! window to offer. The tray thread saves it for the next run.

use crate::boxes::{self, CustomBox};
use crate::style::{self, Background, Fill, LOOKS, Look};
use crate::usage::{MAX_CELLS, Picks, Service, cell_count, most_limits};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// A service's limits as the worker last saw them, for the popup. A
/// service not read yet has none.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Offer {
    pub id: &'static str,
    pub name: String,
    pub labels: Vec<String>,
}

/// Where a box goes on the taskbar: beside the tray, the taskbar's left end,
/// just before the Start button, or just after the last app.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BoxPlace {
    #[default]
    Tray,
    Left,
    BeforeStart,
    AfterApps,
}

/// What the popup and the settings window send for a box's place to hide
/// it, and are told while it is hidden.
pub const HIDDEN: &str = "hidden";

impl BoxPlace {
    /// The places as they are on the taskbar, from the left.
    pub const ALL: [BoxPlace; 4] = [
        BoxPlace::Left,
        BoxPlace::BeforeStart,
        BoxPlace::AfterApps,
        BoxPlace::Tray,
    ];
    pub fn name(self) -> &'static str {
        match self {
            BoxPlace::Tray => "tray",
            BoxPlace::Left => "left",
            BoxPlace::BeforeStart => "before-start",
            BoxPlace::AfterApps => "after-apps",
        }
    }
    pub fn find(name: &str) -> Option<BoxPlace> {
        Self::ALL.into_iter().find(|place| place.name() == name)
    }
    /// A place by the hook's word for it.
    pub fn from_word(word: u32) -> Option<BoxPlace> {
        Self::ALL.into_iter().find(|place| place.word() == word)
    }
    /// The hook's enum Place for it: 1 the left end, 2 beside the tray, 4
    /// before the Start button, 5 after the last app. Must match enum Place
    /// in native/taskbar_tap.cpp.
    pub fn word(self) -> u32 {
        match self {
            BoxPlace::Left => 1,
            BoxPlace::Tray => 2,
            BoxPlace::BeforeStart => 4,
            BoxPlace::AfterApps => 5,
        }
    }
}

/// The most boxes on the taskbar, as the hook has words for.
pub const MAX_TILES: usize = 4;
/// The longest name a box takes.
pub const MAX_NAME: usize = 32;
/// The editor's shelves folded at first: the samples of the preset looks.
pub const FOLDED: [&str; 2] = ["ai", "pc"];

/// One box on the taskbar: its look (an index into LOOKS), the custom box
/// chosen in its place, by name, there or not; its background; the services
/// and limits picked for it; and its place. Boxes in one place stand in the
/// list's order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tile {
    pub look: usize,
    pub custom: Option<String>,
    pub background: Background,
    pub picks: Picks,
    pub place: BoxPlace,
    /// Its own name; none for one made from what it shows.
    pub name: Option<String>,
    /// Kept, with its place, but not on the taskbar.
    pub hidden: bool,
    /// Its own color, ARGB; none for the box color every look starts with.
    pub color: Option<u32>,
}

impl Default for Tile {
    fn default() -> Self {
        Self::new(style::DEFAULT, Background::Box)
    }
}

impl Tile {
    pub fn new(look: &'static Look, background: Background) -> Self {
        let mut tile = Self {
            look: 0,
            custom: None,
            background,
            picks: Picks::default(),
            place: BoxPlace::default(),
            name: None,
            hidden: false,
            color: None,
        };
        tile.set_look(look);
        tile
    }
    pub fn look(&self) -> &'static Look {
        LOOKS.get(self.look).unwrap_or(style::DEFAULT)
    }
    pub fn set_look(&mut self, look: &'static Look) {
        self.look = LOOKS.iter().position(|l| l.name == look.name).unwrap_or(0);
    }
}

pub struct Choice {
    /// The boxes, one at least, and the one the popup and the settings
    /// window change.
    tiles: Mutex<Vec<Tile>>,
    current: AtomicUsize,
    /// An index into Fill::ALL.
    fill: AtomicU32,
    /// The popup summary's own picks: its services, their order, and each
    /// one's lines.
    summary: Mutex<Picks>,
    offers: Mutex<Vec<Offer>>,
    /// The services as the worker last saw them, values and all, for the
    /// tray's popup.
    read: Mutex<Vec<Service>>,
    /// The custom boxes, the samples and those in files.
    boxes: Mutex<Vec<Arc<CustomBox>>>,
    /// Whether the popup is open, as the tray thread last saw: its summary
    /// shows only then.
    popup: AtomicBool,
    /// Whether a box may be dragged on the taskbar.
    drag: AtomicBool,
    /// Whether each tool's own update command runs every 15 minutes.
    updates: AtomicBool,
    /// Whether values that run low warn in color.
    warn: AtomicBool,
    /// Which notifications are on, and the line of almost used up.
    alerts: Mutex<crate::notify::Alerts>,
    /// The settings window's language, "" for Windows'. Kept for the window
    /// only: the boxes, the popup and the notifications stay in English.
    language: Mutex<String>,
    /// The editor's shelves folded, by their words.
    folded: Mutex<Vec<String>>,
    /// The command line's four sample boxes, to check several boxes.
    sample_boxes: AtomicBool,
}

impl Choice {
    pub fn new(look: &'static Look, fill: Fill, background: Background) -> Self {
        let choice = Self {
            tiles: Mutex::new(vec![Tile::new(look, background)]),
            current: AtomicUsize::new(0),
            fill: AtomicU32::new(0),
            summary: Mutex::default(),
            offers: Mutex::default(),
            read: Mutex::default(),
            boxes: Mutex::new(boxes::samples().into_iter().map(Arc::new).collect()),
            popup: AtomicBool::new(false),
            drag: AtomicBool::new(true),
            // Off until the settings say on.
            updates: AtomicBool::new(false),
            warn: AtomicBool::new(true),
            alerts: Mutex::default(),
            language: Mutex::default(),
            folded: Mutex::new(FOLDED.map(String::from).to_vec()),
            sample_boxes: AtomicBool::new(false),
        };
        choice.set_fill(fill);
        choice
    }

    /// The boxes, in the list's order.
    pub fn tiles(&self) -> Vec<Tile> {
        self.tiles.lock().map(|t| t.clone()).unwrap_or_default()
    }
    /// Takes the boxes saved from the last run: one at least, MAX_TILES at
    /// most; the first is chosen.
    pub fn set_tiles(&self, mut tiles: Vec<Tile>) {
        tiles.truncate(MAX_TILES);
        if tiles.is_empty() {
            tiles.push(Tile::default());
        }
        if let Ok(mut held) = self.tiles.lock() {
            *held = tiles;
        }
        self.current.store(0, Ordering::SeqCst);
    }
    /// The box the popup and the settings window change.
    pub fn current(&self) -> usize {
        let count = self.tiles.lock().map(|t| t.len()).unwrap_or(1);
        self.current
            .load(Ordering::SeqCst)
            .min(count.saturating_sub(1))
    }
    /// Chooses the box the popup and the settings window change; a number
    /// past the last is not taken.
    pub fn select(&self, index: usize) -> bool {
        let count = self.tiles.lock().map(|t| t.len()).unwrap_or(1);
        if index >= count {
            return false;
        }
        self.current.store(index, Ordering::SeqCst);
        true
    }
    /// The box chosen, as it is now.
    pub fn tile(&self) -> Tile {
        let index = self.current();
        self.tiles
            .lock()
            .ok()
            .and_then(|t| t.get(index).cloned())
            .unwrap_or_default()
    }
    /// Changes the box chosen.
    fn change<R: Default>(&self, change: impl FnOnce(&mut Tile) -> R) -> R {
        let index = self.current();
        match self.tiles.lock() {
            Ok(mut tiles) => tiles.get_mut(index).map(change).unwrap_or_default(),
            Err(_) => R::default(),
        }
    }
    /// A new box after the last, made like the first and beside the tray,
    /// and chosen; not past MAX_TILES.
    pub fn add_tile(&self) -> bool {
        let Ok(mut tiles) = self.tiles.lock() else {
            return false;
        };
        if tiles.len() >= MAX_TILES {
            return false;
        }
        let tile = Tile {
            place: BoxPlace::Tray,
            name: None,
            hidden: false,
            ..tiles.first().cloned().unwrap_or_default()
        };
        tiles.push(tile);
        self.current.store(tiles.len() - 1, Ordering::SeqCst);
        true
    }
    /// Takes the box chosen away; the last one stays.
    pub fn remove_tile(&self) -> bool {
        let index = self.current();
        let Ok(mut tiles) = self.tiles.lock() else {
            return false;
        };
        if tiles.len() <= 1 {
            return false;
        }
        tiles.remove(index);
        self.current
            .store(index.min(tiles.len() - 1), Ordering::SeqCst);
        true
    }
    /// The boxes in the chosen one's place before and after it, nearest
    /// first; a hidden box has none, and is none.
    fn neighbours(tiles: &[Tile], index: usize) -> (Option<usize>, Option<usize>) {
        let Some(place) = tiles.get(index).filter(|t| !t.hidden).map(|t| t.place) else {
            return (None, None);
        };
        let same = |at: &usize| tiles[*at].place == place && !tiles[*at].hidden;
        (
            (0..index).rev().find(same),
            (index + 1..tiles.len()).find(same),
        )
    }
    /// Whether the chosen box can move one place left, and right, among the
    /// boxes in its place.
    pub fn can_move_tile(&self) -> (bool, bool) {
        let index = self.current();
        let tiles = self.tiles();
        let (before, after) = Self::neighbours(&tiles, index);
        (before.is_some(), after.is_some())
    }
    /// Moves the chosen box one place left or right among the boxes in its
    /// place; it stays chosen.
    pub fn move_tile(&self, earlier: bool) -> bool {
        let index = self.current();
        let Ok(mut tiles) = self.tiles.lock() else {
            return false;
        };
        let (before, after) = Self::neighbours(&tiles, index);
        let Some(to) = (if earlier { before } else { after }) else {
            return false;
        };
        tiles.swap(index, to);
        self.current.store(to, Ordering::SeqCst);
        true
    }
    /// A box dropped on the taskbar, as the hook reported it: which box, the
    /// place's word, and before (1) or after (2) the box `beside`, or alone
    /// (0), where it keeps its place in the list; the boxes numbered as the
    /// hook numbers them, the shown ones. Moved as the popup moves
    /// it. False for a drop not understood.
    pub fn apply_drop(&self, dropped: u32, place: u32, order: u32, beside: u32) -> bool {
        let Some(place) = BoxPlace::from_word(place) else {
            return false;
        };
        let Ok(mut tiles) = self.tiles.lock() else {
            return false;
        };
        // The hook numbers only the boxes shown.
        let from = Self::tile_of_slot(&tiles, dropped as usize);
        let to = if order > 0 {
            Self::tile_of_slot(&tiles, beside as usize)
        } else {
            from
        };
        let (Some(from), Some(to)) = (from, to) else {
            return false;
        };
        if order > 0 && to == from {
            return false;
        }
        let chosen = self.current.load(Ordering::SeqCst);
        let mut tile = tiles.remove(from);
        tile.place = place;
        let at = if order == 0 {
            from
        } else {
            let to = if to > from { to - 1 } else { to };
            if order == 1 { to } else { to + 1 }
        };
        let at = at.min(tiles.len());
        tiles.insert(at, tile);
        // The chosen box stays chosen wherever it went.
        let chosen = if chosen == from {
            at
        } else {
            let left = if chosen > from { chosen - 1 } else { chosen };
            if left >= at { left + 1 } else { left }
        };
        self.current.store(chosen, Ordering::SeqCst);
        true
    }
    /// Each box's name: the names of the services it shows, in its order,
    /// or "Box" with none; one already given gets a number.
    pub fn tile_names(&self, offers: &[Offer]) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for tile in self.tiles() {
            let shown: Vec<&str> = self
                .services_of(&tile, offers)
                .iter()
                .filter_map(|id| offers.iter().find(|o| o.id == *id))
                .map(|o| o.name.as_str())
                .collect();
            // Its own name first.
            let base = match (&tile.name, shown.is_empty()) {
                (Some(name), _) => name.clone(),
                (None, true) => "Box".to_string(),
                (None, false) => shown.join(" · "),
            };
            let mut name = base.clone();
            let mut count = 1;
            while names.contains(&name) {
                count += 1;
                name = format!("{base} {count}");
            }
            names.push(name);
        }
        names
    }

    /// Gives the box at `index` its own name: up to MAX_NAME characters, no
    /// control characters, spaces at the ends taken off; nothing left gives
    /// it back its name from what it shows. False with no box there.
    pub fn rename_tile(&self, index: usize, name: &str) -> bool {
        let Ok(mut tiles) = self.tiles.lock() else {
            return false;
        };
        let Some(tile) = tiles.get_mut(index) else {
            return false;
        };
        let name: String = name
            .chars()
            .filter(|c| !c.is_control())
            .collect::<String>()
            .trim()
            .chars()
            .take(MAX_NAME)
            .collect::<String>()
            .trim_end()
            .to_string();
        tile.name = (!name.is_empty()).then_some(name);
        true
    }

    pub fn look(&self) -> &'static Look {
        self.tile().look()
    }
    pub fn set_look(&self, look: &'static Look) {
        self.change(|tile| tile.set_look(look));
    }
    pub fn box_place(&self) -> BoxPlace {
        self.tile().place
    }
    /// Puts the chosen box in `place`, shown if it was hidden.
    pub fn set_box_place(&self, place: BoxPlace) {
        self.change(|tile| {
            tile.place = place;
            tile.hidden = false;
        });
    }
    /// Whether the chosen box is hidden, and hides it, its place kept.
    pub fn box_hidden(&self) -> bool {
        self.tile().hidden
    }
    pub fn hide_box(&self) {
        self.change(|tile| tile.hidden = true);
    }
    /// The boxes on the taskbar, in the list's order: the hook's boxes, as
    /// it numbers them from 0.
    pub fn shown_tiles(&self) -> Vec<Tile> {
        self.tiles().into_iter().filter(|t| !t.hidden).collect()
    }
    /// Where in the list the box the hook numbers `slot` is.
    fn tile_of_slot(tiles: &[Tile], slot: usize) -> Option<usize> {
        tiles
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.hidden)
            .nth(slot)
            .map(|(index, _)| index)
    }
    /// Chooses the box the hook numbers `slot`, right-clicked on the
    /// taskbar.
    pub fn select_shown(&self, slot: usize) -> bool {
        Self::tile_of_slot(&self.tiles(), slot).is_some_and(|index| self.select(index))
    }
    pub fn drag(&self) -> bool {
        self.drag.load(Ordering::SeqCst)
    }
    pub fn set_drag(&self, drag: bool) {
        self.drag.store(drag, Ordering::SeqCst);
    }
    /// Whether the tools are updated.
    pub fn updates(&self) -> bool {
        self.updates.load(Ordering::SeqCst)
    }
    pub fn set_updates(&self, updates: bool) {
        self.updates.store(updates, Ordering::SeqCst);
    }
    /// Whether values warn in color.
    pub fn warn(&self) -> bool {
        self.warn.load(Ordering::SeqCst)
    }
    pub fn set_warn(&self, warn: bool) {
        self.warn.store(warn, Ordering::SeqCst);
    }
    /// Which notifications are on.
    pub fn alerts(&self) -> crate::notify::Alerts {
        self.alerts.lock().map(|a| *a).unwrap_or_default()
    }
    pub fn set_alerts(&self, alerts: crate::notify::Alerts) {
        if let Ok(mut now) = self.alerts.lock() {
            *now = alerts;
        }
    }
    /// The settings window's language.
    pub fn language(&self) -> String {
        self.language.lock().map(|l| l.clone()).unwrap_or_default()
    }
    pub fn set_language(&self, language: &str) {
        if let Ok(mut now) = self.language.lock() {
            *now = language.to_string();
        }
    }
    /// The editor's shelves folded, by their words.
    pub fn folded(&self) -> Vec<String> {
        self.folded.lock().map(|f| f.clone()).unwrap_or_default()
    }
    pub fn set_folded(&self, folded: Vec<String>) {
        if let Ok(mut now) = self.folded.lock() {
            *now = folded;
        }
    }
    /// The services read: in any box on the taskbar, or in the popup's
    /// summary alone.
    pub fn read_services(&self) -> Vec<&'static str> {
        let offers = self.offers();
        let mut all = self.all_services(&offers);
        for id in self.summary(&offers) {
            if !all.contains(&id) {
                all.push(id);
            }
        }
        all
    }
    pub fn sample_boxes(&self) -> bool {
        self.sample_boxes.load(Ordering::SeqCst)
    }
    pub fn set_sample_boxes(&self, on: bool) {
        self.sample_boxes.store(on, Ordering::SeqCst);
    }
    pub fn fill(&self) -> Fill {
        Fill::ALL[self.fill.load(Ordering::SeqCst) as usize % Fill::ALL.len()]
    }
    pub fn set_fill(&self, fill: Fill) {
        let index = Fill::ALL.iter().position(|&f| f == fill).unwrap_or(0);
        self.fill.store(index as u32, Ordering::SeqCst);
    }
    pub fn background(&self) -> Background {
        self.tile().background
    }
    pub fn set_background(&self, background: Background) {
        self.change(|tile| tile.background = background);
    }
    /// The chosen box's own color, or none for the one every look starts
    /// with, and choosing it.
    pub fn color(&self) -> Option<u32> {
        self.tile().color
    }
    pub fn set_color(&self, color: Option<u32>) {
        self.change(|tile| tile.color = color);
    }
    /// The summary's own picks.
    pub fn summary_picks(&self) -> Picks {
        self.summary.lock().map(|p| p.clone()).unwrap_or_default()
    }
    pub fn set_summary_picks(&self, picks: Picks) {
        if let Ok(mut held) = self.summary.lock() {
            *held = picks;
        }
    }
    /// The custom boxes to pick from.
    pub fn boxes(&self) -> Vec<Arc<CustomBox>> {
        self.boxes.lock().map(|b| b.clone()).unwrap_or_default()
    }
    pub fn set_boxes(&self, boxes: Vec<CustomBox>) {
        if let Ok(mut held) = self.boxes.lock() {
            *held = boxes.into_iter().map(Arc::new).collect();
        }
    }
    pub fn choose(&self, name: Option<String>) {
        self.change(|tile| tile.custom = name);
    }
    /// Every box showing the custom box `was` shows `now` instead, or its
    /// look when that is none: the editor renamed or deleted it.
    pub fn rename_custom(&self, was: &str, now: Option<String>) {
        if let Ok(mut tiles) = self.tiles.lock() {
            for tile in tiles.iter_mut() {
                if tile.custom.as_deref() == Some(was) {
                    tile.custom = now.clone();
                }
            }
        }
    }
    pub fn popup_open(&self) -> bool {
        self.popup.load(Ordering::SeqCst)
    }
    pub fn set_popup_open(&self, open: bool) {
        self.popup.store(open, Ordering::SeqCst);
    }
    /// A box's custom box, when it is there; the presets otherwise.
    pub fn custom_of(&self, tile: &Tile) -> Option<Arc<CustomBox>> {
        let name = tile.custom.as_deref()?;
        self.boxes().into_iter().find(|custom| custom.name == name)
    }
    /// The custom boxes a box's style is picked from: the person's own, and a
    /// sample only while the chosen box shows it, so what is in use stays in
    /// the list; the samples are the editor's to start from.
    pub fn pickable_boxes(&self) -> Vec<Arc<CustomBox>> {
        let chosen = self.tile().custom;
        self.boxes()
            .into_iter()
            .filter(|b| b.file.is_some() || chosen.as_deref() == Some(b.name.as_str()))
            .collect()
    }
    /// The custom box chosen for the box chosen, when it is there.
    pub fn custom(&self) -> Option<Arc<CustomBox>> {
        self.custom_of(&self.tile())
    }
    /// The services a box shows out of those offered, in its order: a
    /// custom box's are those its blocks name.
    pub fn services_of(&self, tile: &Tile, offers: &[Offer]) -> Vec<&'static str> {
        if let Some(custom) = self.custom_of(tile) {
            return custom
                .services()
                .iter()
                .filter_map(|id| offers.iter().find(|o| o.id == *id).map(|o| o.id))
                .collect();
        }
        tile.picks.services(&ids(offers))
    }
    /// The services the box chosen shows now, as the popup checks them.
    pub fn services(&self, offers: &[Offer]) -> Vec<&'static str> {
        self.services_of(&self.tile(), offers)
    }
    /// The services any box on the taskbar shows, each once, in the boxes'
    /// order: those read, and those the tray icon counts. A hidden box's are
    /// not.
    pub fn all_services(&self, offers: &[Offer]) -> Vec<&'static str> {
        let mut all: Vec<&'static str> = Vec::new();
        for tile in self.shown_tiles() {
            for id in self.services_of(&tile, offers) {
                if !all.contains(&id) {
                    all.push(id);
                }
            }
        }
        all
    }
    /// A service's limits any box on the taskbar shows, each once: what is
    /// measured of PC.
    pub fn all_limits(&self, offer: &Offer) -> Vec<String> {
        let mut all: Vec<String> = Vec::new();
        for tile in self.shown_tiles() {
            let shows = self.services_of(&tile, std::slice::from_ref(offer));
            if !shows.contains(&offer.id) {
                continue;
            }
            let labels = match self.custom_of(&tile) {
                Some(custom) => custom.limits(offer.id, &offer.labels),
                None => tile.picks.limits(offer.id, &offer.labels),
            };
            for label in labels {
                if !all.contains(&label) {
                    all.push(label);
                }
            }
        }
        all
    }
    /// The offers as the popup and the settings window list them: those
    /// shown, in the box's order, then the rest in the provider's.
    pub fn listed<'a>(&self, offers: &'a [Offer]) -> Vec<&'a Offer> {
        let shown = self.services(offers);
        let first = shown
            .iter()
            .filter_map(|id| offers.iter().find(|o| o.id == *id));
        let rest = offers.iter().filter(|o| !shown.contains(&o.id));
        first.chain(rest).collect()
    }
    /// Moves a shown service one place earlier or later in the box; at
    /// either end nothing moves.
    pub fn move_service(&self, offers: &[Offer], id: &str, earlier: bool) {
        self.change(|tile| {
            let mut shown = tile.picks.services(&ids(offers));
            let Some(at) = shown.iter().position(|s| *s == id) else {
                return;
            };
            let to = if earlier {
                at.checked_sub(1)
            } else {
                Some(at + 1).filter(|&to| to < shown.len())
            };
            if let Some(to) = to {
                shown.swap(at, to);
                tile.picks
                    .set_services(shown.into_iter().map(String::from).collect());
            }
        });
    }
    /// Puts a shown service at place `to` among those shown, as dragging it
    /// in the popup does; past the end, it goes last.
    pub fn move_service_to(&self, offers: &[Offer], id: &str, to: usize) {
        self.change(|tile| {
            if let Some(shown) = moved(tile.picks.services(&ids(offers)), &id, to) {
                tile.picks
                    .set_services(shown.into_iter().map(String::from).collect());
            }
        });
    }
    /// Puts a shown limit at place `to` among its service's shown limits.
    pub fn move_limit_to(&self, offer: &Offer, label: &str, to: usize) {
        self.change(|tile| {
            let shown = tile.picks.limits(offer.id, &offer.labels);
            if let Some(shown) = moved(shown, &label.to_string(), to) {
                tile.picks.set_limits(offer.id, shown);
            }
        });
    }
    /// The services the summary shows, in its order: those picked for it
    /// that are offered, or, until any is, those the boxes show.
    pub fn summary(&self, offers: &[Offer]) -> Vec<&'static str> {
        let shown = self.summary_shown(offers);
        self.summary_picks().summary(&shown)
    }
    /// The summary's services in the offers' order, apart from the boxes':
    /// with none of those picked offered now, the boxes'.
    fn summary_shown(&self, offers: &[Offer]) -> Vec<&'static str> {
        let picked: Vec<&'static str> = match self.summary_picks().picked_summary_services() {
            Some(picked) => ids(offers)
                .into_iter()
                .filter(|id| picked.iter().any(|p| p.as_str() == *id))
                .collect(),
            None => Vec::new(),
        };
        if picked.is_empty() {
            self.all_services(offers)
        } else {
            picked
        }
    }
    /// Turns a service on or off in the summary, whether or not a box shows
    /// it: any offered one, and never the last one off.
    pub fn toggle_summary_service(&self, offers: &[Offer], id: &'static str) {
        let shown = self.summary(offers);
        let offered = ids(offers);
        if let Some(shown) = toggled(shown, &id, &offered, usize::MAX)
            && let Ok(mut picks) = self.summary.lock()
        {
            picks.set_summary_services(shown.into_iter().map(String::from).collect());
        }
    }
    /// A service's lines in the summary, by label.
    pub fn summary_limits(&self, offer: &Offer) -> Vec<String> {
        self.summary_picks().summary_limits(offer.id, &offer.labels)
    }
    /// Puts a service at place `to` in the summary; the boxes stay as they
    /// are.
    pub fn move_summary_to(&self, offers: &[Offer], id: &str, to: usize) {
        let shown = self.summary_shown(offers);
        let Ok(mut picks) = self.summary.lock() else {
            return;
        };
        let order = picks.summary(&shown);
        if let Some(order) = moved(order, &id, to) {
            picks.set_summary(order.into_iter().map(String::from).collect());
        }
    }
    /// Puts one of a service's lines at place `to` in the summary.
    pub fn move_summary_limit_to(&self, offer: &Offer, label: &str, to: usize) {
        let Ok(mut picks) = self.summary.lock() else {
            return;
        };
        let order = picks.summary_limits(offer.id, &offer.labels);
        if let Some(order) = moved(order, &label.to_string(), to) {
            picks.set_summary_limits(offer.id, order);
        }
    }
    /// The cells a box's picks take: one a service, or one for each two
    /// limits of one with more.
    fn cells(picks: &Picks, offers: &[Offer]) -> usize {
        picks
            .services(&ids(offers))
            .iter()
            .filter_map(|id| offers.iter().find(|o| o.id == *id))
            .map(|o| cell_count(picks.limits(o.id, &o.labels).len()))
            .sum()
    }
    /// Turns a service on or off in the box chosen: as many as the hook has
    /// cells, and never the last one off. One turned on goes last.
    pub fn toggle_service(&self, offers: &[Offer], id: &'static str) {
        self.change(|tile| {
            let offered = ids(offers);
            let shown = tile.picks.services(&offered);
            if let Some(shown) = toggled(shown, &id, &offered, MAX_CELLS) {
                let mut picks = tile.picks.clone();
                picks.set_services(shown.into_iter().map(String::from).collect());
                if Self::cells(&picks, offers) <= MAX_CELLS {
                    tile.picks = picks;
                }
            }
        });
    }
    /// The labels a service shows now in the box chosen, as the popup checks
    /// them: a custom box's are those its blocks name.
    pub fn shown(&self, offer: &Offer) -> Vec<String> {
        let tile = self.tile();
        if let Some(custom) = self.custom_of(&tile) {
            return custom.limits(offer.id, &offer.labels);
        }
        tile.picks.limits(offer.id, &offer.labels)
    }
    /// A service's labels as the popup and the settings window list them:
    /// those shown, in the box's order, then the rest in the provider's.
    pub fn listed_limits(&self, offer: &Offer) -> Vec<String> {
        let mut listed = self.shown(offer);
        for label in &offer.labels {
            if !listed.contains(label) {
                listed.push(label.clone());
            }
        }
        listed
    }
    /// Turns one of a service's limits on or off in the box chosen: two at
    /// most, PC's four, as long as the box's cells hold them, and never the
    /// last one off.
    pub fn toggle(&self, offer: &Offer, label: &str) {
        let offers = self.offers();
        self.change(|tile| {
            let shown = tile.picks.limits(offer.id, &offer.labels);
            if let Some(shown) = toggled(
                shown,
                &label.to_string(),
                &offer.labels,
                most_limits(offer.id),
            ) {
                let mut picks = tile.picks.clone();
                picks.set_limits(offer.id, shown);
                if Self::cells(&picks, &offers) <= MAX_CELLS {
                    tile.picks = picks;
                }
            }
        });
    }
    /// Leaves the services' limits for the popup to offer.
    pub fn offer(&self, services: &[Service]) {
        let offers = services
            .iter()
            .map(|s| Offer {
                id: s.id,
                name: s.name.clone(),
                labels: s.limits.iter().flatten().map(|l| l.label.clone()).collect(),
            })
            .collect();
        if let Ok(mut last) = self.offers.lock() {
            *last = offers;
        }
        if let Ok(mut read) = self.read.lock() {
            *read = services.to_vec();
        }
    }
    pub fn offers(&self) -> Vec<Offer> {
        self.offers.lock().map(|o| o.clone()).unwrap_or_default()
    }
    /// The services as last offered, with their values and when each limit
    /// starts over.
    pub fn read(&self) -> Vec<Service> {
        self.read.lock().map(|r| r.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
#[test]
fn boxes_are_a_list_with_one_chosen() {
    // Places by name and by the hook's word; one box at least and four at
    // most, a new one like the first beside the tray and chosen, the last
    // never taken away; moving among the boxes in one place; a drop beside
    // a box; names from what each shows.
    for place in BoxPlace::ALL {
        assert_eq!(BoxPlace::find(place.name()), Some(place));
        assert_eq!(BoxPlace::from_word(place.word()), Some(place));
    }
    assert_eq!(BoxPlace::ALL.map(BoxPlace::word), [1, 4, 5, 2]);
    assert_eq!(BoxPlace::find("right-before"), None);
    let choice = Choice::new(&LOOKS[2], Fill::Used, Background::Cards);
    choice.offer(&crate::usage::mock(0));
    let offers = choice.offers();
    assert_eq!((choice.tiles().len(), choice.current()), (1, 0));
    assert!(!choice.remove_tile());
    choice.set_box_place(BoxPlace::Left);
    assert!(choice.add_tile());
    assert_eq!(choice.current(), 1);
    let added = choice.tile();
    assert_eq!(
        (added.look, added.background, added.place),
        (2, Background::Cards, BoxPlace::Tray)
    );
    for id in ["gpt", "claude", "gemini"] {
        choice.toggle_service(&offers, id);
    }
    choice.toggle_service(&offers, "pc");
    assert_eq!(choice.services(&offers), ["gemini", "pc"]);
    assert!(choice.add_tile() && choice.add_tile());
    assert!(!choice.add_tile());
    assert_eq!(
        choice.tile_names(&offers),
        [
            "GPT · Claude · Gemini",
            "Gemini · PC",
            "GPT · Claude · Gemini 2",
            "GPT · Claude · Gemini 3"
        ]
    );
    // Beside the tray are the second, third and fourth; the fourth moves
    // left past the third, and no further than the second.
    assert_eq!(choice.can_move_tile(), (true, false));
    assert!(choice.move_tile(true));
    assert_eq!(choice.current(), 2);
    assert!(choice.move_tile(true) && !choice.move_tile(true));
    assert_eq!(choice.current(), 1);
    // A drop: the first box after the last, beside the tray.
    assert!(choice.select(0));
    assert!(choice.apply_drop(0, 2, 2, 3));
    assert_eq!(
        choice.tiles().iter().map(|t| t.place).collect::<Vec<_>>(),
        [BoxPlace::Tray; 4]
    );
    assert_eq!(choice.current(), 3);
    assert!(!choice.apply_drop(9, 1, 0, 0) && !choice.apply_drop(0, 3, 0, 0));
    assert!(!choice.apply_drop(0, 1, 1, 0));
    assert!(choice.remove_tile());
    assert_eq!((choice.tiles().len(), choice.current()), (3, 2));
    assert!(!choice.select(3));
}

fn ids(offers: &[Offer]) -> Vec<&'static str> {
    offers.iter().map(|o| o.id).collect()
}

/// `list` with `item` taken out and put at place `to`, or last past the end.
/// None when it is not in the list or would not move.
fn moved<T: PartialEq>(mut list: Vec<T>, item: &T, to: usize) -> Option<Vec<T>> {
    let at = list.iter().position(|i| i == item)?;
    let to = to.min(list.len() - 1);
    if at == to {
        return None;
    }
    let item = list.remove(at);
    list.insert(to, item);
    Some(list)
}

/// `shown` with `item` turned on or off: at most `most` on, only an offered
/// one, and never the last one off. None when that changes nothing.
fn toggled<T: Clone + PartialEq>(
    mut shown: Vec<T>,
    item: &T,
    offered: &[T],
    most: usize,
) -> Option<Vec<T>> {
    match shown.iter().position(|s| s == item) {
        Some(at) if shown.len() > 1 => {
            shown.remove(at);
        }
        None if shown.len() < most && offered.contains(item) => shown.push(item.clone()),
        _ => return None,
    }
    Some(shown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::mock;

    #[test]
    fn a_hidden_box_keeps_its_place_and_leaves_the_taskbar() {
        // Hidden, a box keeps its place and settings but is not one of the
        // hook's boxes, which are numbered among the shown ones; its
        // services are read only if another box or the summary has them;
        // it has no neighbours to move among; a place shows it again.
        let choice = Choice::new(&LOOKS[0], Fill::Used, Background::Box);
        choice.offer(&mock(0));
        let offers = choice.offers();
        choice.set_box_place(BoxPlace::Left);
        choice.change(|tile| tile.picks.set_services(vec!["pc".into()]));
        for _ in 0..2 {
            assert!(choice.add_tile());
            choice.change(|tile| tile.picks.set_services(vec!["gpt".into()]));
        }
        assert_eq!(choice.current(), 2);
        assert!(choice.select(0));
        choice.hide_box();
        assert!(choice.box_hidden());
        assert_eq!(choice.box_place(), BoxPlace::Left);
        assert_eq!(choice.shown_tiles().len(), 2);
        assert!(!choice.all_services(&offers).contains(&"pc"));
        assert_eq!(choice.can_move_tile(), (false, false));
        // The hook's first box is the list's second: a right click on it
        // chooses that one, and a drop of it beside the other moves it.
        assert!(choice.select_shown(0));
        assert_eq!(choice.current(), 1);
        assert!(!choice.select_shown(2));
        assert!(choice.apply_drop(1, BoxPlace::Tray.word(), 1, 0));
        let tiles = choice.tiles();
        assert!(tiles[0].hidden);
        assert_eq!(tiles.iter().filter(|t| !t.hidden).count(), 2);
        // A place shows it again where it was put.
        assert!(choice.select(0));
        choice.set_box_place(BoxPlace::BeforeStart);
        assert!(!choice.box_hidden());
        assert!(choice.all_services(&offers).contains(&"pc"));
        assert_eq!(choice.shown_tiles().len(), 3);
    }

    #[test]
    fn a_choice_holds_what_was_picked_last() {
        // The popup changes the look, the fill mode and the background
        // while the controller runs.
        let choice = Choice::new(style::DEFAULT, Fill::Left, Background::Cards);
        assert_eq!(choice.look().name, "slim");
        assert_eq!(
            (choice.fill(), choice.background()),
            (Fill::Left, Background::Cards)
        );
        choice.set_look(&LOOKS[3]);
        choice.set_fill(Fill::Used);
        choice.set_background(Background::Box);
        assert_eq!(choice.look().name, "pie");
        assert_eq!(
            (choice.fill(), choice.background()),
            (Fill::Used, Background::Box)
        );
    }

    #[test]
    fn two_limits_at_most_and_never_none() {
        // Claude's week and five hours show; its Opus week waits until one of
        // them is turned off, and the last one cannot be.
        let choice = Choice::new(style::DEFAULT, Fill::Used, Background::Box);
        choice.offer(&mock(0));
        let claude = choice.offers()[1].clone();
        assert_eq!(claude.labels, ["5h", "7d", "Opus 7d"]);
        assert_eq!(choice.shown(&claude), ["5h", "7d"]);
        for (label, shown) in [
            ("Opus 7d", vec!["5h", "7d"]),
            ("5h", vec!["7d"]),
            ("Opus 7d", vec!["7d", "Opus 7d"]),
            ("7d", vec!["Opus 7d"]),
            ("Opus 7d", vec!["Opus 7d"]),
            ("Sonnet 7d", vec!["Opus 7d"]),
        ] {
            choice.toggle(&claude, label);
            assert_eq!(choice.shown(&claude), shown, "after {label}");
        }
        // A service not read yet offers nothing.
        choice.offer(&mock(2));
        assert!(choice.offers()[2].labels.is_empty());
    }

    #[test]
    fn four_services_at_most_and_never_none() {
        // Three show until more are turned on, four at most, one turned on
        // goes last, and the last one cannot be turned off.
        let choice = Choice::new(style::DEFAULT, Fill::Used, Background::Box);
        choice.offer(&mock(0));
        let offers = choice.offers();
        assert_eq!(choice.services(&offers), ["gpt", "claude", "gemini"]);
        for (id, shown) in [
            ("grok", vec!["gpt", "claude", "gemini", "grok"]),
            ("pc", vec!["gpt", "claude", "gemini", "grok"]),
            ("gpt", vec!["claude", "gemini", "grok"]),
            ("pc", vec!["claude", "gemini", "grok", "pc"]),
            ("copilot", vec!["claude", "gemini", "grok", "pc"]),
        ] {
            choice.toggle_service(&offers, id);
            assert_eq!(choice.services(&offers), shown, "after {id}");
        }
        for id in ["claude", "gemini", "pc", "grok"] {
            choice.toggle_service(&offers, id);
        }
        assert_eq!(choice.services(&offers), ["grok"]);
    }

    #[test]
    fn what_shows_moves_to_the_place_it_is_dropped() {
        // A shown service or limit goes to the place dropped, among those
        // shown; past the end it goes last, and one not shown does not move.
        // The popup lists shown limits first, in the box's order.
        let choice = Choice::new(style::DEFAULT, Fill::Used, Background::Box);
        choice.offer(&mock(0));
        let offers = choice.offers();
        for (id, to, shown) in [
            ("gemini", 0, vec!["gemini", "gpt", "claude"]),
            ("gemini", 9, vec!["gpt", "claude", "gemini"]),
            ("claude", 1, vec!["gpt", "claude", "gemini"]),
            ("grok", 0, vec!["gpt", "claude", "gemini"]),
        ] {
            choice.move_service_to(&offers, id, to);
            assert_eq!(choice.services(&offers), shown, "{id} {to}");
        }
        let claude = offers[1].clone();
        choice.move_limit_to(&claude, "7d", 0);
        assert_eq!(choice.shown(&claude), ["7d", "5h"]);
        choice.move_limit_to(&claude, "Opus 7d", 0);
        assert_eq!(choice.shown(&claude), ["7d", "5h"]);
        assert_eq!(choice.listed_limits(&claude), ["7d", "5h", "Opus 7d"]);
        // One turned on goes last, as a service does.
        choice.toggle(&claude, "5h");
        choice.toggle(&claude, "Opus 7d");
        assert_eq!(choice.listed_limits(&claude), ["7d", "Opus 7d", "5h"]);
        // The summary starts in the box's order, with each service's lines
        // in the provider's.
        assert_eq!(choice.summary(&offers), ["gpt", "claude", "gemini"]);
        assert_eq!(choice.summary_limits(&claude), ["5h", "7d", "Opus 7d"]);
        choice.move_summary_to(&offers, "gpt", 2);
        choice.move_summary_limit_to(&claude, "Opus 7d", 0);
        assert_eq!(choice.summary(&offers), ["claude", "gemini", "gpt"]);
        assert_eq!(choice.summary_limits(&claude), ["Opus 7d", "5h", "7d"]);
        assert_eq!(choice.services(&offers), ["gpt", "claude", "gemini"]);
    }

    #[test]
    fn the_summary_picks_its_services_apart_from_the_box() {
        // Until one is picked, the summary shows the box's services; then
        // any offered one can be on, in the box or not, never none, and the
        // box stays as it is.
        let choice = Choice::new(style::DEFAULT, Fill::Used, Background::Box);
        choice.offer(&mock(0));
        let offers = choice.offers();
        assert_eq!(choice.summary(&offers), ["gpt", "claude", "gemini"]);
        choice.toggle_summary_service(&offers, "gemini");
        choice.toggle_summary_service(&offers, "grok");
        assert_eq!(choice.summary(&offers), ["gpt", "claude", "grok"]);
        assert_eq!(choice.services(&offers), ["gpt", "claude", "gemini"]);
        // Its order still moves by the summary's own lines.
        choice.move_summary_to(&offers, "grok", 0);
        assert_eq!(choice.summary(&offers), ["grok", "gpt", "claude"]);
        for id in ["grok", "gpt"] {
            choice.toggle_summary_service(&offers, id);
        }
        assert_eq!(choice.summary(&offers), ["claude"]);
        choice.toggle_summary_service(&offers, "claude");
        assert_eq!(choice.summary(&offers), ["claude"]);
        // Picked ones no longer offered: the box's again.
        let mut picks = choice.summary_picks();
        picks.set_summary_services(vec!["copilot".into()]);
        choice.set_summary_picks(picks);
        assert_eq!(choice.summary(&offers), ["gpt", "claude", "gemini"]);
        // A custom box's services, until any is picked, in the summary's
        // own order.
        let mut picks = choice.summary_picks();
        picks.set_summary_services(Vec::new());
        choice.set_summary_picks(picks);
        choice.choose(Some("Columns sample".into()));
        assert_eq!(choice.services(&offers), ["claude", "gpt"]);
        assert_eq!(choice.summary(&offers), ["gpt", "claude"]);
    }

    #[test]
    fn a_shown_service_moves_a_place_at_a_time() {
        // Up and down swap it with its neighbour; at either end, or for a
        // service not shown, nothing moves. The popup and the window list the
        // shown ones first, in the box's order.
        let choice = Choice::new(style::DEFAULT, Fill::Used, Background::Box);
        choice.offer(&mock(0));
        let offers = choice.offers();
        for (id, earlier, shown) in [
            ("gemini", true, vec!["gpt", "gemini", "claude"]),
            ("gemini", true, vec!["gemini", "gpt", "claude"]),
            ("gemini", true, vec!["gemini", "gpt", "claude"]),
            ("claude", false, vec!["gemini", "gpt", "claude"]),
            ("gpt", false, vec!["gemini", "claude", "gpt"]),
            ("grok", true, vec!["gemini", "claude", "gpt"]),
        ] {
            choice.move_service(&offers, id, earlier);
            assert_eq!(choice.services(&offers), shown, "{id} {earlier}");
        }
        let listed: Vec<_> = choice.listed(&offers).iter().map(|o| o.id).collect();
        assert_eq!(listed, ["gemini", "claude", "gpt", "grok", "pc"]);
    }
}
