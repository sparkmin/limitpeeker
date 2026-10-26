//! The shared-memory contract between the controller and the hook. Both sides
//! spell it out separately, so these tests read the hook's source and fail when
//! the two drift apart.

#[cfg(test)]
mod tests {
    use crate::icons::MAX_PX;
    use crate::layout::DisplayMode;
    use crate::style::{DEFAULT, RANGES};
    use crate::usage::{
        CAPACITY, CELL_BREAK, GAUGE_BREAK, LABEL_BREAK, MARK, MAX_CELL_GAUGES, MAX_CELLS,
        MAX_GAUGES,
    };

    /// The controller's side of the contract, in the xaml module's files.
    const CONTROLLER: &str = concat!(
        include_str!("xaml/mod.rs"),
        include_str!("xaml/channel.rs"),
        include_str!("xaml/attach.rs"),
        include_str!("xaml/worker.rs")
    );

    #[test]
    fn native_names_and_words_match_the_controller() {
        let native = include_str!("../native/taskbar_tap.cpp");
        let controller = CONTROLLER;
        // One label, requested as DisplayMode::Full, in the words the
        // controller reads. The ladder is gone.
        assert!(native.contains("constexpr LONG Full = 4;"));
        assert_eq!(DisplayMode::Full as u32, 4);
        assert!(native.contains("constexpr size_t RequestWord=5, ShownWord=22, FullWidth=43;"));
        assert!(controller.contains("const REQUEST: usize = 5;"));
        assert!(controller.contains("const SHOWN: usize = 22;"));
        assert!(controller.contains("const FULL_WIDTH: usize = 43;"));
        // Hide reasons, measurement failures, attach stages and reservation
        // outcomes are logged by name; each table must end where the enum does.
        assert!(native.contains(
            "enum Hide : LONG { Shown=0, NoMeasure=1, NotRequested=2, NoReservation=3, Settling=4, NoText=5, };"
        ));
        assert!(native.contains(
            "BadScale=5, RootSizeMismatch=6, WalkFailed=7, AnchorsMissing=8, NoRects=9,"
        ));
        assert!(native.contains("Advised=5, FrameSeen=6, Dispatched=7, Started=8,"));
        assert!(native.contains("StructureChanged=4, RestoreMismatch=5,"));
        assert!(
            native.contains("constexpr size_t ReserveState=45, ReserveWidth=46, SettlingTicks=47;")
        );
        // Both sides map the same number of bytes: the session's words, then
        // the four boxes' words, then each box's place and what became of
        // it, the drops, and the Widgets button.
        assert!(native.contains("constexpr DWORD SharedBytes=70102*4;"));
        assert!(controller.contains("const WORDS: usize = 70102;"));
        assert!(native.contains("constexpr size_t BoxFirst=48, BoxWords=17505;"));
        assert!(native.contains("constexpr size_t GaugeEnds=17541;"));
        assert!(controller.contains("const BOX_WORDS: usize = 17505;"));
        assert!(controller.contains("const GAUGE_ENDS: usize = 17541;"));
        assert_eq!(17541 + MAX_GAUGES, 48 + 17505);
        assert!(native.contains("constexpr size_t MaxBoxes=4;"));
        assert!(controller.contains("const MAX_BOXES: usize = 4;"));
        assert_eq!(48 + 4 * 17505, 70068);
        assert_eq!(70068 + 5 * 4 + 10, 70098);
        // Subscribe attempts pack as the controller decodes them.
        assert!(native.contains("constexpr size_t AdviseTries=31;"));
        assert!(
            native.contains("InterlockedExchange(state->words+AdviseTries,attempt|(cleaned<<8));")
        );
        // The attach a hook must be loaded by, how often SetSite reached the
        // session, and why it turned it down, by name as the enum runs; the
        // session is taken by one compare-exchange.
        assert!(native.contains("constexpr size_t AttachToken=7, SiteCalls=8, SiteRefusal=9;"));
        assert!(controller.contains("const ATTACH_TOKEN: usize = 7;"));
        assert!(controller.contains("const SITE_CALLS: usize = 8;"));
        assert!(controller.contains("const SITE_REFUSAL: usize = 9;"));
        assert!(native.contains(
            "enum Refusal : LONG { NotRefused=0, SiteAgain=1, Elevated=2, NotTaskbar=3, LeaseGone=4, OtherAttach=5, Taken=6, SetupFailed=7, };"
        ));
        assert!(native.contains("InterlockedCompareExchange(state->words+StatusWord,1,0)!=0"));
        assert!(controller.contains("PCWSTR(data.as_ptr()),"));
    }

