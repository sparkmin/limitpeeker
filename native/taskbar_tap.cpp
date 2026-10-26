// Opt-in XAML TAP. See THIRD_PARTY_NOTICES.md.
// No DllMain work, no window owner/parent changes, no remote-memory patching.
#define DllGetClassObject PeekerSdkDllGetClassObject
#define DllCanUnloadNow PeekerSdkDllCanUnloadNow
#include <windows.h>
#include <ocidl.h>
#include <xamlom.h>
#undef DllGetClassObject
#undef DllCanUnloadNow
#ifdef GetCurrentTime
#undef GetCurrentTime
#endif
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.UI.h>
#include <winrt/Windows.UI.Core.h>
#include <winrt/Windows.UI.Input.h>
#include <winrt/Windows.UI.Text.h>
#include <winrt/Windows.UI.Xaml.h>
#include <winrt/Windows.UI.Xaml.Automation.h>
#include <winrt/Windows.UI.Xaml.Controls.h>
#include <winrt/Windows.UI.Xaml.Input.h>
#include <winrt/Windows.UI.Xaml.Media.h>
#include <winrt/Windows.UI.Xaml.Media.Animation.h>
#include <winrt/Windows.UI.Xaml.Media.Imaging.h>
#include <winrt/Windows.UI.Xaml.Shapes.h>
#include <winrt/Windows.Storage.Streams.h>
#include <algorithm>
#include <array>
#include <atomic>
#include <cmath>
#include <cstring>
#include <cwchar>
#include <limits>
#include <memory>
#include <mutex>
#include <numbers>
#include <string>
#include <string_view>
#include <thread>
#include <vector>

namespace x = winrt::Windows::UI::Xaml;
namespace c = x::Controls;
namespace xa = x::Automation;
namespace input = x::Input;
namespace m = x::Media;
namespace anim = m::Animation;
namespace imaging = m::Imaging;
namespace shapes = x::Shapes;
namespace f = winrt::Windows::Foundation;
namespace core = winrt::Windows::UI::Core;
// The constant lines below are read as text by the Rust tests in
// src/xaml_layout.rs; keep each on one line as it is.
constexpr GUID TapClsid{0x3f4bddb3,0x3bd6,0x4279,{0x96,0xbd,0x6c,0xc0,0x30,0x75,0xa1,0x81}};
constexpr LONG Magic = 0x4c504b31;
// Must match VERSION in src/xaml.rs. Both sides and the reuse check compare it.
// 32: a hook takes the session only for the attach that loaded it, and says
// why it turned one down.
constexpr LONG Version = 32;
// The one label shown, requested by the controller as DisplayMode::Full, and
// the words for the request, what is shown, and the label's width in DIP.
// The five-rung ladder and its gap and raise margins are gone.
constexpr LONG Full = 4;
constexpr size_t RequestWord=5, ShownWord=22, FullWidth=43;
// The controller's tray window, as the low 32 bits of its handle, which is all
// a window handle uses, and how often a right click on the box asked it for its
// menu, with the message that opens the menu. The hook posts it only to a window
// of the controller's own process.
constexpr size_t MenuWindow=3, MenuRequests=6;
constexpr UINT OpenMenu=WM_APP+4;
// The label comes from the controller, so a new value or format never needs a
// new DLL. Everything from here to the icons is written under one sequence (odd
// while being written): the two string lengths, the sequence taken, how often a
// text was wider than its widest form, and the sequence of the label drawn,
// which lags the one taken while a wider label waits for room.
constexpr size_t TextSeq=48, TextLength=49, SizingLength=50, TextApplied=51, TextClipped=52, TextDrawn=53;
// The look: the box's and the gauge track's colors (ARGB), then the font size,
// font weight, box height, padding each side, gap between cells, gauge height,
// gap between a text and its gauge and corner radius, sizes in hundredths of a
// DIP. The hook holds no look of its own.
constexpr size_t StyleBox=54, StyleTrack=55, StyleFont=56, StyleWeight=57, StyleHeight=58, StylePadding=59, StyleGap=60, StyleGauge=61, StyleGaugeGap=62, StyleRadius=63;
// What each accepts; a label with any value outside is refused whole. Must
// match style.rs.
constexpr LONG FontMin=800, FontMax=2400;
constexpr LONG WeightMin=100, WeightMax=900;
constexpr LONG HeightMin=1600, HeightMax=4800;
constexpr LONG PaddingMin=0, PaddingMax=2400;
constexpr LONG GapMin=0, GapMax=4800;
constexpr LONG GaugeMin=0, GaugeMax=800;
constexpr LONG GaugeGapMin=0, GaugeGapMax=800;
constexpr LONG RadiusMin=0, RadiusMax=1600;
// More of the look: the gauge below the text (0), beside it (1) or a dial
// before it (2), its width beside or its diameter as a dial, each cell's
// card (background, 1 DIP border, corner radius, padding each side, inset
// top and bottom) and how long a fill takes to move, in ms.
constexpr size_t StyleLayout=64, StyleGaugeWidth=65, StyleCellColor=66, StyleCellBorder=67, StyleCellRadius=68, StyleCellPadding=69, StyleCellInset=70, StyleMotion=71;
constexpr LONG LayoutMin=0, LayoutMax=2;
constexpr LONG GaugeWidthMin=0, GaugeWidthMax=12000;
constexpr LONG CellRadiusMin=0, CellRadiusMax=1600;
constexpr LONG CellPaddingMin=0, CellPaddingMax=2400;
constexpr LONG CellInsetMin=0, CellInsetMax=1600;
constexpr LONG MotionMin=0, MotionMax=400;
// Icons and cell widths: the icon's size (0 for none) and its gap before the
// gauges, and whether every cell takes the widest cell's width.
constexpr size_t StyleIcon=72, StyleIconGap=73, StyleUniform=74;
constexpr LONG IconMin=0, IconMax=3200;
constexpr LONG IconGapMin=0, IconGapMax=1600;
constexpr LONG UniformMin=0, UniformMax=1;
// Where a gauge's colored part sits, from the start (0: a bar's left end, a
// dial clockwise from twelve o'clock) or against the end (1: the right end,
// counterclockwise), which each gauge now says for itself in GaugeEnds and is
// only checked here, and where a value sits in its column; how a cell's
// gauges are arranged, stacked (0), side by side (1) or as rings on one dial
// (2), the space between them, between a label and its value, a ring's
// thickness, and the labels' color.
constexpr size_t StyleFillEnd=75, StyleTextEnd=76, StyleArrange=77, StyleSpacing=78, StyleLabelGap=79, StyleRing=80, StyleLabelColor=81;
constexpr LONG FillEndMin=0, FillEndMax=1;
constexpr LONG TextEndMin=0, TextEndMax=1;
constexpr LONG ArrangeMin=0, ArrangeMax=2;
constexpr LONG SpacingMin=0, SpacingMax=1600;
constexpr LONG LabelGapMin=0, LabelGapMax=1600;
constexpr LONG RingMin=0, RingMax=800;
// How heavy a warning's mark is; it is drawn a character at a time.
constexpr size_t StyleMarkWeight=82;
constexpr LONG MarkWeightMin=100, MarkWeightMax=900;
// The label: up to four cells of up to three gauges, twelve in all. Each gauge
// has a fill in permille (-1 for none) and its value's and fill's colors, in
// order across the cells; then the text and its widest form as UTF-16, two
// units a word. In both strings cells are split by U+001F, a cell's gauges by
// U+001E, and a gauge's label, warning mark and value by U+001D.
constexpr size_t GaugeFills=83, GaugeTextColors=95, GaugeFillColors=107, TextChars=119, SizingChars=215;
constexpr LONG TextCapacity=192, MaxCells=4, MaxGauges=12, MaxCellGauges=4;
constexpr wchar_t CellBreak=0x1f, GaugeBreak=0x1e, LabelBreak=0x1d;
// The icons, apart from the label and under their own sequence, since they
// change rarely: the sequence taken, each cell's size in pixels (0 for none),
// then per cell a block of premultiplied BGRA pixels. The controller draws them
// at the size shown; nothing here opens or parses an image.
constexpr size_t IconSeq=311, IconTaken=312, IconSizes=313, IconPixels=321, IconBlock=4096;
constexpr LONG IconMaxPx=64;
// A scene in place of the label: blocks the controller placed, each drawn
// where it says, written under TextSeq with the look. The mode (0 the label, 1
// the scene), the scene's width in hundredths of a DIP, how many blocks, and
// the length of its text pool; then per block its kind, x, y, width, height,
// fill in permille (-1 for none), color, track color, size, weight, options
// and reference; then the pool as UTF-16, two units a word.
constexpr size_t SceneMode=16705, SceneWidth=16706, SceneCount=16707, ScenePoolLength=16708, SceneItems=16709, SceneItemWords=12, ScenePool=17285;
constexpr LONG MaxItems=48, ScenePoolCapacity=512, SceneWidthMin=100, SceneWidthMax=60000;
enum Kind : LONG { TextKind=1, BarKind=2, RingKind=3, PieKind=4, IconKind=5, };
// The longest text a scene's block holds, in UTF-16 units.
constexpr LONG ItemTextMax=64;
// A ring's thickness, in hundredths of a DIP.
constexpr LONG ThicknessMin=50, ThicknessMax=1600;
// How long the whole strip must stay clear of apps before the box may use it,
// and how long a wider label waits for that before it is drawn anyway: old
// values must not stay on screen as if current.
constexpr DWORD SettleMs=500, GrowWaitMs=3000;
// The hook's measurement, published for the controller's log.
constexpr size_t MeasureSeq=32, MeasureValid=33, MeasureItems=34,
    MeasureTray=35, MeasureDpi=36, MeasureTaken=37;
// Why the box is hidden (0 when shown), and the width of the tree adopted at
// attach. The reason is always on record.
constexpr size_t HideReason=38, AttachRootWidth=39;
enum Hide : LONG { Shown=0, NoMeasure=1, NotRequested=2, NoReservation=3, Settling=4, NoText=5, };
// How often the label width was measured.
constexpr size_t LabelMeasures=20;// Why the last measurement failed (0 when it succeeded). "No measurement" alone
// cannot tell a taskbar moved to the top apart from a Windows update renaming
// the anchors, and those need opposite responses.
constexpr size_t MeasureFailure=21;
enum Failure : LONG {
    Measured_=0, NotShellWindow=1, NotPrimary=2, NotBottom=3, NoRoot=4,
    BadScale=5, RootSizeMismatch=6, WalkFailed=7, AnchorsMissing=8, NoRects=9,
};
// How far the hook got attaching its session; the failing call is the stage
// after the one recorded. A reused session failed with E_UNEXPECTED before any
// tree callback, and status 5 alone could not say which call.
constexpr size_t AttachStage=30;
enum Stage : LONG {
    NoStage=0, SiteSet=1, ThreadReady=2, DiagnosticsReady=3, TreeServiceReady=4,
    Advised=5, FrameSeen=6, Dispatched=7, Started=8,
};
// Subscribe calls made (low byte) and cleanup unsubscribes that succeeded (next
// byte). A reused session's subscribe failed with E_UNEXPECTED after the first
// session lived through sleep; a successful cleanup means the failed subscribe
// was in fact registered.
constexpr size_t AdviseTries=31;
constexpr LONG AdviseAttempts=8;
// The attach the controller made last, which a hook's initialization data must
// name; how often SetSite reached the session; and why it last turned the
// session down. An attach returns before its hook starts, so a hook that comes
// late, after the controller attached again, must not start a second box, and
// one that never takes the session must leave a trace.
constexpr size_t AttachToken=7, SiteCalls=8, SiteRefusal=9;
enum Refusal : LONG { NotRefused=0, SiteAgain=1, Elevated=2, NotTaskbar=3, LeaseGone=4, OtherAttach=5, Taken=6, SetupFailed=7, };
// Reservation: what happened to it, the box's width in px (or a refusal's
// reason), and ticks spent settling while an app button still reached into the
// strip.
constexpr size_t ReserveState=45, ReserveWidth=46, SettlingTicks=47;
enum Reserve : LONG {
    NotReserved=0, Reserved=1, TargetRefused=2, Restored=3, StructureChanged=4, RestoreMismatch=5,
};
// Where each gauge's colored part sits, from the start (0) or against the end
// (1), apart from the look, so a meter beside a limit can differ. In the box's
// words, after the scene.
constexpr size_t GaugeEnds=17541;
// The box's words run from TextSeq to the gauges' ends. Each other box has a
// copy of them, BoxWords further on than the one before.
constexpr size_t BoxFirst=48, BoxWords=17505;
// What became of the room on both sides of the taskbar repeater, the left
// side's width in px (or, refused, 1 for no TaskbarFrame and 2 for no
// repeater under it), the ticks an item reached into the left strip, and
// where the items start in px. A place: the taskbar's left end, beside the
// tray, just before the Start button, just after the last app.
constexpr size_t LeftState=11, LeftWidth=12, LeftTicks=13, LeftItemsStart=14;
enum Place : LONG { Nowhere=0, LeftEnd=1, Tray=2, BeforeStart=4, AfterApps=5, };
enum Left : LONG { LeftNone=0, LeftReserved=1, LeftRefused=2, LeftRestored=3, LeftAbandoned=4, LeftMismatch=5, };
// The room between two boxes, and on either side of a row at the left.
constexpr double BoxGap=6;
// Up to four boxes. After their words, per box: where the controller asks
// for it (enum Place; the first box's Nowhere is beside the tray, any
// other's is no box), its rank there (the lower, the further left), whether
// it shows, why not and its width in DIP. Then whether a box may be dragged;
// where one was dropped, under its sequence: which box, its place, its order
// there (enum Order) and the box it is beside (-1 for none); how often a
// drag began and how often one was lost; and after the last app, the
// repeater's right margin in px and the ticks the row there reached the
// tray's side. The hook only reports a drop; the controller moves the box.
constexpr size_t MaxBoxes=4;
constexpr size_t BoxPlaces=70068, BoxRanks=70072, BoxShown=70076, BoxHide=70080, BoxWidth=70084;
constexpr size_t DragAllowed=70088, DropSeq=70089, DropBox=70090, DropPlace=70091, DropOrder=70092, DropTarget=70093, Drags=70094, DragsLost=70095;
constexpr size_t RightWidth=70096, RightTicks=70097;
// Where the Widgets button is, its left and right ends in px, while the
// margins are in; 0 and 0 when there is none. Then the same for the button
// inside it as it is seen.
constexpr size_t WidgetsLeft=70098, WidgetsRight=70099, SeenLeft=70100, SeenRight=70101;
enum Order : LONG { Alone=0, BeforeOther=1, AfterOther=2, };
// How far the pointer moves before a press becomes a drag, and how near a
// place a dragged box snaps to it, in DIP; how long a dropped box stays where
// it was let go while the controller moves it.
constexpr double DragDip=4, SnapDip=30;
// Snaps this near are one place.
constexpr double SnapSameDip=2;
constexpr DWORD DropHoldMs=1000;
// Right after the hook changes the left margin, the apps are laid out once
// as if moved by all of it before they settle at half, and reach into the
// strip beside the tray; that is not taken for apps in the way.
constexpr DWORD MarginGraceMs=1000;
// How long the Widgets button, gone for a moment while the taskbar is laid
// out again, is still taken to be where it was; and how much room to spare
// a row needs to go beside the apps, where it stays while it has any, so it
// does not flap at the edge.
constexpr DWORD WidgetsHoldMs=5000;
constexpr double SideRoomDip=12;
// How far the button inside the Widgets button's item, as it is seen, must
// move before the row after it follows: its animation moves it a pixel or
// two back and forth.
constexpr double ButtonStepDip=4;
// Must equal WORDS*4 in src/xaml.rs.
constexpr DWORD SharedBytes=70102*4;
// The session's own words: the controller's process id (0 once it stops),
// when it last observed (not read here), the hook's status and error, how
// often the tree's layout changed and the box was shown or hidden, the
// controller's heartbeat, and what the ticks cost: how many, their time, the
// tree walks' time and the nodes they visited.
constexpr size_t ControllerWord=2, ObservedWord=4, StatusWord=15, ErrorWord=16, LayoutRevisionWord=17, VisibilityWord=18;
constexpr size_t HeartbeatWord=23, TicksWord=24, TickMicrosWord=25, WalkMicrosWord=26, VisitedWord=27;
// How long the heartbeat may be silent before the box is taken off for good.
constexpr DWORD LeaseMs=1500;
// A hook whose box came off cleanly rests, still subscribed (status 7), until
// Explorer ends; a controller started again claims it (8), and it starts the
// box again once that controller's lease runs, or leaves after ClaimMs. Must
// match RESTING and CLAIMED in src/xaml/channel.rs.
constexpr LONG Resting=7, Claimed=8;
constexpr DWORD ClaimMs=10000;
// Cost accounting. The shell CPU this hook causes has to be separable from what
// servicing the controller's UI Automation queries costs, and only the hook can
// measure its own share.
static LARGE_INTEGER frequency = [] { LARGE_INTEGER f{}; QueryPerformanceFrequency(&f); return f; }();
inline LONG elapsed_us(LONGLONG from) {
    LARGE_INTEGER t{}; QueryPerformanceCounter(&t);
    if(!frequency.QuadPart) return 0;
    return static_cast<LONG>((t.QuadPart - from) * 1000000 / frequency.QuadPart);
}
inline LONGLONG stamp() { LARGE_INTEGER t{}; QueryPerformanceCounter(&t); return t.QuadPart; }
// Every time shared with the controller, in ms. Stops during sleep, so resuming
// does not age the heartbeat past the lease and remove the widget of a live
// controller. Must match shared_now() in src/xaml.rs.
inline DWORD shared_now() { ULONGLONG t=0; QueryUnbiasedInterruptTime(&t); return static_cast<DWORD>(t/10000); }

struct State {
    HANDLE mapping{};
    volatile LONG* words{};
    std::atomic<bool> scheduled{false}, stopped{false};
    // The taskbar thread's dispatcher, kept once the box starts, so a resting
    // hook can start it again there. A CoreDispatcher may be called from any
    // thread.
    std::mutex lock;
    core::CoreDispatcher dispatcher{nullptr};
    LONG read(size_t index) const { return InterlockedCompareExchange(words+index,0,0); }
    void add(size_t index, LONG value) { InterlockedExchangeAdd(words+index,value); }
    void status(LONG status, HRESULT error=S_OK) {
        if(read(StatusWord)==5 && status!=5) return; // Preserve the first observable failure through cleanup.
        InterlockedExchange(words+ErrorWord,error); InterlockedExchange(words+StatusWord,status);
    }
    // Only moves forward: tree callbacks may arrive before the subscribe call returns.
    void reach(Stage stage) {
        for(LONG seen=read(AttachStage);seen<stage;) {
            const LONG prior=InterlockedCompareExchange(words+AttachStage,stage,seen);
            if(prior==seen) break;
            seen=prior;
        }
    }
    // Lease: the controller process is gone. Permanent, so it must never be
    // driven by an observation that a busy shell can delay.
    bool expired() const {
        return read(ControllerWord)==0 || shared_now()-static_cast<DWORD>(read(HeartbeatWord))>LeaseMs;
    }
    ~State() { if(words) UnmapViewOfFile(const_cast<LONG*>(words)); if(mapping) CloseHandle(mapping); }
};

// Refuse elevated shells; the controller separately requires a non-elevated token.
bool permitted() {
    HANDLE token{};
    if(!OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY,&token)) return false;
    TOKEN_ELEVATION elevation{}; DWORD bytes=0;
    const bool ok=GetTokenInformation(token,TokenElevation,&elevation,sizeof(elevation),&bytes)!=FALSE;
    CloseHandle(token);
    return ok && !elevation.TokenIsElevated;
}

// Context owns its own UI element and events. The only native change it makes
// is the tray panel's padding, undone by ownership.
struct Context : std::enable_shared_from_this<Context> {
    std::shared_ptr<State> state;
    // Which box this is, from 0: its words start `base` past the first box's
    // and its width goes in `width_word`. The first box places all of them;
    // the others, in `others`, are drawn where it puts them.
    size_t box_index=0, base=0, width_word=FullWidth;
    std::array<std::shared_ptr<Context>,MaxBoxes-1> others;
    // The box that places all, which a dropped box is reported through.
    std::weak_ptr<Context> placer;
    Context* box_at(size_t at) { return at==0?this:others[at-1].get(); }
    LONG word(size_t index) const { return state->read(base+index); }
    volatile LONG* where(size_t index) const { return state->words+base+index; }
    c::Grid parent{nullptr};
    c::Border widget{nullptr};
    // The scale the label widths were last measured at. Widths measured before
    // the widget joined the tree, or at another scale, can be a fraction short
    // once XAML rounds text to the current pixel grid.
    double measured_scale=0;
    // How the box looks, as sent with the label.
    struct Style {
        uint32_t box=0, track=0;
        LONG font=0, weight=0, height=0, padding=0, gap=0, gauge=0, gauge_gap=0, radius=0;
        LONG layout=0, gauge_width=0;
        uint32_t cell_color=0, cell_border=0;
        LONG cell_radius=0, cell_padding=0, cell_inset=0, motion=0;
        LONG icon=0, icon_gap=0, uniform=0;
        LONG fill_end=0, text_end=0, arrange=0, spacing=0, label_gap=0, ring=0;
        uint32_t label_color=0;
        LONG mark_weight=0;
        bool operator==(Style const&) const = default;
        static double dip(LONG hundredths) { return hundredths/100.0; }
        bool beside() const { return layout==1; }
        bool dial() const { return layout==2; }
        bool side() const { return arrange==1; }
        bool rings() const { return dial() && arrange==2; }
        double border() const { return (cell_border>>24)?1.0:0.0; }
        double icon_part() const { return icon>0?dip(icon)+dip(icon_gap):0; }
    };
    // One gauge as the hook holds it: its label, warning mark and value, their
    // widest forms, its fill and colors, and its label column's width once
    // measured.
    struct Gauge {
        std::wstring label, label_widest, mark, mark_widest, value, value_widest;
        LONG fill=-1;
        uint32_t text_color=0, fill_color=0;
        double name=0;
        // Whether its colored part sits against the end.
        bool end=false;
    };
    // One block of a scene as taken: its kind, place and size in hundredths
    // of a DIP, fill, colors, size (a text's font, a ring's thickness),
    // weight, options, and its text or icon slot.
    struct Item {
        LONG kind=0, x=0, y=0, w=0, h=0, fill=-1, size=0, weight=0, options=0, slot=0;
        uint32_t color=0, track=0;
        std::wstring text;
        bool operator==(Item const&) const = default;
    };
    // A label as the hook holds it: each cell's gauges, the look, and each
    // cell's mark and number columns, the value column they make, and its
    // width, measured from the widest forms. Or a scene: blocks placed by the
    // controller, as wide as it says.
    struct Label {
        LONG seq=0;
        std::vector<std::vector<Gauge>> cells;
        Style style;
        std::vector<double> marks, numbers, values, widths;
        double dip=0;
        bool scene=false;
        std::vector<Item> items;
        bool empty() const { return cells.empty() && items.empty(); }
    };
    // What the box shows, and a label taken but not drawn yet because it is
    // wider than the part of the strip known to be clear.
    Label drawn, pending;
    bool has_pending=false;
    DWORD pending_since=0;
    LONG text_seq=0;
    // One gauge on screen: its label, value and warning mark, a character a
    // text, and a track and fill, or a dial's wedge or a ring's arc with the
    // fill it was drawn for.
    struct Meter {
        c::Grid text{nullptr};
        c::TextBlock label{nullptr}, value{nullptr};
        c::StackPanel marks{nullptr};
        std::wstring mark;
        c::Border track{nullptr}, fill{nullptr};
        shapes::Path wedge{nullptr};
        LONG wedge_fill=-2;
        anim::Storyboard story{nullptr};
        uint32_t text_color=0, fill_color=0;
        bool end=false;
    };
    // One cell of the box: a service's icon and its gauges.
    struct Cell {
        c::Border root{nullptr};
        c::Image icon{nullptr};
        imaging::WriteableBitmap bitmap{nullptr};
        std::vector<Meter> meters;
    };
    // The icons as last taken, one per cell, and the sequence they came
    // under.
    struct Icon {
        LONG size=0;
        std::vector<uint32_t> pixels;
    };
    Icon icons[MaxCells];
    LONG icon_seq=0;
    bool icons_changed=false;
    std::vector<Cell> cells;
    c::StackPanel row{nullptr};
    // A scene's blocks on screen, on a canvas over the box, and each icon
    // slot's bitmap, which every icon block of that slot shows.
    struct Block {
        x::FrameworkElement root{nullptr};
        Meter meter;
        c::TextBlock text{nullptr};
        shapes::Ellipse track{nullptr};
        c::Image image{nullptr};
        double side=-1;
        LONG size=0, weight=0;
        uint32_t track_color=0;
    };
    std::vector<Block> blocks;
    c::Canvas stage{nullptr};
    imaging::WriteableBitmap slots[MaxCells]{nullptr,nullptr,nullptr,nullptr};
    // Measures widths in the tree, at the current scale, without taking room.
    c::TextBlock measurer{nullptr};
    x::DispatcherTimer timer{nullptr};
    winrt::event_token tick{}, layout{}, right_tapped{}, context_requested{};
    winrt::event_token pressed_token{}, moved_token{}, released_token{}, lost_token{};
    bool have_tick=false, have_layout=false, have_input=false, detached=false, updating=false;

    // A drag: the left button went down on the box at press_x (the parent's
    // DIPs) with the box at press_left, and the pointer has since moved far
    // enough to be dragging it. A dropped box is held where it was let go
    // until the place words change or DropHoldMs passes.
    bool pressed=false, dragging=false, held=false;
    double press_x=0, press_left=0;
    DWORD dropped_at=0;
    // Every box's place and rank, as the controller asked for them.
    using Layout=std::array<LONG,2*MaxBoxes>;
    Layout layout_words() const {
        Layout words{};
        for(size_t at=0;at<MaxBoxes;++at) {
            words[at]=state->read(BoxPlaces+at);
            words[MaxBoxes+at]=state->read(BoxRanks+at);
        }
        return words;
    }
    Layout held_layout{};
    bool pinned() const {
        return dragging || (held && shared_now()-dropped_at<DropHoldMs && layout_words()==held_layout);
    }
    // Where a dragged box may land, in the parent's DIPs, with the place,
    // the order and the box beside it that it stands for, taken when the
    // drag begins; the one it sits on, or -1.
    struct Snap { double x=0; Place place=Nowhere; Order order=Alone; LONG target=-1; };
    std::vector<Snap> snaps;
    int snapped=-1;

    // Reservation: widen the tray's own panel by its left padding and draw the
    // box as an overlay in the strip that opens, so Windows lays the apps out,
    // compacts and overflows them around it. Nothing is added to Explorer's
    // panel: a child there crashed Explorer at overflow. The padding is
    // recorded and undone by ownership.
    x::FrameworkElement anchor{nullptr};
    bool reserve_done=false;
    int reserve_attempts=0;
    c::StackPanel tray_stack{nullptr};
    c::Grid tray_grid{nullptr};
    bool padding_local=false;
    x::Thickness padding_original{}, padding_applied{};
    double reserved_dip=0;
    // The part of the strip, from the tray's end, that the apps have been seen
    // to stay out of. It grows only after the whole strip has stayed clear for
    // SettleMs, never on the tick that changed the padding.
    double settled_dip=0;
    bool waiting_layout=false, clearing=false;
    DWORD clear_since=0;

    // A margin of one of Explorer's elements this hook put in: whether it
    // had a local value, what it was, and what is in now. Given back only
    // while it is still that, as the tray's padding is.
    struct Owned {
        x::FrameworkElement element{nullptr};
        bool local=false;
        x::Thickness original{}, applied{};
        static Owned of(x::FrameworkElement const& element) {
            Owned owned{element};
            owned.local=element.ReadLocalValue(x::FrameworkElement::MarginProperty())!=x::DependencyProperty::UnsetValue();
            owned.original=owned.applied=element.Margin();
            return owned;
        }
        // The original margin with `left` and `right` more.
        void put(double left, double right) {
            applied={original.Left+left,original.Top,original.Right+right,original.Bottom};
            element.Margin(applied);
        }
        bool kept() const { return !element || same(element.Margin(),applied); }
        // False, and left as it is, when someone else changed it.
        bool give_back() {
            if(!element) return true;
            const bool ours=kept();
            if(ours && local) element.Margin(original);
            else if(ours) element.ClearValue(x::FrameworkElement::MarginProperty());
            element=nullptr;
            return ours;
        }
    };
    // The places at the left and after the apps. The room after the apps is
    // the taskbar repeater's right margin. The room at the left is the left
    // margin of the apps' first item, the Start button, so the repeater
    // itself stays at the screen's left: Windows places the search box it
    // draws while Search is open by where that box is inside the repeater.
    // Centered, the Widgets button keeps the left end.
    x::FrameworkElement left_repeater{nullptr};
    Owned right_owned, left_owned;
    // When the item with the room at the left was last laid out.
    DWORD left_seen=0;
    bool left_done=false;
    int left_attempts=0;
    // The left margin's part this hook added, and the right one's.
    double left_dip=0, right_dip=0;
    // When this hook last put the margins in or changed them.
    DWORD left_changed=0;
    bool left_moving() const { return left_changed && shared_now()-left_changed<MarginGraceMs; }
    // The Widgets button as last seen and when, the button inside its item,
    // and its widest before the apps and after them: each time the taskbar
    // is laid out again, Windows takes it out for a moment, puts it back as
    // an icon and widens it to show the weather.
    bool widgets_known=false;
    double widgets_first=0, widgets_last=0, widgets_widest_before=0, widgets_widest_after=0;
    double button_first=0, button_last=0;
    DWORD widgets_at=0;
    // Whether the row after the last app stands beside the apps with no
    // margin of its own: only while the Widgets button is on its side and
    // leaves room. The row before the Start button always has its room in
    // the Start button's margin.
    bool afters_beside=false;

    bool reserved() const { return tray_stack || tray_grid; }
    x::FrameworkElement tray_panel() const {
        return tray_stack?tray_stack.as<x::FrameworkElement>():tray_grid.as<x::FrameworkElement>();
    }
    x::DependencyProperty padding_property() const {
        return tray_stack?c::StackPanel::PaddingProperty():c::Grid::PaddingProperty();
    }
    x::Thickness padding() const { return tray_stack?tray_stack.Padding():tray_grid.Padding(); }
    void padding(x::Thickness const& value) { if(tray_stack) tray_stack.Padding(value); else tray_grid.Padding(value); }
    static bool same(x::Thickness const& a, x::Thickness const& b) {
        return std::abs(a.Left-b.Left)<.01 && std::abs(a.Top-b.Top)<.01
            && std::abs(a.Right-b.Right)<.01 && std::abs(a.Bottom-b.Bottom)<.01;
    }

    // Changes nothing unless the tray's panel is one this hook knows. A refusal
    // leaves its reason in ReserveWidth, one byte each from the lowest: the
    // child count where the search stopped, that node's column count (255 when
    // it is not a Grid), its child count, and the depth reached.
    bool reserve_space(double width) {
        const auto byte=[](int64_t v) { return static_cast<LONG>(v<0?0:v>255?255:v); };
        const auto refused=[&](int64_t anchors,int64_t columns,int64_t children,int64_t depth) {
            InterlockedExchange(state->words+ReserveWidth,
                byte(anchors)|(byte(columns)<<8)|(byte(children)<<16)|(byte(depth)<<24));
            return false;
        };
        // Follow only single-child wrappers, at most four deep. A fan-out
        // means an unfamiliar tree, which is refused rather than guessed at.
        x::DependencyObject node=anchor;
        int32_t fanout=0;
        int depth=0;
        for(;depth<4;++depth) {
            fanout=m::VisualTreeHelper::GetChildrenCount(node);
            if(fanout!=1) break;
            node=m::VisualTreeHelper::GetChild(node,0);
            if(auto stack=node.try_as<c::StackPanel>(); stack && stack.Orientation()==c::Orientation::Horizontal) { tray_stack=stack; break; }
            if(auto grid=node.try_as<c::Grid>(); grid && grid.ColumnDefinitions().Size()>0) { tray_grid=grid; break; }
        }
        if(!reserved()) {
            auto stopped=node.try_as<c::Grid>();
            return refused(fanout,stopped?0:255,stopped?stopped.Children().Size():0,depth);
        }
        padding_local=tray_panel().ReadLocalValue(padding_property())!=x::DependencyProperty::UnsetValue();
        padding_original=padding();
        padding_applied={padding_original.Left+width,padding_original.Top,padding_original.Right,padding_original.Bottom};
        padding(padding_applied);
        reserved_dip=width;
        return true;
    }

    // Undoes the padding only if it is still the value this hook applied;
    // anything else belongs to someone else and is counted as a mismatch.
    void restore_space(Reserve outcome) {
        if(!reserved()) return;
        bool mismatch=false;
        if(same(padding(),padding_applied)) {
            if(padding_local) padding(padding_original);
            else tray_panel().ClearValue(padding_property());
        } else mismatch=true;
        tray_stack=nullptr; tray_grid=nullptr;
        InterlockedExchange(state->words+ReserveState,mismatch?RestoreMismatch:outcome);
    }

    // Someone else changed the padding, or the panel left the tree: step back
    // without touching it rather than fight over it.
    void abandon_space() {
        tray_stack=nullptr; tray_grid=nullptr;
        settled_dip=0;
        InterlockedExchange(state->words+ReserveState,StructureChanged);
    }
    // Takes the boxes off first, then gives the padding and the margin
    // back, each caught on its own, so a padding that cannot be restored
    // never leaves a stale box drawn with its timer running.
    void detach() noexcept {
        if(detached) return;
        detached=true;
        HRESULT failed=S_OK;
        try {
            if(timer) { timer.Stop(); if(have_tick) timer.Tick(tick); }
            if(parent && have_layout) parent.LayoutUpdated(layout);
            drop_widget();
        } catch(...) { failed=winrt::to_hresult(); }
        for(auto const& other:others) {
            try { if(other) other->drop_widget(); }
            catch(...) { if(SUCCEEDED(failed)) failed=winrt::to_hresult(); }
        }
        try { if(reserved()) restore_space(Restored); }
        catch(...) { if(SUCCEEDED(failed)) failed=winrt::to_hresult(); }
        try { restore_left(LeftRestored); }
        catch(...) { if(SUCCEEDED(failed)) failed=winrt::to_hresult(); }
        try {
            forget();
            for(auto& other:others) {
                if(other) other->forget();
                other=nullptr;
            }
            timer=nullptr;
        } catch(...) { if(SUCCEEDED(failed)) failed=winrt::to_hresult(); }
        InterlockedExchange(state->words+ShownWord,0);
        for(size_t at=0;at<MaxBoxes;++at) InterlockedExchange(state->words+BoxShown+at,0);
        if(FAILED(failed)) state->status(5,failed);
        else state->status(6); // UI detached; diagnostics unsubscription pending.
        state->stopped=true;
    }
    // Lets go of everything the box drew with, on its dispatcher.
    void forget() {
        clear_cells();
        for(auto& slot:slots) slot=nullptr;
        widget=nullptr; row=nullptr; stage=nullptr; measurer=nullptr; parent=nullptr;
    }