    #[test]
    fn the_label_comes_from_the_controller() {
        // The hook holds no label and knows no limit or warning mark, so a new
        // value, format or provider never needs a new DLL and an Explorer
        // restart.
        let native = include_str!("../native/taskbar_tap.cpp");
        let controller = CONTROLLER;
        for mock in ["GPT", "72%", "Claude", "Gemini", "5h", "7d", "Opus", MARK] {
            assert!(!native.contains(mock), "{mock} is hardcoded in the hook");
        }
        // Where the words sit, and how long each string may be, on both sides.
        assert!(native.contains(
            "constexpr size_t TextSeq=48, TextLength=49, SizingLength=50, TextApplied=51, TextClipped=52, TextDrawn=53;"
        ));
        assert!(native.contains(
            "constexpr size_t GaugeFills=83, GaugeTextColors=95, GaugeFillColors=107, TextChars=119, SizingChars=215;"
        ));
        for (name, word) in [
            ("TEXT_SEQ", 48),
            ("TEXT_LENGTH", 49),
            ("SIZING_LENGTH", 50),
            ("TEXT_APPLIED", 51),
            ("TEXT_CLIPPED", 52),
            ("TEXT_DRAWN", 53),
            ("GAUGE_FILLS", 83),
            ("GAUGE_TEXT_COLORS", 95),
            ("GAUGE_FILL_COLORS", 107),
            ("TEXT_CHARS", 119),
            ("SIZING_CHARS", 215),
        ] {
            assert!(controller.contains(&format!("const {name}: usize = {word};")));
        }
        assert!(native.contains(&format!(
            "constexpr LONG TextCapacity={CAPACITY}, MaxCells={MAX_CELLS}, MaxGauges={MAX_GAUGES}, MaxCellGauges={MAX_CELL_GAUGES};"
        )));
        assert!(native.contains(&format!(
            "constexpr wchar_t CellBreak={:#x}, GaugeBreak={:#x}, LabelBreak={:#x};",
            u32::from(CELL_BREAK),
            u32::from(GAUGE_BREAK),
            u32::from(LABEL_BREAK)
        )));
        // A fill and two colors per gauge, then each string takes CAPACITY
        // UTF-16 units, two a word; the icons follow.
        assert_eq!(95 - 83, MAX_GAUGES);
        assert_eq!(107 - 95, MAX_GAUGES);
        assert_eq!(119 - 107, MAX_GAUGES);
        assert_eq!(215 - 119, CAPACITY / 2);
        assert_eq!(311 - 215, CAPACITY / 2);
        // A cell may hold more than its share of the twelve; a label that
        // holds more in all is refused before it is sent.
        assert!(MAX_CELL_GAUGES <= MAX_GAUGES);
    }