    // Occupancy is defined by two structural anchors, not by what a node
    // inherits from. Measuring the real tree showed Taskbar.TaskListButton and
    // every SystemTray.* element derive from neither ButtonBase nor TextBox, so
    // the old type filter saw 3 sized elements out of 257 and never once
    // refused a placement. Only these two names are assumed; without them the
    // placement is refused rather than guessed.
    static constexpr std::wstring_view ItemsAnchor{L"Microsoft.UI.Xaml.Controls.ItemsRepeater"};
    static constexpr std::wstring_view TrayAnchor{L"SystemTray.SystemTrayFrame"};

    struct Occupancy {
        int anchors=0, rects=0, visited=0;
        bool ok=true;
        double items_right=0, tray_left=0;
    };

    // One occupied rect. Anything without usable bounds contributes nothing; a
    // non-finite one aborts the walk instead of being guessed at.
    void occupy(x::DependencyObject const& node, x::UIElement const& root, Occupancy& o, bool tray) {
        if(node==widget) return;
        auto element=node.try_as<x::FrameworkElement>();
        if(!element || element.Visibility()!=x::Visibility::Visible) return;
        auto point=element.TransformToVisual(root).TransformPoint({0,0});
        const double width=element.ActualWidth(), height=element.ActualHeight();
        if(!std::isfinite(point.X)||!std::isfinite(point.Y)||!std::isfinite(width)||!std::isfinite(height)) { o.ok=false; return; }
        if(width<=0 || height<=0) return;
        ++o.rects;
        if(tray) { if(o.tray_left==0 || point.X<o.tray_left) o.tray_left=point.X; }
        else if(point.X+width>o.items_right) o.items_right=point.X+width;
    }

    void scan_occupancy(x::DependencyObject const& node, x::UIElement const& root, Occupancy& o, int depth=0) {
        if(!o.ok) return;
        if(++o.visited>2048 || depth>48) { o.ok=false; return; }
        if(node==widget) return;
        const auto name=winrt::get_class_name(node);
        if(name==TrayAnchor) { ++o.anchors; occupy(node,root,o,true); return; }
        if(name==ItemsAnchor) {
            ++o.anchors;
            const auto count=m::VisualTreeHelper::GetChildrenCount(node);
            if(count<0 || count>2048) { o.ok=false; return; }
            // The repeater spans the whole taskbar, so only its items describe
            // occupied space. Items do not nest, so this does not recurse.
            for(int index=0;index<count;++index)
                occupy(m::VisualTreeHelper::GetChild(node,index),root,o,false);
            return;
        }
        auto element=node.try_as<x::FrameworkElement>();
        if(element && element.Visibility()!=x::Visibility::Visible) return;
        const auto count=m::VisualTreeHelper::GetChildrenCount(node);
        if(count<0 || count>2048) { o.ok=false; return; }
        for(int index=0;index<count;++index)
            scan_occupancy(m::VisualTreeHelper::GetChild(node,index),root,o,depth+1);
    }

    // What one tick measured, in this process, from the live tree.
    struct Measured {
        bool ok=false;
        RECT rect{};
        UINT dpi=0;
        double scale=1;
        Occupancy occupancy;
        x::FrameworkElement root{nullptr};
    };

    // Measurement runs every tick and does not depend on anything the controller
    // published. It places the box, checks that the apps have cleared the
    // strip, and is logged by the controller.
    Measured measure() {
        Measured m;
        const auto fail=[&](Failure why) { InterlockedExchange(state->words+MeasureFailure,why); return m; };
        auto hwnd=FindWindowW(L"Shell_TrayWnd",nullptr);
        DWORD pid=0; GetWindowThreadProcessId(hwnd,&pid);
        if(pid!=GetCurrentProcessId() || !GetWindowRect(hwnd,&m.rect)) return fail(NotShellWindow);
        m.dpi=GetDpiForWindow(hwnd);
        MONITORINFO monitor{sizeof(MONITORINFO)};
        if(!GetMonitorInfoW(MonitorFromWindow(hwnd,MONITOR_DEFAULTTONULL),&monitor)
            || !(monitor.dwFlags&MONITORINFOF_PRIMARY)) return fail(NotPrimary);
        // Only a full-width taskbar along the bottom edge is supported. Top,
        // left and right placements exist on this Windows build and must fall
        // back to tray-only rather than be guessed at.
        if(m.rect.left!=monitor.rcMonitor.left || m.rect.right!=monitor.rcMonitor.right
            || m.rect.bottom!=monitor.rcMonitor.bottom) return fail(NotBottom);
        // A panel taken out of the tree has no root. Calling through a null
        // one is an access violation, which catch(...) does not catch.
        const auto xaml_root=parent.XamlRoot();
        if(!xaml_root) return fail(NoRoot);
        m.root=xaml_root.Content().try_as<x::FrameworkElement>();
        if(!m.root) return fail(NoRoot);
        m.scale=xaml_root.RasterizationScale();
        if(!std::isfinite(m.scale)||m.scale<1||m.scale>5) return fail(BadScale);
        // Reject unfamiliar coordinate origins/layouts rather than guessing.
        if(std::abs(m.root.ActualWidth()*m.scale-(m.rect.right-m.rect.left))>2
            || std::abs(m.root.ActualHeight()*m.scale-(m.rect.bottom-m.rect.top))>2) return fail(RootSizeMismatch);
        const auto walk=stamp();
        scan_occupancy(m.root,m.root,m.occupancy);
        state->add(WalkMicrosWord,elapsed_us(walk));
        state->add(VisitedWord,m.occupancy.visited);
        if(!m.occupancy.ok) return fail(WalkFailed);
        if(m.occupancy.anchors<2) return fail(AnchorsMissing);
        if(m.occupancy.rects<=0) return fail(NoRects);
        m.ok=true;
        InterlockedExchange(state->words+MeasureFailure,Measured_);
        return m;
    }

    // Publish under an odd/even sequence so the controller never reads a sample
    // that is half old and half new.
    void publish(Measured const& m) {
        state->add(MeasureSeq,1);
        if(m.ok) {
            // Occupied edges round outward, so the log never shows more room
            // than there is.
            InterlockedExchange(state->words+MeasureItems,m.rect.left+static_cast<LONG>(std::ceil(m.occupancy.items_right*m.scale)));
            InterlockedExchange(state->words+MeasureTray,m.rect.left+static_cast<LONG>(std::floor(m.occupancy.tray_left*m.scale)));
            InterlockedExchange(state->words+MeasureDpi,static_cast<LONG>(m.dpi));
            InterlockedExchange(state->words+MeasureTaken,static_cast<LONG>(shared_now()));
        }
        InterlockedExchange(state->words+MeasureValid,m.ok?1:0);
        state->add(MeasureSeq,1);
    }


    static std::vector<std::wstring> split(std::wstring const& value,wchar_t mark) {
        std::vector<std::wstring> parts;
        for(size_t from=0;;) {
            const auto at=value.find(mark,from);
            parts.push_back(value.substr(from,at==std::wstring::npos?std::wstring::npos:at-from));
            if(at==std::wstring::npos) return parts;
            from=at+1;
        }
    }
    // A label string's cells, each a list of gauges, each a label, a warning
    // mark (empty for none) and a value. Empty when a gauge lacks its two label
    // breaks.
    using Parts=std::vector<std::vector<std::vector<std::wstring>>>;
    static Parts parse(std::wstring const& value) {
        Parts cells;
        for(auto const& cell:split(value,CellBreak)) {
            auto& gauges=cells.emplace_back();
            for(auto const& gauge:split(cell,GaugeBreak)) {
                auto parts=split(gauge,LabelBreak);
                if(parts.size()!=3) return {};
                gauges.push_back(std::move(parts));
            }
        }
        return cells;
    }

    // The look as sent, checked against what the hook accepts. False when any
    // value is out of range.
    static bool styled(LONG const* look, Style& style) {
        const auto word=[&](size_t at) { return look[at-StyleBox]; };
        style.box=static_cast<uint32_t>(word(StyleBox));
        style.track=static_cast<uint32_t>(word(StyleTrack));
        style.font=word(StyleFont); style.weight=word(StyleWeight); style.height=word(StyleHeight);
        style.padding=word(StylePadding); style.gap=word(StyleGap); style.gauge=word(StyleGauge);
        style.gauge_gap=word(StyleGaugeGap); style.radius=word(StyleRadius);
        style.layout=word(StyleLayout); style.gauge_width=word(StyleGaugeWidth);
        style.cell_color=static_cast<uint32_t>(word(StyleCellColor));
        style.cell_border=static_cast<uint32_t>(word(StyleCellBorder));
        style.cell_radius=word(StyleCellRadius); style.cell_padding=word(StyleCellPadding);
        style.cell_inset=word(StyleCellInset); style.motion=word(StyleMotion);
        style.icon=word(StyleIcon); style.icon_gap=word(StyleIconGap); style.uniform=word(StyleUniform);
        style.fill_end=word(StyleFillEnd); style.text_end=word(StyleTextEnd); style.arrange=word(StyleArrange);
        style.spacing=word(StyleSpacing); style.label_gap=word(StyleLabelGap); style.ring=word(StyleRing);
        style.label_color=static_cast<uint32_t>(word(StyleLabelColor));
        style.mark_weight=word(StyleMarkWeight);
        const auto within=[](LONG value,LONG low,LONG high) { return value>=low && value<=high; };
        return within(style.font,FontMin,FontMax) && within(style.weight,WeightMin,WeightMax)
            && within(style.height,HeightMin,HeightMax) && within(style.padding,PaddingMin,PaddingMax)
            && within(style.gap,GapMin,GapMax) && within(style.gauge,GaugeMin,GaugeMax)
            && within(style.gauge_gap,GaugeGapMin,GaugeGapMax) && within(style.radius,RadiusMin,RadiusMax)
            && within(style.layout,LayoutMin,LayoutMax) && within(style.gauge_width,GaugeWidthMin,GaugeWidthMax)
            && within(style.cell_radius,CellRadiusMin,CellRadiusMax) && within(style.cell_padding,CellPaddingMin,CellPaddingMax)
            && within(style.cell_inset,CellInsetMin,CellInsetMax) && within(style.motion,MotionMin,MotionMax)
            && within(style.icon,IconMin,IconMax) && within(style.icon_gap,IconGapMin,IconGapMax)
            && within(style.uniform,UniformMin,UniformMax)
            && within(style.fill_end,FillEndMin,FillEndMax) && within(style.text_end,TextEndMin,TextEndMax)
            && within(style.arrange,ArrangeMin,ArrangeMax) && within(style.spacing,SpacingMin,SpacingMax)
            && within(style.label_gap,LabelGapMin,LabelGapMax) && within(style.ring,RingMin,RingMax)
            && within(style.mark_weight,MarkWeightMin,MarkWeightMax);
    }

    // Copies a scene's blocks and text pool as written. False when its size,
    // count or pool is out of range; each block is checked once the look,
    // and so the box's height, is known.
    bool read_scene(std::vector<Item>& items, LONG& width) {
        width=word(SceneWidth);
        const LONG count=word(SceneCount), pool_length=word(ScenePoolLength);
        if(width<SceneWidthMin || width>SceneWidthMax || count<1 || count>MaxItems
            || pool_length<0 || pool_length>ScenePoolCapacity) return false;
        auto units=reinterpret_cast<volatile const wchar_t*>(where(ScenePool));
        std::wstring pool(static_cast<size_t>(pool_length),L'\0');
        for(LONG index=0;index<pool_length;++index) pool[index]=units[index];
        for(LONG index=0;index<count;++index) {
            const auto field=[&](size_t at) { return word(SceneItems+index*SceneItemWords+at); };
            Item item;
            item.kind=field(0); item.x=field(1); item.y=field(2); item.w=field(3); item.h=field(4);
            item.fill=field(5); item.color=static_cast<uint32_t>(field(6)); item.track=static_cast<uint32_t>(field(7));
            item.size=field(8); item.weight=field(9); item.options=field(10);
            const LONG reference=field(11);
            if(item.kind==TextKind) {
                const LONG from=(reference>>16)&0xffff, length=reference&0xffff;
                if(from+length>pool_length) return false;
                item.text=pool.substr(static_cast<size_t>(from),static_cast<size_t>(length));
            } else if(item.kind==IconKind) item.slot=reference;
            items.push_back(std::move(item));
        }
        return true;
    }

    // Whether a block fits the box and its kind: inside the box, a fill in
    // permille or none, and what its kind needs.
    static bool fits(Item const& item, LONG width, LONG height) {
        const auto within=[](LONG value,LONG low,LONG high) { return value>=low && value<=high; };
        if(!within(item.x,0,width) || !within(item.y,0,height) || item.w<1 || item.h<1
            || item.w>width-item.x || item.h>height-item.y || !within(item.fill,-1,1000)) return false;
        switch(item.kind) {
        case TextKind: return within(item.size,FontMin,FontMax) && within(item.weight,WeightMin,WeightMax) && within(item.options,0,2)
            && static_cast<LONG>(item.text.size())<=ItemTextMax;
        case BarKind: case PieKind: return within(item.options,0,1);
        case RingKind: return within(item.size,ThicknessMin,ThicknessMax) && item.size*2<=std::min(item.w,item.h) && within(item.options,0,1);
        case IconKind: return within(item.slot,0,MaxCells-1);
        default: return false;
        }
    }

    // Copies the controller's label, or its scene, under its odd/even
    // sequence. A torn write is retried on the next tick; a label or scene
    // out of range, or a label whose two strings do not have the same cells
    // and gauges, is refused and the previous one kept. Nothing is cut to
    // fit.
    void take_text() {
        const LONG seq=word(TextSeq);
        if(seq==text_seq || (seq&1)) return;
        const bool scene=word(SceneMode)==1;
        const LONG text_length=word(TextLength), sizing_length=word(SizingLength);
        const bool sized=text_length>=1 && text_length<=TextCapacity && sizing_length>=1 && sizing_length<=TextCapacity;
        const auto copy=[&](size_t at,LONG length) {
            auto units=reinterpret_cast<volatile const wchar_t*>(where(at));
            std::wstring out(static_cast<size_t>(length),L'\0');
            for(LONG index=0;index<length;++index) out[index]=units[index];
            return out;
        };
        std::wstring text, sizing;
        if(sized && !scene) { text=copy(TextChars,text_length); sizing=copy(SizingChars,sizing_length); }
        LONG look[StyleMarkWeight+1-StyleBox]{}, fills[MaxGauges]{}, text_colors[MaxGauges]{}, fill_colors[MaxGauges]{}, ends[MaxGauges]{};
        for(size_t index=0;index<std::size(look);++index) look[index]=word(StyleBox+index);
        for(LONG index=0;index<MaxGauges;++index) {
            fills[index]=word(GaugeFills+index);
            text_colors[index]=word(GaugeTextColors+index);
            fill_colors[index]=word(GaugeFillColors+index);
            ends[index]=word(GaugeEnds+index);
        }
        Label next;
        LONG width=0;
        const bool read=scene && read_scene(next.items,width);
        if(word(TextSeq)!=seq) return;
        text_seq=seq;
        if(!styled(look,next.style)) return;
        if(scene) {
            if(!read) return;
            for(auto const& item:next.items) if(!fits(item,width,next.style.height)) return;
            next.scene=true;
            next.dip=Style::dip(width)+2*Style::dip(next.style.padding);
        } else {
            if(!sized) return;
            const auto texts=parse(text), widests=parse(sizing);
            if(texts.empty() || static_cast<LONG>(texts.size())>MaxCells || widests.size()!=texts.size()) return;
            LONG flat=0;
            for(size_t index=0;index<texts.size();++index) {
                auto const& shown=texts[index];
                auto const& widest=widests[index];
                if(shown.empty() || static_cast<LONG>(shown.size())>MaxCellGauges || widest.size()!=shown.size()) return;
                auto& cell=next.cells.emplace_back();
                for(size_t at=0;at<shown.size();++at,++flat) {
                    if(flat>=MaxGauges || widest[at][2].empty() || fills[flat]<-1 || fills[flat]>1000 || ends[flat]<0 || ends[flat]>1) return;
                    cell.push_back({shown[at][0],widest[at][0],shown[at][1],widest[at][1],shown[at][2],widest[at][2],fills[flat],
                        static_cast<uint32_t>(text_colors[flat]),static_cast<uint32_t>(fill_colors[flat]),0,ends[flat]==1});
                }
            }
            measure_label(next);
        }
        next.seq=seq;
        pending=std::move(next);
        // A newer label replacing a waiting one keeps the first one's wait.
        if(!has_pending) pending_since=shared_now();
        has_pending=true;
        InterlockedExchange(where(TextApplied),seq);
    }

    // Copies the icons under their own odd/even sequence. A torn write is
    // retried on the next tick; a size out of range is refused and the
    // previous icons kept.
    void take_icons() {
        const LONG seq=word(IconSeq);
        if(seq==icon_seq || (seq&1)) return;
        Icon next[MaxCells];
        bool sized=true;
        for(LONG index=0;index<MaxCells && sized;++index) {
            const LONG size=word(IconSizes+index);
            if(size<0 || size>IconMaxPx) { sized=false; break; }
            auto pixels=reinterpret_cast<volatile const uint32_t*>(where(IconPixels+index*IconBlock));
            next[index].size=size;
            next[index].pixels.resize(static_cast<size_t>(size)*static_cast<size_t>(size));
            for(size_t at=0;at<next[index].pixels.size();++at) next[index].pixels[at]=pixels[at];
        }
        if(word(IconSeq)!=seq) return;
        icon_seq=seq;
        if(!sized) return;
        for(LONG index=0;index<MaxCells;++index) icons[index]=std::move(next[index]);
        icons_changed=true;
        InterlockedExchange(where(IconTaken),seq);
    }

    double text_width(std::wstring const& value, Style const& style, LONG weight) {
        return text_width(value,style.font,weight);
    }
    double text_width(std::wstring const& value, LONG size, LONG weight) {
        if(value.empty()) return 0;
        font(measurer,size,weight);
        measurer.Text(value);
        measurer.Measure({10000,10000});
        const double width=std::ceil(measurer.DesiredSize().Width);
        if(!std::isfinite(width)||width<0) winrt::throw_hresult(E_FAIL);
        // A text wider than any box is cut where it is drawn, and counted;
        // it does not end the session.
        return std::min(width,2048.0);
    }

    double text_width(std::wstring const& value, Style const& style) {
        return text_width(value,style,style.weight);
    }

    // A text's characters, a surrogate pair kept whole.
    static std::vector<std::wstring> characters(std::wstring const& text) {
        std::vector<std::wstring> out;
        for(size_t at=0;at<text.size();) {
            const bool pair=at+1<text.size() && text[at]>=0xd800 && text[at]<=0xdbff
                && text[at+1]>=0xdc00 && text[at+1]<=0xdfff;
            out.push_back(text.substr(at,pair?2:1));
            at+=pair?2:1;
        }
        return out;
    }

    // A warning mark's width as it is drawn: a character at a time, in the
    // mark's weight.
    double mark_width(std::wstring const& mark, Style const& style) {
        double width=0;
        for(auto const& character:characters(mark)) width+=text_width(character,style,style.mark_weight);
        return width;
    }

    // A gauge's text: its label column, the gap after a label, and the value
    // column.
    static double text_part(Style const& style,Gauge const& gauge,double values) {
        return gauge.name+(gauge.name>0?Style::dip(style.label_gap):0)+values;
    }

    // Measures each gauge's widest label, mark and value with the font the
    // cells render with, in XAML DIP, at the current scale; the box and the
    // reservation are sized by them. A cell's marks share one column and its
    // numbers another, which with the gap between make its value column; its
    // labels share one too when its gauges are stacked, so that values and
    // bars line up. A cell adds its icon, its gauges' bars or dials beside the
    // text, the space between gauges, and its card's padding and border.
    void measure_label(Label& label) {
        auto const& style=label.style;
        label.marks.clear();
        label.numbers.clear();
        label.values.clear();
        label.widths.clear();
        for(auto& cell:label.cells) {
            double names=0, marks=0, numbers=0;
            for(auto& gauge:cell) {
                gauge.name=text_width(gauge.label_widest,style);
                names=std::max(names,gauge.name);
                marks=std::max(marks,mark_width(gauge.mark_widest,style));
                numbers=std::max(numbers,text_width(gauge.value_widest,style));
            }
            if(!style.side()) for(auto& gauge:cell) gauge.name=names;
            label.marks.push_back(marks);
            label.numbers.push_back(numbers);
        }
        // Every cell as wide as the widest, so each service takes the same
        // room.
        if(style.uniform) {
            const double marks=*std::max_element(label.marks.begin(),label.marks.end());
            const double numbers=*std::max_element(label.numbers.begin(),label.numbers.end());
            std::fill(label.marks.begin(),label.marks.end(),marks);
            std::fill(label.numbers.begin(),label.numbers.end(),numbers);
            if(!style.side()) {
                double names=0;
                for(auto const& cell:label.cells) names=std::max(names,cell.front().name);
                for(auto& cell:label.cells) for(auto& gauge:cell) gauge.name=names;
            }
        }
        for(size_t index=0;index<label.cells.size();++index) {
            const double marks=label.marks[index];
            label.values.push_back(marks+(marks>0?Style::dip(style.label_gap):0)+label.numbers[index]);
        }
        const double part=style.layout!=0?Style::dip(style.gauge_gap)+Style::dip(style.gauge_width):0;
        for(size_t index=0;index<label.cells.size();++index) {
            auto const& cell=label.cells[index];
            const double values=label.values[index];
            double content=0;
            if(style.side()) {
                for(auto const& gauge:cell) content+=text_part(style,gauge,values)+part;
                content+=Style::dip(style.spacing)*static_cast<double>(cell.size()-1);
            } else content=text_part(style,cell.front(),values)+part;
            label.widths.push_back(style.icon_part()+content+2*Style::dip(style.cell_padding)+2*style.border());
        }
        if(style.uniform) {
            const double widest=*std::max_element(label.widths.begin(),label.widths.end());
            std::fill(label.widths.begin(),label.widths.end(),widest);
        }
        double dip=2*Style::dip(style.padding)+Style::dip(style.gap)*static_cast<double>(label.cells.size()-1);
        for(const double width:label.widths) dip+=width;
        label.dip=dip;
        state->add(LabelMeasures,1);
    }

    static void set_width(x::FrameworkElement const& element,double width) {
        if(!(std::abs(element.Width()-width)<=.1)) element.Width(width);
    }
    static void set_left(x::FrameworkElement const& element,double left) {
        if(!(std::abs(element.Margin().Left-left)<=.1)) element.Margin({left,0,0,0});
    }
    static void set_right(x::FrameworkElement const& element,double right) {
        if(!(std::abs(element.Margin().Right-right)<=.1)) element.Margin({0,0,right,0});
    }
    static m::SolidColorBrush brush(uint32_t argb) {
        return m::SolidColorBrush(winrt::Windows::UI::Color{static_cast<uint8_t>(argb>>24),
            static_cast<uint8_t>(argb>>16),static_cast<uint8_t>(argb>>8),static_cast<uint8_t>(argb)});
    }
    static x::CornerRadius corners(double radius) { return {radius,radius,radius,radius}; }
    // A text's box runs from the capitals' height to the baseline rather than
    // from the font's ascent to its descent, so centering it centers the
    // digits, as the icon, dial and bar are. Widths do not change.
    static void font(c::TextBlock const& text, LONG size, LONG weight) {
        text.TextLineBounds(x::TextLineBounds::Tight);
        text.FontSize(Style::dip(size));
        text.FontWeight(winrt::Windows::UI::Text::FontWeight{static_cast<uint16_t>(weight)});
        text.IsTextScaleFactorEnabled(false);
    }
    static void font(c::TextBlock const& text, Style const& style, LONG weight) { font(text,style.font,weight); }
    static void font(c::TextBlock const& text, Style const& style) { font(text,style,style.weight); }

    // One cell: a card, invisible unless it has a color or border, holding the
    // service's icon and its gauges, stacked, side by side, or as rings on one
    // dial. With no fill only the track shows.
    Cell make_cell(size_t index) {
        auto const& style=drawn.style;
        const double inset=Style::dip(style.cell_inset);
        const double inner=Style::dip(style.cell_padding), border=style.border();
        Cell cell;
        cell.root=c::Border();
        cell.root.Margin({index>0?Style::dip(style.gap):0,inset,0,inset});
        cell.root.Padding({inner,0,inner,0});
        cell.root.CornerRadius(corners(Style::dip(style.cell_radius)));
        cell.root.Background(brush(style.cell_color));
        cell.root.BorderBrush(brush(style.cell_border));
        cell.root.BorderThickness({border,border,border,border});
        auto line=c::StackPanel();
        line.Orientation(c::Orientation::Horizontal);
        line.VerticalAlignment(x::VerticalAlignment::Center);
        if(style.icon>0) {
            const double icon=Style::dip(style.icon);
            cell.icon=c::Image();
            cell.icon.Width(icon);
            cell.icon.Height(icon);
            cell.icon.Stretch(m::Stretch::Uniform);
            cell.icon.VerticalAlignment(x::VerticalAlignment::Center);
            cell.icon.Margin({0,0,Style::dip(style.icon_gap),0});
            line.Children().Append(cell.icon);
        }
        for(auto const& gauge:drawn.cells[index]) cell.meters.push_back(make_meter(gauge));
        auto gauges=c::StackPanel();
        gauges.VerticalAlignment(x::VerticalAlignment::Center);
        gauges.Spacing(Style::dip(style.spacing));
        if(style.rings()) {
            // One dial with a ring per gauge from the outside in, and the texts
            // stacked beside it in the same order.
            const double diameter=Style::dip(style.gauge_width), thickness=Style::dip(style.ring);
            auto dial=c::Grid();
            dial.Width(diameter);
            dial.Height(diameter);
            dial.VerticalAlignment(x::VerticalAlignment::Center);
            dial.Margin({0,0,Style::dip(style.gauge_gap),0});
            for(size_t at=0;at<cell.meters.size();++at) {
                auto& meter=cell.meters[at];
                gauges.Children().Append(meter.text);
                const double radius=ring_radius(style,at);
                if(radius<=0) continue;
                shapes::Ellipse track;
                track.Width(2*radius+thickness);
                track.Height(2*radius+thickness);
                track.Stroke(brush(style.track));
                track.StrokeThickness(thickness);
                dial.Children().Append(track);
                meter.wedge=shapes::Path();
                meter.wedge.Stroke(brush(meter.fill_color));
                meter.wedge.StrokeThickness(thickness);
                meter.wedge.StrokeStartLineCap(m::PenLineCap::Round);
                meter.wedge.StrokeEndLineCap(m::PenLineCap::Round);
                dial.Children().Append(meter.wedge);
            }
            line.Children().Append(dial);
        } else {
            if(style.side()) gauges.Orientation(c::Orientation::Horizontal);
            for(auto& meter:cell.meters) gauges.Children().Append(make_unit(meter));
        }
        line.Children().Append(gauges);
        cell.root.Child(line);
        row.Children().Append(cell.root);
        return cell;
    }

    // A gauge's text: its label at the start, in the labels' color, its value
    // in the value column, at the end or the start, and a warning's mark before
    // the value, both in the gauge's own color, which alone shows a warning or
    // a stale value. Widths and places are set by apply_widths.
    Meter make_meter(Gauge const& gauge) {
        auto const& style=drawn.style;
        Meter meter;
        meter.text=c::Grid();
        meter.text.VerticalAlignment(x::VerticalAlignment::Center);
        meter.label=c::TextBlock();
        font(meter.label,style);
        meter.label.HorizontalAlignment(x::HorizontalAlignment::Left);
        meter.label.VerticalAlignment(x::VerticalAlignment::Center);
        meter.label.Foreground(brush(style.label_color));
        meter.value=c::TextBlock();
        font(meter.value,style);
        meter.value.HorizontalAlignment(x::HorizontalAlignment::Left);
        meter.value.VerticalAlignment(x::VerticalAlignment::Center);
        meter.value.TextAlignment(style.text_end?x::TextAlignment::Right:x::TextAlignment::Left);
        meter.text_color=gauge.text_color;
        meter.value.Foreground(brush(meter.text_color));
        meter.fill_color=gauge.fill_color;
        meter.end=gauge.end;
        meter.marks=c::StackPanel();
        meter.marks.Orientation(c::Orientation::Horizontal);
        meter.marks.HorizontalAlignment(x::HorizontalAlignment::Right);
        meter.marks.VerticalAlignment(x::VerticalAlignment::Center);
        meter.text.Children().Append(meter.label);
        meter.text.Children().Append(meter.value);
        meter.text.Children().Append(meter.marks);
        return meter;
    }

    // Draws a warning's mark a character at a time, heavier than the digits.
    // Each character starts on a whole pixel, so all look alike, rather than
    // one landing between pixels and blurring.
    void set_mark(Meter& meter,std::wstring const& mark) {
        auto const& style=drawn.style;
        meter.mark=mark;
        meter.marks.Children().Clear();
        const auto paint=brush(meter.text_color);
        for(auto const& character:characters(mark)) {
            c::TextBlock text;
            font(text,style,style.mark_weight);
            text.VerticalAlignment(x::VerticalAlignment::Center);
            text.Foreground(paint);
            text.Text(character);
            meter.marks.Children().Append(text);
        }
    }

    // One gauge and its text as the layout places them: a bar below or beside
    // the text, or a dial before it.
    c::StackPanel make_unit(Meter& meter) {
        auto const& style=drawn.style;
        auto panel=c::StackPanel();
        panel.VerticalAlignment(x::VerticalAlignment::Center);
        if(style.dial()) {
            const double diameter=Style::dip(style.gauge_width);
            auto dial=c::Grid();
            dial.Width(diameter);
            dial.Height(diameter);
            dial.VerticalAlignment(x::VerticalAlignment::Center);
            dial.Margin({0,0,Style::dip(style.gauge_gap),0});
            shapes::Ellipse disc;
            disc.Fill(brush(style.track));
            dial.Children().Append(disc);
            meter.wedge=shapes::Path();
            meter.wedge.Fill(brush(meter.fill_color));
            dial.Children().Append(meter.wedge);
            panel.Orientation(c::Orientation::Horizontal);
            panel.Children().Append(dial);
            panel.Children().Append(meter.text);
            return panel;
        }
        const double gauge=Style::dip(style.gauge);
        meter.track=c::Border();
        meter.track.Height(gauge);
        meter.track.VerticalAlignment(x::VerticalAlignment::Center);
        meter.track.HorizontalAlignment(x::HorizontalAlignment::Left);
        meter.track.CornerRadius(corners(gauge/2));
        meter.track.Background(brush(style.track));
        meter.fill=c::Border();
        meter.fill.Width(0);
        meter.fill.HorizontalAlignment(meter.end?x::HorizontalAlignment::Right:x::HorizontalAlignment::Left);
        meter.fill.CornerRadius(corners(gauge/2));
        meter.fill.Background(brush(meter.fill_color));
        meter.track.Child(meter.fill);
        if(style.beside()) {
            panel.Orientation(c::Orientation::Horizontal);
            meter.track.Margin({Style::dip(style.gauge_gap),0,0,0});
        } else meter.track.Margin({0,Style::dip(style.gauge_gap),0,0});
        panel.Children().Append(meter.text);
        panel.Children().Append(meter.track);
        return panel;
    }

    // Moves a fill to its width, over `motion` ms when that is set: short,
    // decelerating, and only when the width changes. The width moves rather
    // than a scale, so the rounded ends keep their shape at small values.
    static void fill_to(Meter& meter,double target,LONG motion) {
        const double from=meter.fill.Width();
        if(std::abs(from-target)<=.1) return;
        if(meter.story) { meter.story.Stop(); meter.story=nullptr; }
        meter.fill.Width(target);
        if(motion<=0 || !std::isfinite(from)) return;
        anim::DoubleAnimation move;
        move.From(from);
        move.To(target);
        move.Duration(x::DurationHelper::FromTimeSpan(std::chrono::milliseconds(motion)));
        move.EnableDependentAnimation(true);
        anim::CubicEase ease;
        ease.EasingMode(anim::EasingMode::EaseOut);
        move.EasingFunction(ease);
        anim::Storyboard story;
        story.Children().Append(move);
        anim::Storyboard::SetTarget(move,meter.fill);
        anim::Storyboard::SetTargetProperty(move,L"Width");
        story.Begin();
        meter.story=story;
    }

    // Copies a cell's icon into its bitmap. The pixels arrive premultiplied
    // BGRA at the size shown, so nothing is scaled.
    void apply_icon(size_t index) {
        auto& cell=cells[index];
        if(!cell.icon) return;
        auto const& icon=icons[index];
        if(icon.size<=0) { cell.icon.Visibility(x::Visibility::Collapsed); return; }
        if(!cell.bitmap || cell.bitmap.PixelWidth()!=icon.size) {
            cell.bitmap=imaging::WriteableBitmap(icon.size,icon.size);
            cell.icon.Source(cell.bitmap);
        }
        std::memcpy(cell.bitmap.PixelBuffer().data(),icon.pixels.data(),icon.pixels.size()*sizeof(uint32_t));
        cell.bitmap.Invalidate();
        cell.icon.Visibility(x::Visibility::Visible);
    }