    #[test]
    fn the_look_comes_from_the_controller() {
        // The hook holds no look of its own, so restyling never needs a new
        // DLL and an Explorer restart.
        let native = include_str!("../native/taskbar_tap.cpp");
        let controller = CONTROLLER;
        assert!(!native.contains("FontSize(13)"));
        assert!(!native.contains("Color{255,"));
        // Box and track colors, then the sizes, cards, icons, where colors and
        // values sit, how gauges are arranged, and how heavy a mark is, in
        // `words()` order, right up to the gauges' words.
        for line in [
            "constexpr size_t StyleBox=54, StyleTrack=55, StyleFont=56, StyleWeight=57, StyleHeight=58, StylePadding=59, StyleGap=60, StyleGauge=61, StyleGaugeGap=62, StyleRadius=63;",
            "constexpr size_t StyleLayout=64, StyleGaugeWidth=65, StyleCellColor=66, StyleCellBorder=67, StyleCellRadius=68, StyleCellPadding=69, StyleCellInset=70, StyleMotion=71;",
            "constexpr size_t StyleIcon=72, StyleIconGap=73, StyleUniform=74;",
            "constexpr size_t StyleFillEnd=75, StyleTextEnd=76, StyleArrange=77, StyleSpacing=78, StyleLabelGap=79, StyleRing=80, StyleLabelColor=81;",
            "constexpr size_t StyleMarkWeight=82;",
        ] {
            assert!(native.contains(line), "{line}");
        }
        assert!(controller.contains("const STYLE_BOX: usize = 54;"));
        assert_eq!(54 + DEFAULT.style.words().len(), 83);
        // Every text is centered by its capitals.
        assert!(native.contains("text.TextLineBounds(x::TextLineBounds::Tight);"));
        // The hook refuses what the controller would never send.
        for (name, least, most) in RANGES {
            assert!(
                native.contains(&format!(
                    "constexpr LONG {name}Min={least}, {name}Max={most};"
                )),
                "{name} range differs"
            );
        }
    }

    #[test]
    fn icons_arrive_as_pixels_the_controller_drew() {
        // The hook never opens or parses an image; it copies pixels drawn
        // outside Explorer at the size shown.
        let native = include_str!("../native/taskbar_tap.cpp");
        let controller = CONTROLLER;
        for parser in [
            "BitmapImage",
            "SvgImageSource",
            "UriSource",
            "SetSourceAsync",
        ] {
            assert!(!native.contains(parser), "{parser} in the hook");
        }
        assert!(native.contains(
            "constexpr size_t IconSeq=311, IconTaken=312, IconSizes=313, IconPixels=321, IconBlock=4096;"
        ));
        for (name, word) in [
            ("ICON_SEQ", 311),
            ("ICON_TAKEN", 312),
            ("ICON_SIZES", 313),
            ("ICON_PIXELS", 321),
            ("ICON_BLOCK", 4096),
        ] {
            assert!(controller.contains(&format!("const {name}: usize = {word};")));
        }
        assert!(native.contains(&format!("constexpr LONG IconMaxPx={MAX_PX};")));
        assert_eq!(MAX_PX * MAX_PX, 4096);
        assert!(313 + MAX_CELLS <= 321);
        assert_eq!(321 + MAX_CELLS * 4096, 16705);
    }