    // A ring's radius, to the middle of its stroke, the first gauge outermost.
    static double ring_radius(Style const& style,size_t index) {
        const double thickness=Style::dip(style.ring);
        return Style::dip(style.gauge_width)/2-thickness/2-static_cast<double>(index)*(thickness+Style::dip(style.spacing));
    }

    // From twelve o'clock as far as the fill goes, clockwise, or counter-
    // clockwise when the colored part sits against the end, around (center,
    // center): a filled wedge for a dial, an open arc for a ring. A whole
    // circle at 1000, nothing at 0 or when the value is not known.
    static void draw_arc(Meter& meter,float center,float r,LONG fill,bool end,bool wedge) {
        if(!meter.wedge || fill==meter.wedge_fill) return;
        meter.wedge_fill=fill;
        if(fill<=0 || r<=0) { meter.wedge.Data(nullptr); return; }
        if(fill>=1000) {
            m::EllipseGeometry whole;
            whole.Center({center,center});
            whole.RadiusX(r);
            whole.RadiusY(r);
            meter.wedge.Data(whole);
            return;
        }
        const double angle=2*std::numbers::pi*fill/1000.0;
        m::PathFigure figure;
        if(wedge) {
            figure.StartPoint({center,center});
            m::LineSegment up;
            up.Point({center,center-r});
            figure.Segments().Append(up);
        } else figure.StartPoint({center,center-r});
        figure.IsClosed(wedge);
        figure.IsFilled(wedge);
        m::ArcSegment arc;
        const float across=r*static_cast<float>(std::sin(angle));
        arc.Point({end?center-across:center+across,center-r*static_cast<float>(std::cos(angle))});
        arc.Size({r,r});
        arc.IsLargeArc(angle>std::numbers::pi);
        arc.SweepDirection(end?m::SweepDirection::Counterclockwise:m::SweepDirection::Clockwise);
        figure.Segments().Append(arc);
        m::PathGeometry geometry;
        geometry.Figures().Append(figure);
        meter.wedge.Data(geometry);
    }

    // Takes the label's cells and a scene's blocks off the box.
    void clear_cells() {
        for(auto& cell:cells) for(auto& meter:cell.meters) if(meter.story) meter.story.Stop();
        cells.clear();
        if(row) row.Children().Clear();
        for(auto& block:blocks) if(block.meter.story) block.meter.story.Stop();
        blocks.clear();
        if(stage) stage.Children().Clear();
    }

    // Copies each icon slot into its bitmap and shows it in every icon block
    // of that slot; a slot with no icon hides its blocks.
    void apply_slots() {
        for(LONG slot=0;slot<MaxCells;++slot) {
            auto const& icon=icons[slot];
            if(icon.size<=0) { slots[slot]=nullptr; continue; }
            if(!slots[slot] || slots[slot].PixelWidth()!=icon.size) slots[slot]=imaging::WriteableBitmap(icon.size,icon.size);
            std::memcpy(slots[slot].PixelBuffer().data(),icon.pixels.data(),icon.pixels.size()*sizeof(uint32_t));
            slots[slot].Invalidate();
        }
        for(size_t index=0;index<blocks.size();++index) {
            auto& block=blocks[index];
            if(!block.image) continue;
            auto const& bitmap=slots[drawn.items[index].slot];
            block.image.Source(bitmap);
            block.image.Visibility(bitmap?x::Visibility::Visible:x::Visibility::Collapsed);
        }
    }

    // One block of a scene, on the stage: a text, a bar's track and fill, a
    // ring's track and arc, a pie's disc and wedge, or an icon. Its place,
    // size and values are set by place_block.
    Block make_block(Item const& item) {
        auto const& style=drawn.style;
        Block block;
        switch(item.kind) {
        case TextKind: {
            auto box=c::Grid();
            block.text=c::TextBlock();
            block.text.VerticalAlignment(x::VerticalAlignment::Center);
            block.text.TextWrapping(x::TextWrapping::NoWrap);
            block.text.TextAlignment(item.options==1?x::TextAlignment::Center:item.options==2?x::TextAlignment::Right:x::TextAlignment::Left);
            box.Children().Append(block.text);
            block.root=box;
            break;
        }
        case BarKind: {
            block.meter.track=c::Border();
            block.meter.fill=c::Border();
            block.meter.fill.Width(0);
            block.meter.fill.HorizontalAlignment(item.options?x::HorizontalAlignment::Right:x::HorizontalAlignment::Left);
            block.meter.track.Child(block.meter.fill);
            block.root=block.meter.track;
            break;
        }
        case RingKind: case PieKind: {
            auto dial=c::Grid();
            block.track=shapes::Ellipse();
            dial.Children().Append(block.track);
            block.meter.wedge=shapes::Path();
            if(item.kind==RingKind) {
                block.meter.wedge.StrokeStartLineCap(m::PenLineCap::Round);
                block.meter.wedge.StrokeEndLineCap(m::PenLineCap::Round);
            }
            dial.Children().Append(block.meter.wedge);
            block.root=dial;
            break;
        }
        default: {
            block.image=c::Image();
            block.image.Stretch(m::Stretch::Uniform);
            block.root=block.image;
            break;
        }
        }
        block.meter.fill_color=item.color^1;
        block.meter.text_color=item.color^1;
        block.track_color=(item.track?item.track:style.track)^1;
        stage.Children().Append(block.root);
        return block;
    }

    // Puts a block where the scene says and gives it its values, changing
    // only what differs; a ring or pie is square, centered in its place, and
    // is drawn again when its size changes. A text wider than its place is
    // cut off by it and counted.
    void place_block(Block& block, Item const& item) {
        auto const& style=drawn.style;
        double left=Style::dip(item.x), top=Style::dip(item.y);
        const double w=Style::dip(item.w), h=Style::dip(item.h);
        const uint32_t track=item.track?item.track:style.track;
        const bool end=(item.options&1)!=0;
        switch(item.kind) {
        case TextKind: {
            set_width(block.root,w);
            if(!(std::abs(block.root.Height()-h)<=.1)) block.root.Height(h);
            if(block.size!=item.size || block.weight!=item.weight) {
                font(block.text,item.size,item.weight);
                block.size=item.size; block.weight=item.weight;
            }
            if(std::wstring_view{block.text.Text()}!=item.text) block.text.Text(item.text);
            if(block.meter.text_color!=item.color) { block.meter.text_color=item.color; block.text.Foreground(brush(item.color)); }
            if(text_width(item.text,item.size,item.weight)>w+.5) state->add(base+TextClipped,1);
            break;
        }
        case BarKind: {
            set_width(block.root,w);
            if(!(std::abs(block.root.Height()-h)<=.1)) {
                block.root.Height(h);
                block.meter.track.CornerRadius(corners(h/2));
                block.meter.fill.CornerRadius(corners(h/2));
            }
            if(block.track_color!=track) { block.track_color=track; block.meter.track.Background(brush(track)); }
            if(block.meter.fill_color!=item.color) { block.meter.fill_color=item.color; block.meter.fill.Background(brush(item.color)); }
            fill_to(block.meter,item.fill<0?0:w*item.fill/1000.0,style.motion);
            break;
        }
        case RingKind: case PieKind: {
            const double side=std::min(w,h);
            left+=(w-side)/2; top+=(h-side)/2;
            const bool ring=item.kind==RingKind;
            const double thickness=ring?Style::dip(item.size):0;
            if(std::abs(block.side-side)>.01 || block.size!=item.size) {
                block.side=side; block.size=item.size;
                block.root.Width(side); block.root.Height(side);
                block.meter.wedge_fill=-2;
                if(ring) { block.track.StrokeThickness(thickness); block.meter.wedge.StrokeThickness(thickness); }
            }
            if(block.track_color!=track) {
                block.track_color=track;
                if(ring) block.track.Stroke(brush(track)); else block.track.Fill(brush(track));
            }
            if(block.meter.fill_color!=item.color) {
                block.meter.fill_color=item.color;
                if(ring) block.meter.wedge.Stroke(brush(item.color)); else block.meter.wedge.Fill(brush(item.color));
            }
            const float center=static_cast<float>(side/2);
            draw_arc(block.meter,center,static_cast<float>(ring?side/2-thickness/2:side/2),item.fill,end,!ring);
            break;
        }
        default:
            set_width(block.root,w);
            if(!(std::abs(block.root.Height()-h)<=.1)) block.root.Height(h);
            break;
        }
        if(!(std::abs(c::Canvas::GetLeft(block.root)-left)<=.1)) c::Canvas::SetLeft(block.root,left);
        if(!(std::abs(c::Canvas::GetTop(block.root)-top)<=.1)) c::Canvas::SetTop(block.root,top);
    }

    // Puts a scene on the box: new blocks when their kinds changed, then each
    // block's place and values.
    void draw_scene(bool reshape) {
        if(reshape) {
            clear_cells();
            for(auto const& item:drawn.items) blocks.push_back(make_block(item));
            icons_changed=true;
        }
        if(icons_changed) { apply_slots(); icons_changed=false; }
        for(size_t index=0;index<blocks.size();++index) place_block(blocks[index],drawn.items[index]);
        InterlockedExchange(state->words+width_word,static_cast<LONG>(std::lround(drawn.dip)));
    }

    // Sets each cell's width, each gauge's label, number and mark columns, and
    // its fill. A gauge beside its text starts in the same place whatever the
    // value, and a value at the end ends where the digits do. A mark sits a
    // gap before the digits: before the number column when values start it, so
    // every number starts in one place, marked or not, or before the digits
    // themselves when values end it.
    void apply_widths() {
        auto const& style=drawn.style;
        const float center=static_cast<float>(Style::dip(style.gauge_width)/2);
        const double gap=Style::dip(style.label_gap);
        for(size_t index=0;index<cells.size();++index) {
            auto& cell=cells[index];
            set_width(cell.root,drawn.widths[index]);
            const double values=drawn.values[index], numbers=drawn.numbers[index];
            for(size_t at=0;at<cell.meters.size();++at) {
                auto& meter=cell.meters[at];
                auto const& gauge=drawn.cells[index][at];
                const double text=text_part(style,gauge,values);
                set_width(meter.text,text);
                set_width(meter.label,gauge.name);
                set_width(meter.value,numbers);
                set_left(meter.value,text-numbers);
                const double digits=style.text_end?std::min(numbers,text_width(gauge.value,style)):numbers;
                set_right(meter.marks,digits+gap);
                if(style.rings()) {
                    draw_arc(meter,center,static_cast<float>(ring_radius(style,at)),gauge.fill,gauge.end,false);
                    continue;
                }
                if(style.dial()) {
                    draw_arc(meter,center,center,gauge.fill,gauge.end,true);
                    continue;
                }
                const double track=style.beside()?Style::dip(style.gauge_width):text;
                set_width(meter.track,track);
                fill_to(meter,gauge.fill<0?0:track*gauge.fill/1000.0,style.motion);
            }
        }
        InterlockedExchange(state->words+width_word,static_cast<LONG>(std::lround(drawn.dip)));
    }

    // Puts the pending label on screen. The cells are rebuilt only when the
    // look, or the number of cells or of a cell's gauges, changes; otherwise
    // only what differs is assigned. A text wider than its column is cut off by
    // it; the cell does not grow after it, which would move the apps on every
    // value, so this only counts it.
    void draw() {
        const bool restyle=drawn.empty() || !(pending.style==drawn.style);
        bool reshape=restyle || pending.scene!=drawn.scene || pending.cells.size()!=drawn.cells.size()
            || pending.items.size()!=drawn.items.size();
        // A gauge's end is set when it is made.
        for(size_t index=0;!reshape && index<pending.cells.size();++index) {
            reshape=pending.cells[index].size()!=drawn.cells[index].size();
            for(size_t at=0;!reshape && at<pending.cells[index].size();++at)
                reshape=pending.cells[index][at].end!=drawn.cells[index][at].end;
        }
        // A block is made again when its kind, or what it was made with,
        // changes.
        for(size_t index=0;!reshape && index<pending.items.size();++index)
            reshape=pending.items[index].kind!=drawn.items[index].kind || pending.items[index].options!=drawn.items[index].options;
        drawn=std::move(pending);
        pending=Label{};
        has_pending=false;
        auto const& style=drawn.style;
        if(restyle) {
            const double padding=Style::dip(style.padding);
            widget.Height(Style::dip(style.height));
            widget.Padding({padding,0,padding,0});
            widget.CornerRadius(corners(Style::dip(style.radius)));
            widget.Background(brush(style.box));
        }
        if(drawn.scene) {
            draw_scene(reshape);
            InterlockedExchange(where(TextDrawn),drawn.seq);
            return;
        }
        // A new look rebuilds the cells; it is rare, and simpler than
        // restyling each part in place.
        if(reshape) {
            clear_cells();
            for(size_t index=0;index<drawn.cells.size();++index) cells.push_back(make_cell(index));
            icons_changed=true;
        }
        if(icons_changed) {
            for(size_t index=0;index<cells.size();++index) apply_icon(index);
            icons_changed=false;
        }
        apply_widths();
        for(size_t index=0;index<cells.size();++index) {
            for(size_t at=0;at<cells[index].meters.size();++at) {
                auto& meter=cells[index].meters[at];
                auto const& gauge=drawn.cells[index][at];
                if(std::wstring_view{meter.label.Text()}!=gauge.label) meter.label.Text(gauge.label);
                if(std::wstring_view{meter.value.Text()}!=gauge.value) meter.value.Text(gauge.value);
                if(meter.text_color!=gauge.text_color) {
                    meter.text_color=gauge.text_color;
                    const auto paint=brush(meter.text_color);
                    meter.value.Foreground(paint);
                    for(auto const& child:meter.marks.Children()) child.as<c::TextBlock>().Foreground(paint);
                }
                if(meter.mark!=gauge.mark) set_mark(meter,gauge.mark);
                if(meter.fill_color!=gauge.fill_color) {
                    meter.fill_color=gauge.fill_color;
                    if(meter.fill) meter.fill.Background(brush(meter.fill_color));
                    else if(meter.wedge && style.rings()) meter.wedge.Stroke(brush(meter.fill_color));
                    else if(meter.wedge) meter.wedge.Fill(brush(meter.fill_color));
                }
                if(text_width(gauge.value,style)>drawn.numbers[index]+.5 || mark_width(gauge.mark,style)>drawn.marks[index]+.5
                    || text_width(gauge.label,style)>gauge.name+.5)
                    state->add(base+TextClipped,1);
            }
        }
        InterlockedExchange(where(TextDrawn),drawn.seq);
    }

    // Gives the tray back: the taskbar left the supported shape, or the
    // controller asked for the box to go. Reserved again once that changes back.
    void release() {
        restore_space(Restored);
        reserve_done=false;
        reserve_attempts=0;
        settled_dip=0;
        clearing=false;
    }

    // Where the controller asks for box `at`: the first beside the tray
    // unless asked for another place, any other nowhere unless asked. Boxes
    // in one place go side by side by rank, then by number.
    static bool known(LONG asked) { return asked==LeftEnd || asked==Tray || asked==BeforeStart || asked==AfterApps; }
    Place asked(size_t at) const {
        const LONG word=state->read(BoxPlaces+at);
        return known(word)?static_cast<Place>(word):at==0?Tray:Nowhere;
    }
    using Boxes=std::vector<Context*>;
    Boxes boxes_at(Place slot, Place const* where) {
        Boxes found;
        for(size_t at=0;at<MaxBoxes;++at)
            if(where[at]==slot && box_at(at)) found.push_back(box_at(at));
        std::stable_sort(found.begin(),found.end(),[&](Context const* a, Context const* b) {
            return state->read(BoxRanks+a->box_index)<state->read(BoxRanks+b->box_index);
        });
        return found;
    }
    static bool has_any(Context const& box) { return !box.drawn.empty() || box.has_pending; }
    // A box's width as drawn, and once its waiting label is drawn too.
    static double drawn_dip(Context const& box) { return box.drawn.empty()?0:box.drawn.dip; }
    static double wanted_dip(Context const& box) { return std::max(drawn_dip(box),box.has_pending?box.pending.dip:0.); }
    // The width a row of boxes takes side by side, as drawn or as wanted.
    static double row_dip(Boxes const& boxes, bool wanted) {
        double sum=0;
        int count=0;
        for(auto const* box:boxes)
            if(const double dip=wanted?wanted_dip(*box):drawn_dip(*box); dip>0) { sum+=dip; ++count; }
        return sum+(count>1?BoxGap*(count-1):0);
    }
    // Puts a box at `left` in the root's DIPs, centered across the taskbar.
    // Only changed geometry is assigned, to avoid a LayoutUpdated loop. A box
    // being dragged, or just dropped, stays where the pointer left it.
    static bool put(Context& box, double left, x::FrameworkElement const& root, f::Point origin) {
        const double top=(root.ActualHeight()-Style::dip(box.drawn.style.height))/2;
        if(top<0) return false;
        if(box.pinned()) return true;
        const auto margin=box.widget.Margin();
        if(std::abs(margin.Left-(left-origin.X))>.1 || std::abs(margin.Top-(top-origin.Y))>.1)
            box.widget.Margin({left-origin.X,top-origin.Y,0,0});
        set_width(box.widget,box.drawn.dip);
        return true;
    }
    // Puts a row of boxes side by side from `left`; false when one could not
    // be put.
    static bool put_row(Boxes const& boxes, double left, x::FrameworkElement const& root, f::Point origin) {
        for(auto* box:boxes) {
            if(drawn_dip(*box)<=0) continue;
            if(!put(*box,left,root,origin)) return false;
            left+=drawn_dip(*box)+BoxGap;
        }
        return true;
    }

    // One tick of placement. Each box goes where it is asked, and the boxes
    // in one place side by side: those at the left end and before the Start
    // button on the taskbar repeater's left margin, those after the last app
    // on its right margin, those beside the tray on the tray's padding. Why
    // each box is hidden is left in `hides`; the first's is the answer.
    Hide hides[MaxBoxes]{NotRequested,NotRequested,NotRequested,NotRequested};
    Hide arrange(Measured const& measured) {
        Place where[MaxBoxes];
        for(size_t at=0;at<MaxBoxes;++at) {
            const Place wanted=asked(at);
            Context* b=box_at(at);
            where[at]=b && (at==0 || (wanted!=Nowhere && has_any(*b)))?wanted:Nowhere;
            hides[at]=wanted==Nowhere?NotRequested:NoText;
        }
        if(state->read(RequestWord)!=Full) {
            if(reserved()) release();
            release_left();
            for(auto& hide:hides) hide=NotRequested;
            return NotRequested;
        }
        const Boxes ends=boxes_at(LeftEnd,where), starts=boxes_at(BeforeStart,where);
        const Boxes afters=boxes_at(AfterApps,where), trays=boxes_at(Tray,where);
        const bool margins=!ends.empty() || !starts.empty() || !afters.empty();
        if(!margins) release_left();
        if(drawn.empty() && !has_pending) return hides[0]=NoText;
        if(!measured.ok) {
            const LONG why=state->read(MeasureFailure);
            if(why==NotShellWindow || why==NotPrimary || why==NotBottom) {
                if(reserved()) release();
                release_left();
            }
            for(size_t at=0;at<MaxBoxes;++at) if(where[at]!=Nowhere) hides[at]=NoMeasure;
            return hides[0];
        }
        const auto set=[&](Context const* b, Hide why) { hides[b->box_index]=why; };
        if(margins) arrange_repeater(measured,ends,starts,afters,set);
        if(trays.empty()) {
            if(reserved()) release();
        } else {
            const Hide hide=arrange_tray(measured,trays);
            for(auto const* b:trays) set(b,hide==Shown && drawn_dip(*b)<=0?NoText:hide);
        }
        return hides[0];
    }

    // Beside the tray: reserve the strip (retried for 10 s while the tray
    // assembles) for the boxes there, keep them over it, and draw a wider
    // label only once the apps have stayed out of the whole strip. Right
    // after the padding changes the apps can still reach into the new part
    // for a tick or, on a crowded taskbar, 1.5 s.
    Hide arrange_tray(Measured const& measured, std::vector<Context*> const& boxes) {
        // The part of the strip the boxes take, one of them `grown` to `dip`.
        const auto content=[&](Context const* grown, double dip) {
            double sum=0;
            int count=0;
            for(auto const* box:boxes) {
                const double width=box==grown?dip:drawn_dip(*box);
                if(width>0) { sum+=width; ++count; }
            }
            return sum+(count>1?BoxGap*(count-1):0);
        };
        // A new value, or a narrower format, fits the part known to be clear.
        for(auto* box:boxes)
            if(box->has_pending && content(box,box->pending.dip)<=settled_dip+.5) box->draw();
        double width=0;
        int count=0;
        for(auto const* box:boxes)
            if(const double dip=wanted_dip(*box); dip>0) { width+=dip; ++count; }
        width+=count>1?BoxGap*(count-1):0;
        if(!reserved() && !reserve_done && reserve_attempts<40) {
            ++reserve_attempts;
            reserve_done=reserve_space(width);
            InterlockedExchange(state->words+ReserveState,reserve_done?Reserved:TargetRefused);
            if(reserve_done) { settled_dip=0; waiting_layout=true; }
        }
        if(!reserved()) return NoReservation;
        // Someone else changed the padding, or the panel left the tree.
        if(!same(padding(),padding_applied) || !m::VisualTreeHelper::GetParent(tray_panel())) {
            abandon_space();
            return NoReservation;
        }
        if(std::abs(width-reserved_dip)>.5) {
            padding_applied.Left+=width-reserved_dip;
            padding(padding_applied);
            // Growing keeps what was confirmed; the new part is confirmed only
            // after the apps have been laid out around it.
            if(width>reserved_dip) waiting_layout=true;
            reserved_dip=width;
            settled_dip=std::min(settled_dip,reserved_dip);
        }
        auto const& root=measured.root;
        const auto panel=tray_panel().TransformToVisual(root).TransformPoint({0,0});
        const auto origin=parent.TransformToVisual(root).TransformPoint({0,0});
        // The strip is where the panel's own content used to start.
        const double left=panel.X+padding_original.Left;
        if(!std::isfinite(left) || !std::isfinite(origin.X) || !std::isfinite(origin.Y)) return NoMeasure;
        InterlockedExchange(state->words+ReserveWidth,static_cast<LONG>(std::lround(reserved_dip*measured.scale)));
        const double items=measured.occupancy.items_right;
        if(items>left+.5) state->add(SettlingTicks,1);
        if(settled_dip<reserved_dip-.5) {
            const DWORD now=shared_now();
            if(waiting_layout || items>left+.5) clearing=false;
            else if(!clearing) { clearing=true; clear_since=now; }
            else if(now-clear_since>=SettleMs) { settled_dip=reserved_dip; clearing=false; }
        } else clearing=false;
        waiting_layout=false;
        const DWORD now=shared_now();
        for(auto* box:boxes)
            if(box->has_pending && (content(box,box->pending.dip)<=settled_dip+.5 || now-box->pending_since>=GrowWaitMs))
                box->draw();
        const double all=content(nullptr,0);
        if(all<=0 || all>settled_dip+.5) return Settling;
        for(auto const* box:boxes) if(box==this && drawn.empty()) return Settling;
        // The boxes keep to the tray's end of the strip, so a narrower one
        // leaves the newly reserved part free for the apps to leave.
        const double start=left+reserved_dip-all;
        double at=start;
        for(auto* box:boxes) {
            if(drawn_dip(*box)<=0) continue;
            if(!put(*box,at,root,origin)) return NoMeasure;
            at+=drawn_dip(*box)+BoxGap;
        }
        // Apps the left margin's change pushed over for a moment are not in
        // the way.
        if(items>start+.5 && !left_moving()) return Settling;
        return Shown;
    }

    // The first node of this class within `depth` levels, or null; an
    // unfamiliar tree is refused rather than guessed at.
    static x::FrameworkElement find_class(x::DependencyObject const& node, std::wstring_view name, int depth, int& budget) {
        if(--budget<0) return nullptr;
        if(winrt::get_class_name(node)==name) return node.try_as<x::FrameworkElement>();
        if(depth<=0) return nullptr;
        const auto count=m::VisualTreeHelper::GetChildrenCount(node);
        for(int index=0;index<count && index<64;++index)
            if(auto found=find_class(m::VisualTreeHelper::GetChild(node,index),name,depth-1,budget)) return found;
        return nullptr;
    }
    // The taskbar's repeater, TaskbarFrameRepeater: the first ItemsRepeater
    // within three levels of the Taskbar.TaskbarFrame under the root. Its
    // items are the Start button, search, task view and the apps.
    static x::FrameworkElement taskbar_frame(x::UIElement const& root) {
        int budget=400;
        return find_class(root,L"Taskbar.TaskbarFrame",3,budget);
    }
    static x::FrameworkElement taskbar_repeater(x::FrameworkElement const& frame) {
        int budget=400;
        return frame?find_class(frame,ItemsAnchor,3,budget):nullptr;
    }
    // A left and a right end in the root's DIPs, once any is seen.
    struct Ends {
        bool any=false;
        double first=0, last=0;
        void add(double left, double right) {
            if(!any || left<first) first=left;
            if(!any || right>last) last=right;
            any=true;
        }
    };
    // The repeater's items on screen: all of them, the apps from the Start
    // button to the last app, and the Widgets button. Windows puts that
    // button in the repeater but apart from the apps, at its left end when
    // they are centered and beside the tray when they are on the left. It is
    // known by its automation id or its name; one not known is taken for an
    // app. Recycled items sit far outside the taskbar and are left out.
    struct Items {
        Ends all, apps, widgets;
        // The Widgets button as it is seen, inside its item: centered, the
        // item reaches some way past it.
        Ends button;
        // Which side of the apps the Widgets button is on, and its widest
        // there, once steady.
        bool widgets_before=false, widgets_after=false;
        double widgets_widest=0;
        // The item laid out leftmost, apart from the margin this hook gave
        // it, which takes the room at the left, and whether the item that
        // has it was laid out now.
        x::FrameworkElement leftmost{nullptr};
        bool owned_seen=false;
    };
    static bool widgets_button(x::FrameworkElement const& element) {
        return xa::AutomationProperties::GetAutomationId(element)==L"WidgetsButton" || element.Name()==L"WidgetsButton";
    }
    // Whether an element draws anything of its own: a brush behind or around
    // it, text, an image, an icon or a shape. A transparent brush counts, as
    // it is what a hover lights up.
    static bool draws(x::DependencyObject const& node) {
        if(auto border=node.try_as<c::Border>()) {
            const auto thickness=border.BorderThickness();
            return border.Background() || (border.BorderBrush() && thickness.Left+thickness.Top+thickness.Right+thickness.Bottom>0);
        }
        if(auto panel=node.try_as<c::Panel>()) return panel.Background()!=nullptr;
        if(auto presenter=node.try_as<c::ContentPresenter>()) return presenter.Background()!=nullptr;
        return node.try_as<c::TextBlock>() || node.try_as<c::Image>() || node.try_as<c::IconElement>() || node.try_as<shapes::Shape>();
    }
    // The Widgets button as it is seen: what its item draws, within the
    // item, among the item's first few elements. The item is laid out wider
    // than that, and the button inside it goes by no name of its own. Not
    // any when it draws nothing.
    static Ends seen_button(x::FrameworkElement const& item, x::FrameworkElement const& root, double left, double right) {
        Ends seen;
        std::vector<x::DependencyObject> queue{item};
        for(size_t at=0;at<queue.size();++at) {
            const auto node=queue[at];
            const auto element=node.try_as<x::FrameworkElement>();
            if(element && element.Visibility()!=x::Visibility::Visible) continue;
            if(element && draws(node)) {
                const double width=element.ActualWidth();
                const auto point=element.TransformToVisual(root).TransformPoint({0,0});
                const double first=std::max(left,static_cast<double>(point.X)), last=std::min(right,point.X+width);
                if(width>0 && std::isfinite(point.X) && first<last) seen.add(first,last);
            }
            const auto count=m::VisualTreeHelper::GetChildrenCount(node);
            for(int index=0;index<count && queue.size()<256;++index) queue.push_back(m::VisualTreeHelper::GetChild(node,index));
        }
        return seen;
    }
    // `owned`, given `owned_dip` more left margin by this hook, is taken as
    // laid out that much further left when the leftmost item is looked for,
    // so that margin does not make another item the leftmost.
    static Items items_of(x::FrameworkElement const& repeater, x::FrameworkElement const& root, x::FrameworkElement const& owned=nullptr, double owned_dip=0) {
        Items items;
        double leftmost=0;
        const double across=root.ActualWidth();
        const auto count=m::VisualTreeHelper::GetChildrenCount(repeater);
        for(int index=0;index<count && index<2048;++index) {
            auto element=m::VisualTreeHelper::GetChild(repeater,index).try_as<x::FrameworkElement>();
            if(!element || element.Visibility()!=x::Visibility::Visible) continue;
            const double width=element.ActualWidth();
            const auto point=element.TransformToVisual(root).TransformPoint({0,0});
            if(!(width>0) || !std::isfinite(point.X) || point.X+width<=0 || point.X>=across) continue;
            items.all.add(point.X,point.X+width);
            if(widgets_button(element)) {
                items.widgets.add(point.X,point.X+width);
                const Ends seen=seen_button(element,root,point.X,point.X+width);
                if(seen.any) items.button.add(seen.first,seen.last);
                else items.button.add(point.X,point.X+width);
                continue;
            }
            items.apps.add(point.X,point.X+width);
            // The room at the left goes to the apps' first item, never to
            // the Widgets button.
            const bool own=owned && element==owned;
            items.owned_seen=items.owned_seen || own;
            const double slot=point.X-(own?owned_dip:0);
            if(!items.leftmost || slot<leftmost) {
                items.leftmost=element;
                leftmost=slot;
            }
        }
        if(!items.apps.any) items.apps=items.all;
        return items;
    }
    // Whether Windows' Widgets switch is on: TaskbarDa under Explorer's
    // Advanced key, on when it is not there, as Windows has it.
    static bool widgets_switched_on() {
        DWORD value=1, size=sizeof(value);
        const LSTATUS status=RegGetValueW(HKEY_CURRENT_USER,L"Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced",L"TaskbarDa",RRF_RT_REG_DWORD,nullptr,&value,&size);
        return status!=ERROR_SUCCESS || value!=0;
    }
    // The items with the Widgets button the rows beside the apps go by:
    // where it is, or was until WidgetsHoldMs ago, the side of the apps it
    // is on, and its widest on that side, so a row does not move each time
    // Windows narrows or widens it. Its widest is kept for each side apart,
    // as it is laid out wider when the apps are centered. Not held once the
    // Widgets switch is off.
    Items steady(Items items) {
        const DWORD now=shared_now();
        if(items.widgets.any) {
            const bool moved=!widgets_known || std::abs(items.button.first-button_first)>=ButtonStepDip
                || std::abs(items.button.last-button_last)>=ButtonStepDip;
            if(moved) {
                button_first=items.button.first;
                button_last=items.button.last;
            }
            items.button={true,button_first,button_last};
            widgets_known=true;
            widgets_first=items.widgets.first;
            widgets_last=items.widgets.last;
            widgets_at=now;
        } else if(widgets_known && now-widgets_at<WidgetsHoldMs && widgets_switched_on()) {
            items.widgets={true,widgets_first,widgets_last};
            items.button={true,button_first,button_last};
        } else {
            widgets_known=false;
            widgets_widest_before=widgets_widest_after=0;
            return items;
        }
        Ends const& widgets=items.widgets;
        const double width=widgets.last-widgets.first;
        items.widgets_before=widgets.last<=items.apps.first+.5;
        items.widgets_after=widgets.first>=items.apps.last-.5;
        if(items.widgets_before) items.widgets_widest=widgets_widest_before=std::max(widgets_widest_before,width);
        if(items.widgets_after) items.widgets_widest=widgets_widest_after=std::max(widgets_widest_after,width);
        return items;
    }
    // Whether the row after the last app may stand just after it, with no
    // margin of its own, while the Widgets button is after the apps. The
    // apps start at the repeater's left and the button keeps to its right,
    // so without that margin the button would be all of it further right.
    bool beside_apps(Items const& items, double width, double end) const {
        const double widgets=end+right_dip-(end-items.widgets.last)-items.widgets_widest;
        return widgets-BoxGap-(items.apps.last+BoxGap+width)>=(afters_beside?0.:SideRoomDip);
    }
    // Where a row `width` wide goes before the Start button: just before
    // it, in the room the Start button's margin keeps.
    static double before_start(Items const& items, double width) {
        return items.apps.first-BoxGap-width;
    }
    // Where the row at the left end starts: at the strip's start, or,
    // centered, just after the Widgets button as it is now.
    static double left_end(Items const& items, double strip) {
        return (items.widgets_before?std::max(strip,items.button.last):strip)+BoxGap;
    }
    // Where a row goes after the last app: just after it, or, standing
    // beyond the Widgets button, just after that, in its own margin.
    double after_apps(Items const& items) const {
        const double last=items.widgets_after && !afters_beside?std::max(items.all.last,items.widgets.last):items.apps.last;
        return last+BoxGap;
    }
    // Undoes the room on both sides, each margin only if it is still the
    // value this hook applied, as the tray's padding.
    void restore_left(Left outcome) {
        if(!left_repeater) return;
        const bool right_ours=right_owned.give_back(), left_ours=left_owned.give_back();
        left_repeater=nullptr;
        InterlockedExchange(state->words+LeftState,right_ours && left_ours?outcome:LeftMismatch);
    }
    // Gives the margins back: no box is at the left or after the apps any
    // more, or the taskbar left the supported shape. Taken again when asked
    // for again.
    void release_left() {
        restore_left(LeftRestored);
        left_done=false;
        left_attempts=0;
    }
    // The room on both sides of the taskbar repeater: on the left the left
    // margin of the apps' first item, the Start button, so the repeater
    // stays at the screen's left, as Windows places the search box it draws
    // while Search is open by where that box is inside the repeater; on the
    // right the repeater's right margin. On the left it is as wide as the
    // rows at the left end and before the Start button, with a gap each side
    // of each, whatever the alignment; on the right as wide as the row after
    // the last app. The left end's row is drawn at the repeater's left, or,
    // centered, just after the Widgets button, which keeps the left end; the
    // row before the Start button just before it; the row after the apps
    // just after the last app; all in room that stays clear however many
    // items there are. With the Widgets button beyond the apps, the row
    // after them stands beside them with no room of its own while there is
    // space, so none is left empty beyond the button; else it stands beyond
    // the button, in its room. The row after the apps hides while it reaches
    // the tray's side. The room is made once (tried
    // for 10 s while the taskbar assembles), follows the rows' widths, and is
    // stepped back from, never fought over, when someone else changes it or
    // the repeater leaves the tree. While an item still reaches into the
    // left strip the rows there wait; a wider label is drawn once there is
    // room, or after GrowWaitMs.
    template<class Set>
    void arrange_repeater(Measured const& measured, Boxes const& ends, Boxes const& starts, Boxes const& afters, Set const& set) {
        const auto all=[&](Hide why) { for(auto const* row:{&ends,&starts,&afters}) for(auto const* box:*row) set(box,why); };
        const auto part=[](Boxes const& row) { return row.empty()?0.:row_dip(row,true)+2*BoxGap; };
        auto const& root=measured.root;
        // The margins the rows want; a row beside the apps has none of its own.
        const auto wanted=[&] { return std::pair{part(ends)+part(starts),afters_beside?0.:part(afters)}; };
        if(!left_repeater && !left_done && left_attempts<40) {
            ++left_attempts;
            const auto frame=taskbar_frame(root);
            const auto repeater=taskbar_repeater(frame);
            if(!repeater) {
                InterlockedExchange(state->words+LeftWidth,frame?2:1);
                InterlockedExchange(state->words+LeftState,LeftRefused);
                return all(NoReservation);
            }
            afters_beside=false;
            // The room at the left goes to the leftmost item below, once the
            // items are measured.
            right_owned=Owned::of(repeater);
            right_dip=wanted().second;
            right_owned.put(0,right_dip);
            left_owned=Owned{};
            left_dip=0;
            left_repeater=repeater;
            left_done=true;
            left_changed=shared_now();
            InterlockedExchange(state->words+LeftState,LeftReserved);
        }
        if(!left_repeater) return all(NoReservation);
        // The item with the room at the left may leave the repeater when
        // Windows makes its items anew; it is let go, and the leftmost item
        // takes the room again below.
        if(left_owned.element && m::VisualTreeHelper::GetParent(left_owned.element).try_as<x::FrameworkElement>()!=left_repeater)
            left_owned.give_back();
        if(!right_owned.kept() || !left_owned.kept() || !m::VisualTreeHelper::GetParent(left_repeater)) {
            right_owned=Owned{};
            left_owned=Owned{};
            left_repeater=nullptr;
            InterlockedExchange(state->words+LeftState,LeftAbandoned);
            return all(NoReservation);
        }
        // The repeater and its items as the taskbar was last laid out; the
        // margins changed below take effect when it is laid out again.
        const auto content=left_repeater.TransformToVisual(root).TransformPoint({0,0});
        const double end=content.X+left_repeater.ActualWidth();
        const Items items=items_of(left_repeater,root,left_owned.element,left_dip), around=steady(items);
        // The row after the apps beside them or beyond the Widgets button,
        // decided only once the taskbar has been laid out after the margins
        // last changed.
        if(!left_moving() && std::isfinite(end)) {
            afters_beside=!afters.empty() && around.widgets_after && beside_apps(around,row_dip(afters,true),end);
        }
        const auto [left,right]=wanted();
        // A new leftmost item takes the room at the left from the one
        // before. Not while the one that has it is out of the layout for a
        // moment, as the Widgets button is each time the taskbar is laid out
        // again: moving the room would lay it out again.
        const DWORD seen_now=shared_now();
        if(items.owned_seen) left_seen=seen_now;
        const bool moved=items.leftmost && items.leftmost!=left_owned.element
            && (!left_owned.element || items.owned_seen || seen_now-left_seen>=WidgetsHoldMs);
        if(moved) {
            left_owned.give_back();
            left_owned=Owned::of(items.leftmost);
        }
        if(moved || std::abs(left-left_dip)>.5 || std::abs(right-right_dip)>.5) {
            if(left_owned.element) left_owned.put(left,0);
            right_owned.put(0,right);
            left_dip=left;
            right_dip=right;
            left_changed=shared_now();
        }
        // The left strip starts where the repeater does; its room is the
        // Start button's margin.
        const auto origin=parent.TransformToVisual(root).TransformPoint({0,0});
        const double strip=content.X;
        if(!std::isfinite(strip) || !std::isfinite(origin.X) || !std::isfinite(origin.Y)) return all(NoMeasure);
        InterlockedExchange(state->words+LeftWidth,static_cast<LONG>(std::lround(left_dip*measured.scale)));
        InterlockedExchange(state->words+RightWidth,static_cast<LONG>(std::lround(right_dip*measured.scale)));
        const bool any=items.all.any;
        const auto px=[&](double dip, bool up) { return measured.rect.left+static_cast<LONG>(up?std::ceil(dip*measured.scale):std::floor(dip*measured.scale)); };
        if(any) InterlockedExchange(state->words+LeftItemsStart,px(items.apps.first,false));
        InterlockedExchange(state->words+WidgetsLeft,items.widgets.any?px(items.widgets.first,false):0);
        InterlockedExchange(state->words+WidgetsRight,items.widgets.any?px(items.widgets.last,true):0);
        InterlockedExchange(state->words+SeenLeft,items.button.any?px(items.button.first,false):0);
        InterlockedExchange(state->words+SeenRight,items.button.any?px(items.button.last,true):0);
        // Clear once the apps' first item is past the room.
        const bool clear=!any || items.apps.first>=content.X+left_dip-.5;
        if(!clear && left>0) state->add(LeftTicks,1);
        // The row after the apps starts a gap after the last one and ends at
        // most a gap before the tray's side: the strip beside it, or the
        // tray itself.
        const double after=any?after_apps(around):content.X+BoxGap, edge=measured.occupancy.tray_left-BoxGap;
        const DWORD now=shared_now();
        const auto grow=[&](Boxes const& row, bool room) {
            for(auto* box:row) if(box->has_pending && (room || now-box->pending_since>=GrowWaitMs)) box->draw();
        };
        grow(ends,clear);
        grow(starts,clear);
        grow(afters,after+row_dip(afters,true)<=edge+.5);
        // A row is put where it goes, or waits hidden.
        const auto seat=[&](Boxes const& row, double from, Hide wait) {
            Hide why=wait;
            if(why==Shown && !put_row(row,from,root,origin)) why=NoMeasure;
            for(auto const* box:row) set(box,box->drawn.empty()?NoText:why);
        };
        // Until the taskbar is laid out again after the left margin went in
        // or grew, the strip reads as starting off the screen.
        const Hide left_wait=!clear || strip<-.5?Settling:Shown;
        if(!ends.empty()) seat(ends,left_end(around,strip),left_wait);
        if(!starts.empty()) {
            const double width=row_dip(starts,false);
            seat(starts,any?before_start(around,width):content.X-BoxGap-width,left_wait);
        }
        // A row beside the apps waits while the Widgets button, as it is now,
        // is in its way: it goes beyond the button once that is decided.
        if(!afters.empty()) {
            const double width=row_dip(afters,false);
            const bool reaches=after+width>edge+.5;
            const bool blocked=afters_beside && around.widgets_after && after+width>around.widgets.first-BoxGap+.5;
            if(reaches) state->add(RightTicks,1);
            // Apps the margins' change pushed over for a moment are not in
            // the way.
            seat(afters,after,blocked || (reaches && !left_moving())?Settling:Shown);
        }
    }