    #[test]
    fn a_scene_is_read_where_the_controller_writes_it() {
        // After the icons, under the label's sequence: the mode, the width,
        // the count and the pool's length, twelve words a block, then the
        // pool; the same kinds and ranges on both sides.
        use crate::scene::*;
        let native = include_str!("../native/taskbar_tap.cpp");
        let controller = CONTROLLER;
        assert!(native.contains(&format!(
            "constexpr size_t SceneMode={SCENE_MODE}, SceneWidth={SCENE_WIDTH}, SceneCount={SCENE_COUNT}, ScenePoolLength={SCENE_POOL_LENGTH}, SceneItems={SCENE_ITEMS}, SceneItemWords={ITEM_WORDS}, ScenePool={SCENE_POOL};"
        )));
        assert!(native.contains(&format!(
            "constexpr LONG MaxItems={MAX_ITEMS}, ScenePoolCapacity={POOL_CAPACITY}, SceneWidthMin={WIDTH_MIN}, SceneWidthMax={WIDTH_MAX};"
        )));
        assert!(native.contains(&format!(
            "constexpr LONG ThicknessMin={THICKNESS_MIN}, ThicknessMax={THICKNESS_MAX};"
        )));
        // A block's text is as long on both sides.
        assert!(native.contains(&format!("constexpr LONG ItemTextMax={TEXT_MAX};")));
        assert!(native.contains("static_cast<LONG>(item.text.size())<=ItemTextMax"));
        assert!(native.contains(&format!(
            "enum Kind : LONG {{ TextKind={}, BarKind={}, RingKind={}, PieKind={}, IconKind={}, }};",
            Kind::Text as u32,
            Kind::Bar as u32,
            Kind::Ring as u32,
            Kind::Pie as u32,
            Kind::Icon as u32
        )));
        assert_eq!(SCENE_MODE, 321 + MAX_CELLS * 4096);
        assert_eq!(SCENE_POOL + POOL_CAPACITY / 2, 17541);
        // The scene goes under the label's sequence, so the hook takes one
        // or the other, whole.
        let sent = &controller[controller.find("fn show_scene(").unwrap()..];
        let sent = &sent[..sent.find("sent.scene = Some").unwrap()];
        assert!(sent.contains("self.set(base + TEXT_SEQ, seq.wrapping_add(1));"));
        assert!(sent.contains("self.set(base + SCENE_MODE, 1);"));
        assert!(sent.contains("self.set(base + TEXT_SEQ, seq.wrapping_add(2));"));
        assert!(controller.contains("self.set(base + SCENE_MODE, 0);"));
        // The hook refuses a block out of the box or its kind's range.
        assert!(native.contains(
            "for(auto const& item:next.items) if(!fits(item,width,next.style.height)) return;"
        ));
    }

    #[test]
    fn a_right_click_on_the_box_asks_the_controller_for_its_menu() {
        // The hook posts the tray menu's own message, only to a window of the
        // controller's process, and counts it, in two words nothing else used.
        let native = include_str!("../native/taskbar_tap.cpp");
        let controller = CONTROLLER;
        let tray = include_str!("windows_app/mod.rs");
        assert!(native.contains("constexpr size_t MenuWindow=3, MenuRequests=6;"));
        assert!(controller.contains("const MENU_WINDOW: usize = 3;"));
        assert!(controller.contains("const MENU_REQUESTS: usize = 6;"));
        assert!(native.contains("constexpr UINT OpenMenu=WM_APP+4;"));
        assert!(tray.contains("const OPEN_MENU: u32 = WM_APP + 4;"));
        assert!(native.contains(
            "if(!window || pid==0 || !GetWindowThreadProcessId(window,&owner) || owner!=pid) return;"
        ));
        for word in ["3", "6"] {
            assert!(!native.contains(&format!("words+{word})")));
            assert!(!native.contains(&format!("read({word})")));
            assert!(!controller.contains(&format!(".set({word},")));
            assert!(!controller.contains(&format!(".get({word})")));
        }
    }

    #[test]
    fn the_sessions_own_words_have_one_name_each_side() {
        // Named on both sides and no longer written as bare numbers; the
        // lease is as long on both.
        let native = include_str!("../native/taskbar_tap.cpp");
        let controller = CONTROLLER;
        assert!(native.contains(
            "constexpr size_t ControllerWord=2, ObservedWord=4, StatusWord=15, ErrorWord=16, LayoutRevisionWord=17, VisibilityWord=18;"
        ));
        assert!(native.contains(
            "constexpr size_t HeartbeatWord=23, TicksWord=24, TickMicrosWord=25, WalkMicrosWord=26, VisitedWord=27;"
        ));
        for (name, word) in [
            ("CONTROLLER_PID", 2),
            ("OBSERVED", 4),
            ("HOOK_STATUS", 15),
            ("HOOK_ERROR_CODE", 16),
            ("LAYOUT_REVISION", 17),
            ("VISIBILITY_CHANGES", 18),
            ("HEARTBEAT", 23),
            ("TICKS", 24),
            ("TICK_US", 25),
            ("WALK_US", 26),
            ("VISITED", 27),
        ] {
            assert!(
                controller.contains(&format!("const {name}: usize = {word};")),
                "{name}"
            );
        }
        for word in [2, 4, 5, 15, 16, 17, 18, 22, 23, 24, 25, 26, 27] {
            for raw in [
                format!("read({word})"),
                format!("words+{word})"),
                format!("words+{word},"),
                format!("add({word},"),
            ] {
                assert!(!native.contains(&raw), "{raw} in the hook");
            }
            for raw in [
                format!(".set({word},"),
                format!(".get({word})"),
                format!("word({word})"),
            ] {
                assert!(!controller.contains(&raw), "{raw} in the controller");
            }
        }
        assert!(native.contains("constexpr DWORD LeaseMs=1500;"));
        assert!(controller.contains("const LEASE_MS: u64 = 1500;"));
        assert!(native.contains("// The constant lines below are read as text by the Rust tests"));
    }