    // Where a dragged box may land, in the parent's DIPs, as it would be
    // drawn there: before or after each other box shown, in that box's place;
    // where the box is, when it is alone in its place; else at the left end,
    // just before the Start button, just after the last app (or beyond the
    // Widgets button, as those rows are drawn) and beside the tray. Places
    // at one spot, within SnapSameDip, are one, the first listed: with the
    // apps on the left, the left end and before the Start button, which is
    // never left of the left end. Places only near each other stay apart:
    // with many apps, after the last one and beside the tray.
    std::vector<Snap> snaps_for(Context const& dragged) {
        std::vector<Snap> found;
        const auto xaml_root=parent.XamlRoot();
        const auto root=xaml_root?xaml_root.Content().try_as<x::FrameworkElement>():nullptr;
        if(!root || !anchor) return found;
        const double origin=parent.TransformToVisual(root).TransformPoint({0,0}).X;
        const double width=drawn_dip(dragged);
        const Place own=asked(dragged.box_index);
        const auto repeater=left_repeater?left_repeater:taskbar_repeater(taskbar_frame(root));
        const Items items=repeater?steady(items_of(repeater,root,left_owned.element,left_dip)):Items{};
        const double strip=repeater?repeater.TransformToVisual(root).TransformPoint({0,0}).X:0;
        const double tray=reserved()
            ?tray_panel().TransformToVisual(root).TransformPoint({0,0}).X+padding_original.Left+reserved_dip
            :anchor.TransformToVisual(root).TransformPoint({0,0}).X;
        for(const Place slot:{LeftEnd,BeforeStart,AfterApps,Tray}) {
            bool beside=false;
            for(size_t at=0;at<MaxBoxes;++at) {
                Context const* other=box_at(at);
                if(!other || other==&dragged || asked(at)!=slot || other->drawn.empty() || !other->widget
                    || other->widget.Visibility()!=x::Visibility::Visible) continue;
                beside=true;
                const double left=other->widget.Margin().Left+origin;
                const LONG target=static_cast<LONG>(at);
                found.push_back({left-BoxGap-width,slot,BeforeOther,target});
                found.push_back({left+other->drawn.dip+BoxGap,slot,AfterOther,target});
            }
            if(beside) continue;
            if(own==slot) found.push_back({dragged.press_left+origin,slot,Alone});
            else if(slot==LeftEnd) found.push_back({left_end(items,strip),slot,Alone});
            else if(slot==BeforeStart && items.all.any) found.push_back({std::max(before_start(items,width),strip+BoxGap),slot,Alone});
            else if(slot==AfterApps && items.all.any) found.push_back({after_apps(items),slot,Alone});
            else if(slot==Tray) found.push_back({tray-width,slot,Alone});
        }
        std::vector<Snap> kept;
        for(auto const& snap:found)
            if(std::none_of(kept.begin(),kept.end(),[&](Snap const& k) { return std::abs(k.x-snap.x)<SnapSameDip; }))
                kept.push_back(snap);
        for(auto& snap:kept) snap.x-=origin;
        return kept;
    }
    // A box let go on `snap`: written for the controller, the sequence
    // last; the box is held where it was let go until the controller moves
    // it, or for DropHoldMs.
    void drop(Context& dropped, Snap const& snap) {
        dropped.held=true;
        dropped.dropped_at=shared_now();
        dropped.held_layout=layout_words();
        InterlockedExchange(state->words+DropBox,static_cast<LONG>(dropped.box_index));
        InterlockedExchange(state->words+DropPlace,snap.place);
        InterlockedExchange(state->words+DropOrder,snap.order);
        InterlockedExchange(state->words+DropTarget,snap.target);
        state->add(DropSeq,1);
    }

    // New icons for cells or blocks already drawn; new ones get theirs in
    // draw().
    void apply_new_icons() {
        if(icons_changed && !cells.empty()) {
            for(size_t index=0;index<cells.size();++index) apply_icon(index);
            icons_changed=false;
        } else if(icons_changed && !blocks.empty()) {
            apply_slots();
            icons_changed=false;
        }
    }
    // A label's widths again at a new scale. A scene is placed in DIPs by
    // the controller; only a label is measured here.
    void measure_again() {
        if(!drawn.empty() && !drawn.scene) { measure_label(drawn); apply_widths(); }
        if(has_pending && !pending.scene) measure_label(pending);
    }
    static bool show(x::FrameworkElement const& widget, bool visible) {
        const auto target=visible?x::Visibility::Visible:x::Visibility::Collapsed;
        if(widget.Visibility()==target) return false;
        widget.Visibility(target);
        return true;
    }
    void update() noexcept {
        if(detached || updating) return;
        updating=true;
        const auto began=stamp();
        try {
            if(state->expired()) { detach(); updating=false; return; }
            take_icons();
            take_text();
            apply_new_icons();
            for(auto const& other:others) {
                other->take_icons();
                other->take_text();
                other->apply_new_icons();
            }
            const auto measured=measure();
            publish(measured);
            if(measured.ok && measured.scale!=measured_scale) {
                measure_again();
                for(auto const& other:others) other->measure_again();
                measured_scale=measured.scale;
            }
            const Hide why=arrange(measured);
            InterlockedExchange(state->words+HideReason,why);
            const bool visible=why==Shown;
            // Count actual transitions, including ones shorter than controller polling.
            // A box being dragged stays on screen.
            if(show(widget,visible || dragging)) InterlockedIncrement(state->words+VisibilityWord);
            InterlockedExchange(state->words+ShownWord,visible?Full:0);
            // Each box: shown or not, why, and its width.
            for(size_t at=0;at<MaxBoxes;++at) {
                Context* b=box_at(at);
                const bool shows=hides[at]==Shown;
                if(at>0) show(b->widget,shows || b->dragging);
                InterlockedExchange(state->words+BoxShown+at,shows?1:0);
                InterlockedExchange(state->words+BoxHide+at,hides[at]);
                InterlockedExchange(state->words+BoxWidth+at,static_cast<LONG>(std::lround(drawn_dip(*b))));
            }
            state->status(visible?2:3);
        } catch(...) {
            state->status(5,winrt::to_hresult());
            detach();
        }
        state->add(TicksWord,1);
        state->add(TickMicrosWord,elapsed_us(began));
        updating=false;
    }
    // A press of the left button on the box, while boxes may be dragged: the
    // box takes the pointer, so a drag goes on wherever the pointer goes.
    void press(input::PointerRoutedEventArgs const& args) {
        if(state->read(DragAllowed)!=1 || !widget || !parent) return;
        const auto point=args.GetCurrentPoint(parent);
        if(!point.Properties().IsLeftButtonPressed()) return;
        pressed=true;
        dragging=false;
        press_x=point.Position().X;
        press_left=widget.Margin().Left;
        widget.CapturePointer(args.Pointer());
        args.Handled(true);
    }
    // Once the pointer is DragDip from where it was pressed, the box follows
    // it along the taskbar, and sits on a place it comes within SnapDip of.
    void drag(input::PointerRoutedEventArgs const& args) {
        if(!pressed) return;
        const double x=args.GetCurrentPoint(parent).Position().X;
        if(!dragging) {
            if(std::abs(x-press_x)<DragDip) return;
            dragging=true;
            held=false;
            state->add(Drags,1);
            if(auto self=placer.lock()) snaps=self->snaps_for(*this);
        }
        const double free=press_left+x-press_x;
        snapped=nearest(free,SnapDip);
        auto margin=widget.Margin();
        margin.Left=snapped<0?free:snaps[snapped].x;
        widget.Margin(margin);
        args.Handled(true);
    }
    // The snap nearest `left`, closer than `reach`, or -1.
    int nearest(double left, double reach) const {
        int found=-1;
        for(size_t index=0;index<snaps.size();++index)
            if(const double off=std::abs(snaps[index].x-left); off<reach) { reach=off; found=static_cast<int>(index); }
        return found;
    }
    // Let go: a drag is reported on the place it sat on, or else the
    // nearest. A press that never moved that far is a click, which does
    // nothing.
    void let_go(input::PointerRoutedEventArgs const& args) {
        if(!pressed) return;
        pressed=false;
        const bool dropped=dragging;
        dragging=false;
        if(dropped) {
            const double free=press_left+args.GetCurrentPoint(parent).Position().X-press_x;
            const int pick=snapped>=0?snapped:nearest(free,std::numeric_limits<double>::infinity());
            if(auto self=placer.lock(); self && pick>=0) self->drop(*this,snaps[pick]);
            args.Handled(true);
        }
        snapped=-1;
        widget.ReleasePointerCapture(args.Pointer());
    }
    // The pointer was taken away in a drag: the next tick puts the box back.
    void lose() {
        if(dragging) state->add(DragsLost,1);
        pressed=false;
        dragging=false;
        snapped=-1;
    }
    // Asks the controller for its tray menu, which opens where the pointer is,
    // on this box: its number, from 1, goes with the message. Its process may
    // take the foreground, as the shell lets a tray icon's, so the menu closes
    // on a click elsewhere.
    void open_menu() {
        if(state->expired()) return;
        const DWORD pid=static_cast<DWORD>(state->read(ControllerWord));
        const auto window=reinterpret_cast<HWND>(static_cast<LONG_PTR>(state->read(MenuWindow)));
        DWORD owner=0;
        if(!window || pid==0 || !GetWindowThreadProcessId(window,&owner) || owner!=pid) return;
        AllowSetForegroundWindow(pid);
        if(PostMessageW(window,OpenMenu,box_index+1,0)) state->add(MenuRequests,1);
    }