    #[test]
    fn every_box_has_words_of_its_own_and_only_room_is_made() {
        // Each box its own copy of the box's words, read by the same code
        // from where they start, and its own words for where it goes and
        // what became of it; at the left and after the apps the taskbar
        // gets room, never a child.
        use crate::choice::BoxPlace;
        let native = include_str!("../native/taskbar_tap.cpp");
        assert!(native.contains(
            "constexpr size_t LeftState=11, LeftWidth=12, LeftTicks=13, LeftItemsStart=14;"
        ));
        assert!(native.contains(
            "constexpr size_t BoxPlaces=70068, BoxRanks=70072, BoxShown=70076, BoxHide=70080, BoxWidth=70084;"
        ));
        for (name, word) in [
            ("BOX_PLACES", 70068),
            ("BOX_RANKS", 70072),
            ("BOX_SHOWN", 70076),
            ("BOX_HIDE", 70080),
            ("BOX_WIDTH", 70084),
            ("LEFT_STATE", 11),
            ("LEFT_WIDTH", 12),
            ("LEFT_TICKS", 13),
            ("LEFT_ITEMS_START", 14),
        ] {
            assert!(
                CONTROLLER.contains(&format!("const {name}: usize = {word};")),
                "{name}"
            );
        }
        assert!(native.contains(&format!(
            "enum Place : LONG {{ Nowhere=0, LeftEnd={}, Tray={}, BeforeStart={}, AfterApps={}, }};",
            BoxPlace::Left.word(),
            BoxPlace::Tray.word(),
            BoxPlace::BeforeStart.word(),
            BoxPlace::AfterApps.word()
        )));
        assert!(native.contains(
            "enum Left : LONG { LeftNone=0, LeftReserved=1, LeftRefused=2, LeftRestored=3, LeftAbandoned=4, LeftMismatch=5, };"
        ));
        assert!(native.contains("other->base=BoxWords*at;"));
        assert!(native.contains("other->width_word=BoxWidth+at;"));
        // A right click asks for the menu on its own box.
        assert!(native.contains("PostMessageW(window,OpenMenu,box_index+1,0)"));
        assert!(!native.contains("state->read(TextSeq)"));
        // The room at the left is the leftmost item's left margin, so the
        // repeater stays at the screen's left; after the apps it is the
        // repeater's right margin. A new leftmost item takes the room from
        // the one before, though not while the one that has it is out of
        // the layout for a moment; nothing is added.
        assert!(native.contains("right_owned=Owned::of(repeater);"));
        assert!(native.contains("right_owned.put(0,right);"));
        assert!(native.contains("if(left_owned.element) left_owned.put(left,0);"));
        assert!(
            native
                .contains("const bool moved=items.leftmost && items.leftmost!=left_owned.element")
        );
        assert!(native.contains("items.owned_seen || seen_now-left_seen>=WidgetsHoldMs"));
        assert!(native.contains("const double slot=point.X-(own?owned_dip:0);"));
        assert!(native.contains("const double strip=content.X;"));
        assert!(!native.contains("left_repeater.Margin("));
        assert!(!native.contains("left_repeater.Children()"));
        assert!(!native.contains("L\"RootGrid\""));
    }

    #[test]
    fn the_widgets_button_is_apart_from_the_apps() {
        // The Widgets button sits in the taskbar's repeater apart from the
        // apps, seen inside its padding. The place before the Start button
        // is always just before it, in the Start button's margin; centered,
        // the left end is just after the Widgets button as it is seen now. The
        // place after the last app goes beside the apps with no margin of its
        // own while the button beyond them leaves room, else beyond it in its
        // margin. The button's ends are reported for the log.
        let native = include_str!("../native/taskbar_tap.cpp");
        assert!(native.contains(
            "constexpr size_t WidgetsLeft=70098, WidgetsRight=70099, SeenLeft=70100, SeenRight=70101;"
        ));
        for (name, word) in [
            ("WIDGETS_LEFT", 70098),
            ("WIDGETS_RIGHT", 70099),
            ("SEEN_LEFT", 70100),
            ("SEEN_RIGHT", 70101),
        ] {
            assert!(
                CONTROLLER.contains(&format!("const {name}: usize = {word};")),
                "{name}"
            );
        }
        assert!(native.contains("GetAutomationId(element)==L\"WidgetsButton\""));
        assert!(native.contains("seat(starts,any?before_start(around,width)"));
        assert!(native.contains("std::max(before_start(items,width),strip+BoxGap)"));
        assert!(native.contains("return items.apps.first-BoxGap-width;"));
        assert!(native.contains("if(!ends.empty()) seat(ends,left_end(around,strip),left_wait);"));
        assert!(native.contains("found.push_back({left_end(items,strip),slot,Alone});"));
        assert!(native.contains(
            "return (items.widgets_before?std::max(strip,items.button.last):strip)+BoxGap;"
        ));
        // The button as it is seen is what its item draws, within the item;
        // a transparent brush counts. It is held with the item while that is
        // out for a moment.
        assert!(
            native.contains("const Ends seen=seen_button(element,root,point.X,point.X+width);")
        );
        assert!(native.contains("return border.Background() || (border.BorderBrush() &&"));
        assert!(native.contains("items.button={true,button_first,button_last};"));
        // Its animation's pixel or two is not followed.
        assert!(native.contains("constexpr double ButtonStepDip=4;"));
        assert!(native.contains("std::abs(items.button.last-button_last)>=ButtonStepDip;"));
        // Once Windows' Widgets switch is off, the button gone is not held;
        // the switch is read only while it is.
        assert!(native.contains(
            "} else if(widgets_known && now-widgets_at<WidgetsHoldMs && widgets_switched_on()) {"
        ));
        assert!(native.contains("L\"TaskbarDa\",RRF_RT_REG_DWORD,nullptr,&value,&size);"));
        assert!(native.contains("return status!=ERROR_SUCCESS || value!=0;"));
        assert!(!native.contains("beside_start("));
        assert!(native.contains("const double after=any?after_apps(around)"));
        assert!(native.contains("found.push_back({after_apps(items),slot,Alone});"));
        // Each by the button at its widest on that side, and where it was
        // for a moment after it goes, so a row does not move with it; the
        // choice made only once the taskbar was laid out after the margins
        // last changed, and a little more room needed to go beside than to
        // stay.
        assert!(native.contains(
            "items_of(left_repeater,root,left_owned.element,left_dip), around=steady(items);"
        ));
        assert!(native.contains(
            "repeater?steady(items_of(repeater,root,left_owned.element,left_dip)):Items{};"
        ));
        assert!(native.contains("constexpr DWORD WidgetsHoldMs=5000;"));
        assert!(native.contains("constexpr double SideRoomDip=12;"));
        assert!(native.contains("if(!left_moving() && std::isfinite(end)) {"));
        assert!(
            native
                .contains("afters_beside=!afters.empty() && around.widgets_after && beside_apps(")
        );
        assert!(
            native.contains(
                "return std::pair{part(ends)+part(starts),afters_beside?0.:part(afters)};"
            )
        );
        assert!(!native.contains("item_ends("));
    }