    // The box: a border holding the label's row, a scene's canvas and the
    // measurer, which takes a right click; on the root Grid as our own
    // element, hidden until placed.
    void make_widget(wchar_t const* name) {
        widget=c::Border();
        widget.Name(name);
        // Size, padding, corners and colors come with the first label drawn.
        widget.Width(208);
        widget.HorizontalAlignment(x::HorizontalAlignment::Left);
        widget.VerticalAlignment(x::VerticalAlignment::Top);
        // It takes a right click, which opens the tray menu.
        widget.IsHitTestVisible(true);
        widget.Visibility(x::Visibility::Collapsed);
        auto content=c::Grid();
        row=c::StackPanel();
        row.Orientation(c::Orientation::Horizontal);
        content.Children().Append(row);
        // A scene's blocks go where it says, inside the box's padding.
        stage=c::Canvas();
        content.Children().Append(stage);
        // A Canvas does not size to its children, so the measurer takes no
        // room while it sits in the tree at the current scale.
        auto host=c::Canvas();
        measurer=c::TextBlock();
        measurer.Opacity(0);
        host.Children().Append(measurer);
        content.Children().Append(host);
        widget.Child(content);
        c::Canvas::SetZIndex(widget,1000);
        auto weak=weak_from_this();
        // A right click on the box stops at the box and opens the tray menu, and
        // its context request stops there too, so the taskbar's own menu does
        // not open under it. Other input goes on as before.
        right_tapped=widget.RightTapped([weak](auto const&,input::RightTappedRoutedEventArgs const& args) noexcept {
            try {
                args.Handled(true);
                if(auto self=weak.lock()) self->open_menu();
            } catch(...) {}
        });
        context_requested=widget.ContextRequested([](auto const&,input::ContextRequestedEventArgs const& args) noexcept {
            try { args.Handled(true); } catch(...) {}
        });
        // The left button drags the box along the taskbar.
        pressed_token=widget.PointerPressed([weak](auto const&,input::PointerRoutedEventArgs const& args) noexcept {
            try { if(auto self=weak.lock()) self->press(args); } catch(...) {}
        });
        moved_token=widget.PointerMoved([weak](auto const&,input::PointerRoutedEventArgs const& args) noexcept {
            try { if(auto self=weak.lock()) self->drag(args); } catch(...) {}
        });
        released_token=widget.PointerReleased([weak](auto const&,input::PointerRoutedEventArgs const& args) noexcept {
            try { if(auto self=weak.lock()) self->let_go(args); } catch(...) {}
        });
        lost_token=widget.PointerCaptureLost([weak](auto const&,input::PointerRoutedEventArgs const&) noexcept {
            try { if(auto self=weak.lock()) self->lose(); } catch(...) {}
        });
        have_input=true;
        parent.Children().Append(widget);
    }
    // Takes the box off the root Grid with what it showed. Each step is
    // caught on its own by the caller's order.
    void drop_widget() {
        if(widget && have_input) {
            widget.RightTapped(right_tapped);
            widget.ContextRequested(context_requested);
            widget.PointerPressed(pressed_token);
            widget.PointerMoved(moved_token);
            widget.PointerReleased(released_token);
            widget.PointerCaptureLost(lost_token);
        }
        have_input=false;
        if(widget) widget.Visibility(x::Visibility::Collapsed);
        if(parent && widget) {
            uint32_t index=0;
            if(parent.Children().IndexOf(widget,index)) parent.Children().RemoveAt(index);
        }
    }

    void start() {
        placer=weak_from_this();
        make_widget(L"LimitPeekerBox");
        // The other boxes, each from its own copy of the box's words. They
        // have no timer: this one takes their words and places them.
        for(size_t at=1;at<MaxBoxes;++at) {
            auto other=std::make_shared<Context>();
            other->state=state;
            other->parent=parent;
            other->box_index=at;
            other->base=BoxWords*at;
            other->width_word=BoxWidth+at;
            other->placer=weak_from_this();
            const std::wstring name=L"LimitPeekerBox"+std::to_wstring(at+1);
            other->make_widget(name.c_str());
            others[at-1]=other;
        }
        auto weak=weak_from_this();
        timer=x::DispatcherTimer();
        timer.Interval(std::chrono::milliseconds(250));
        tick=timer.Tick([weak](auto const&,auto const&) noexcept {if(auto self=weak.lock()) self->update();});
        have_tick=true;
        layout=parent.LayoutUpdated([weak](auto const&,auto const&) noexcept {
            if(auto self=weak.lock()) {
                InterlockedIncrement(self->state->words+LayoutRevisionWord);
                self->update();
            }
        });
        have_layout=true;
        timer.Start();
        update();
    }
};

// UI objects are created, mutated and released only on their XAML dispatcher.
// A stopped Context has no XAML references; the small inert shell is retained
// until Explorer exits to avoid cross-thread destruction/unload races.
thread_local std::shared_ptr<Context> context;

struct Watcher : winrt::implements<Watcher,IVisualTreeServiceCallback2> {
    winrt::com_ptr<IGlobalInterfaceTable> git;
    DWORD cookie;
    std::shared_ptr<State> state;
    Watcher(winrt::com_ptr<IGlobalInterfaceTable> table,DWORD key,std::shared_ptr<State> s):git(table),cookie(key),state(s){}
    HRESULT STDMETHODCALLTYPE OnVisualTreeChange(ParentChildRelation,VisualElement element,VisualMutationType kind) noexcept override {
        try {
            if(!state->stopped) InterlockedIncrement(state->words+LayoutRevisionWord);
            if(kind!=Add || state->stopped || !element.Type || wcscmp(element.Type,L"SystemTray.SystemTrayFrame")!=0) return S_OK;
            state->reach(FrameSeen);
            winrt::com_ptr<IXamlDiagnostics> diagnostics;
            winrt::check_hresult(git->GetInterfaceFromGlobal(cookie,__uuidof(IXamlDiagnostics),diagnostics.put_void()));
            f::IInspectable object;
            winrt::check_hresult(diagnostics->GetIInspectableFromHandle(element.Handle,reinterpret_cast<::IInspectable**>(winrt::put_abi(object))));
            auto anchor=object.try_as<x::FrameworkElement>();
            if(!anchor || state->scheduled.exchange(true)) return S_OK;
            auto s=state;
            anchor.Dispatcher().RunAsync(core::CoreDispatcherPriority::Low,[anchor,s]() noexcept {
                try {
                    s->reach(Dispatched);
                    if(s->expired() || s->stopped) { s->stopped=true; s->status(4); return; }
                    auto parent=anchor.Parent().try_as<c::Grid>();
                    // No shifting columns or resizing any native taskbar element.
                    if(!parent || parent.ColumnDefinitions().Size()!=0 || parent.RowDefinitions().Size()>1) {
                        s->status(5,E_NOTIMPL); s->stopped=true; return;
                    }
                    if(auto content=anchor.XamlRoot()?anchor.XamlRoot().Content().try_as<x::FrameworkElement>():nullptr)
                        InterlockedExchange(s->words+AttachRootWidth,
                            static_cast<LONG>(content.ActualWidth()*anchor.XamlRoot().RasterizationScale()));
                    context=std::make_shared<Context>();
                    context->state=s; context->parent=parent; context->anchor=anchor;
                    context->start();
                    s->reach(Started);
                    std::lock_guard guard(s->lock);
                    s->dispatcher=anchor.Dispatcher();
                } catch(...) {
                    s->status(5,winrt::to_hresult());
                    if(context) context->detach();
                    s->stopped=true;
                }
            });
        } catch(...) { state->status(5,winrt::to_hresult()); state->stopped=true; }
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE OnElementStateChanged(InstanceHandle,VisualElementState,LPCWSTR) noexcept override {return S_OK;}
};

// On the taskbar thread: a resting hook's box starts again on the tray frame it
// had, while that frame is still in the tree; otherwise the hook leaves (4) and
// the controller attaches anew.
void restart(std::shared_ptr<State> const& s) noexcept {
    try {
        auto anchor=context?context->anchor:nullptr;
        auto parent=anchor?anchor.Parent().try_as<c::Grid>():nullptr;
        if(!anchor || !anchor.XamlRoot() || !parent || parent.ColumnDefinitions().Size()!=0 || parent.RowDefinitions().Size()>1) {
            s->stopped=true; s->status(4); return;
        }
        context=std::make_shared<Context>();
        context->state=s; context->parent=parent; context->anchor=anchor;
        context->start();
        s->reach(Started);
    } catch(...) {
        s->status(5,winrt::to_hresult());
        if(context) context->detach();
        s->stopped=true;
    }
}

// The box came off cleanly (6) and the taskbar thread is known: stay subscribed,
// resting (7), with no time limit. A controller started again claims the
// session (7 to 8) and, once its lease runs, the box starts again on the same
// tray without a new subscription, which would reach every XAML thread and end
// Explorer on the Snap layouts one. --remove sends it away with the same
// compare-exchange (7 to 4), so the two cannot both win. False when the hook
// leaves: it was sent away, a claim never came to a lease, or it was not
// resting.
bool rest(std::shared_ptr<State> const& s) {
    core::CoreDispatcher dispatcher{nullptr};
    { std::lock_guard guard(s->lock); dispatcher=s->dispatcher; }
    if(!dispatcher || InterlockedCompareExchange(s->words+StatusWord,Resting,6)!=6) return false;
    DWORD claimed=0;
    for(;;) {
        Sleep(250);
        const LONG status=s->read(StatusWord);
        if(status==Claimed) {
            if(!claimed) claimed=shared_now();
            if(s->read(0)==Magic && s->read(1)==Version && !s->expired()) break;
            if(shared_now()-claimed>ClaimMs) { s->status(4); return false; }
        } else if(status!=Resting) {
            return false;
        }
    }
    s->status(1);
    s->stopped=false;
    auto shared=s;
    try {
        dispatcher.RunAsync(core::CoreDispatcherPriority::Low,[shared]() noexcept { restart(shared); });
    } catch(...) {
        // The taskbar thread is gone: leave, and unsubscribe.
        s->stopped=true; s->status(4); return false;
    }
    return true;
}

// The attach a hook's initialization data names, 0 when it names none.
LONG attach_token(IXamlDiagnostics* diagnostics) noexcept {
    BSTR data=nullptr;
    if(FAILED(diagnostics->GetInitializationData(&data)) || !data) return 0;
    const LONG token=static_cast<LONG>(wcstoul(data,nullptr,10));
    SysFreeString(data);
    return token;
}

struct Tap : winrt::implements<Tap,IObjectWithSite> {
    winrt::com_ptr<IUnknown> site;
    HRESULT STDMETHODCALLTYPE SetSite(IUnknown* value) noexcept override {
        std::shared_ptr<State> state;
        bool taken=false;
        // Once the session is known to be of this version, every refusal is
        // on record.
        auto refuse=[&state](Refusal why,HRESULT hr) { InterlockedExchange(state->words+SiteRefusal,why); return hr; };
        try {
            if(!value) return E_UNEXPECTED;
            const DWORD pid=GetCurrentProcessId();
            state=std::make_shared<State>();
            wchar_t name[96]{};
            swprintf_s(name,L"Local\\LimitPeekerXaml_v1_%lu",pid);
            state->mapping=OpenFileMappingW(FILE_MAP_ALL_ACCESS,FALSE,name);
            if(!state->mapping) return HRESULT_FROM_WIN32(GetLastError());
            state->words=static_cast<volatile LONG*>(MapViewOfFile(state->mapping,FILE_MAP_ALL_ACCESS,0,0,SharedBytes));
            if(!state->words) return HRESULT_FROM_WIN32(GetLastError());
            if(state->read(0)!=Magic || state->read(1)!=Version) return E_ACCESSDENIED;
            state->add(SiteCalls,1);
            if(site) return refuse(SiteAgain,E_UNEXPECTED);
            if(!permitted()) return refuse(Elevated,E_ACCESSDENIED);
            DWORD shell=0; GetWindowThreadProcessId(FindWindowW(L"Shell_TrayWnd",nullptr),&shell);
            if(shell!=pid) return refuse(NotTaskbar,E_ACCESSDENIED);
            if(state->expired()) return refuse(LeaseGone,E_ACCESSDENIED);
            winrt::com_ptr<IXamlDiagnostics> diagnostics;
            if(const HRESULT hr=value->QueryInterface(__uuidof(IXamlDiagnostics),diagnostics.put_void()); FAILED(hr)) return refuse(SetupFailed,hr);
            // A hook an earlier attach loaded, come after the controller
            // attached again.
            if(attach_token(diagnostics.get())!=state->read(AttachToken)) return refuse(OtherAttach,E_ACCESSDENIED);
            // Deliberately PIN: safely removing UI != safely unloading XAML code.
            HMODULE pin{};
            if(!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS|GET_MODULE_HANDLE_EX_FLAG_PIN,
                reinterpret_cast<LPCWSTR>(&permitted),&pin)) return refuse(SetupFailed,HRESULT_FROM_WIN32(GetLastError()));
            auto git=winrt::create_instance<IGlobalInterfaceTable>(CLSID_StdGlobalInterfaceTable);
            DWORD cookie=0;
            if(const HRESULT hr=git->RegisterInterfaceInGlobal(diagnostics.get(),__uuidof(IXamlDiagnostics),&cookie); FAILED(hr)) return refuse(SetupFailed,hr);
            // One hook takes the session: of two that come at once, the
            // other finds it taken.
            if(InterlockedCompareExchange(state->words+StatusWord,1,0)!=0) {
                git->RevokeInterfaceFromGlobal(cookie);
                return refuse(Taken,E_ACCESSDENIED);
            }
            taken=true;
            InterlockedExchange(state->words+ErrorWord,S_OK);
            site.copy_from(value);
            state->reach(SiteSet);
            std::thread([git,cookie,state]() noexcept {
                try {
                    winrt::init_apartment(winrt::apartment_type::multi_threaded);
                    struct Apartment { ~Apartment() { winrt::uninit_apartment(); } } apartment;
                    state->reach(ThreadReady);
                    struct Registration {
                        winrt::com_ptr<IGlobalInterfaceTable> table; DWORD key;
                        ~Registration() { table->RevokeInterfaceFromGlobal(key); }
                    } registration{git,cookie};
                    winrt::com_ptr<IXamlDiagnostics> diagnostics;
                    winrt::check_hresult(git->GetInterfaceFromGlobal(cookie,__uuidof(IXamlDiagnostics),diagnostics.put_void()));
                    state->reach(DiagnosticsReady);
                    auto watcher=winrt::make_self<Watcher>(git,cookie,state);
                    auto service=diagnostics.as<IVisualTreeService3>();
                    state->reach(TreeServiceReady);
                    HRESULT advised=E_FAIL;
                    for(LONG attempt=1,cleaned=0;;++attempt) {
                        InterlockedExchange(state->words+AdviseTries,attempt|(cleaned<<8));
                        advised=service->AdviseVisualTreeChange(watcher.get());
                        if(SUCCEEDED(advised) || attempt>=AdviseAttempts || state->stopped || state->expired()) break;
                        Sleep(500);
                        if(SUCCEEDED(service->UnadviseVisualTreeChange(watcher.get()))) ++cleaned;
                        InterlockedExchange(state->words+AdviseTries,attempt|(cleaned<<8));
                    }
                    if(FAILED(advised) && (state->stopped || state->expired())) {
                        // Never subscribed, so nothing to release; 5 would block the next reuse.
                        state->stopped=true; state->status(4); return;
                    }
                    winrt::check_hresult(advised);
                    state->reach(Advised);
                    // Live until the box comes off, then resting, and live
                    // again each time a controller claims it.
                    do {
                        while(!state->stopped) {
                            if(state->expired() && !state->scheduled) { state->stopped=true; state->status(4); }
                            Sleep(250);
                        }
                    } while(rest(state));
                    const auto hr=service->UnadviseVisualTreeChange(watcher.get());
                    if(FAILED(hr)) state->status(5,hr);
                    else {
                        state->status(4);
                    }
                    // COM interfaces released before this thread exits.
                } catch(...) {state->status(5,winrt::to_hresult()); state->stopped=true;}
            }).detach();
            return S_OK;
        } catch(...) {
            // A session taken ends on the error; one not yet taken is
            // turned down, so the controller may attach again.
            const HRESULT hr=winrt::to_hresult();
            if(state && state->words && state->read(0)==Magic && state->read(1)==Version) {
                if(taken) { state->status(5,hr); state->stopped=true; }
                else refuse(SetupFailed,hr);
            }
            return hr;
        }
    }
    HRESULT STDMETHODCALLTYPE GetSite(REFIID iid,void** result) noexcept override {
        if(!result) return E_POINTER;
        *result=nullptr;
        return site?site->QueryInterface(iid,result):E_FAIL;
    }
};

struct Factory : winrt::implements<Factory,IClassFactory> {
    HRESULT STDMETHODCALLTYPE CreateInstance(IUnknown* outer,REFIID iid,void** result) noexcept override {
        if(!result) return E_POINTER;
        *result=nullptr;
        if(outer) return CLASS_E_NOAGGREGATION;
        try { return winrt::make<Tap>().as(iid,result); } catch(...) {return winrt::to_hresult();}
    }
    HRESULT STDMETHODCALLTYPE LockServer(BOOL) noexcept override {return S_OK;}
};
extern "C" HRESULT __stdcall DllGetClassObject(REFCLSID clsid,REFIID iid,void** result) noexcept {
    if(!result) return E_POINTER;
    *result=nullptr;
    if(clsid!=TapClsid || !permitted()) return CLASS_E_CLASSNOTAVAILABLE;
    try { return winrt::make<Factory>().as(iid,result); } catch(...) {return winrt::to_hresult();}
}
extern "C" HRESULT __stdcall DllCanUnloadNow() noexcept {return S_FALSE;}