    #[test]
    fn a_box_has_a_place_and_a_dropped_one_is_only_reported() {
        // Each box's place in the hook's own enum and its rank, drags allowed
        // by the controller, and a drop written for the controller, with the
        // box it was dropped beside, which moves the box; the hook never
        // writes a place or a rank. A dragged box snaps within 30 DIP. Right
        // after it changes the margins, apps reaching beside the tray do not
        // hide the boxes there.
        use crate::choice::BoxPlace;
        let native = include_str!("../native/taskbar_tap.cpp");
        assert!(native.contains(
            "constexpr size_t DragAllowed=70088, DropSeq=70089, DropBox=70090, DropPlace=70091, DropOrder=70092, DropTarget=70093, Drags=70094, DragsLost=70095;"
        ));
        assert!(native.contains("constexpr size_t RightWidth=70096, RightTicks=70097;"));
        for (name, word) in [
            ("DRAG_ALLOWED", 70088),
            ("DROP_SEQ", 70089),
            ("DROP_BOX", 70090),
            ("DROP_PLACE", 70091),
            ("DROP_ORDER", 70092),
            ("DROP_TARGET", 70093),
            ("DRAGS", 70094),
            ("DRAGS_LOST", 70095),
            ("RIGHT_WIDTH", 70096),
            ("RIGHT_TICKS", 70097),
        ] {
            assert!(
                CONTROLLER.contains(&format!("const {name}: usize = {word};")),
                "{name}"
            );
        }
        assert_eq!(BoxPlace::ALL.map(BoxPlace::word), [1, 4, 5, 2]);
        assert!(native.contains("enum Order : LONG { Alone=0, BeforeOther=1, AfterOther=2, };"));
        assert!(native.contains("constexpr double DragDip=4, SnapDip=30;"));
        // Only snaps at one spot are one: with many apps, after the last one
        // and beside the tray are both places.
        assert!(native.contains("constexpr double SnapSameDip=2;"));
        assert!(native.contains("std::abs(k.x-snap.x)<SnapSameDip;"));
        for word in ["BoxPlaces", "BoxRanks", "DragAllowed"] {
            assert!(!native.contains(&format!("words+{word},")), "{word}");
            assert!(!native.contains(&format!("add({word},")), "{word}");
        }
        assert!(native.contains("state->add(DropSeq,1);"));
        assert!(native.contains("if(state->read(DragAllowed)!=1 || !widget || !parent) return;"));
        assert!(native.contains("constexpr DWORD MarginGraceMs=1000;"));
        assert!(native.contains("if(items>start+.5 && !left_moving()) return Settling;"));
        assert!(
            native.contains(
                "seat(afters,after,blocked || (reaches && !left_moving())?Settling:Shown);"
            )
        );
    }

    #[test]
    fn nothing_is_added_to_explorers_tray_panel() {
        // A child in SystemTrayFrameGrid crashed Explorer at overflow; the
        // reservation widens the panel's padding instead.
        let native = include_str!("../native/taskbar_tap.cpp");
        assert!(!native.contains("Children().InsertAt"));
        assert!(!native.contains("ColumnDefinitions().InsertAt"));
        assert!(native.contains("padding(padding_applied);"));
    }

    #[test]
    fn shared_times_use_a_clock_that_stops_during_sleep() {
        // A clock that counts sleep ages the heartbeat by the whole sleep on
        // resume, and the hook then removes a live controller's widget for good.
        // Both sides must read the same clock.
        let native = include_str!("../native/taskbar_tap.cpp");
        assert!(!native.contains("GetTickCount"));
        assert!(
            native.contains("QueryUnbiasedInterruptTime(&t); return static_cast<DWORD>(t/10000);")
        );
        // The controller keeps the sleep-counting clock only for log lines,
        // which nothing compares against.
        let controller = CONTROLLER;
        assert!(!controller.contains("GetTickCount64()"));
        let log = include_str!("log.rs");
        assert_eq!(log.matches("GetTickCount64()").count(), 1);
        assert!(log.contains("unsafe { GetTickCount64() }, text.as_ref()"));
    }
}
