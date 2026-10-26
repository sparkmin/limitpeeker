//! How the box looks. The hook draws with whatever look it is sent, so a new
//! look needs only a new EXE, never a new DLL or an Explorer restart.

/// Colors are ARGB. Sizes are in hundredths of a DIP; `motion` is in ms.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Style {
    pub box_color: u32,
    pub track_color: u32,
    pub font: u32,
    pub weight: u32,
    pub height: u32,
    pub padding: u32,
    pub gap: u32,
    pub gauge: u32,
    pub gauge_gap: u32,
    pub radius: u32,
    /// The gauge below the text (0), beside it (1), or a dial before the
    /// text (2); and its width beside, or the dial's diameter.
    pub layout: u32,
    pub gauge_width: u32,
    /// Each cell's card. A transparent color and border draw no card.
    pub cell_color: u32,
    pub cell_border: u32,
    pub cell_radius: u32,
    pub cell_padding: u32,
    pub cell_inset: u32,
    /// How long a fill takes to move to a new value; 0 moves it at once.
    pub motion: u32,
    /// The icon's size before the gauges (0 for none), the gap after it, and
    /// whether every cell takes the widest cell's width.
    pub icon: u32,
    pub icon_gap: u32,
    pub uniform: u32,
    /// Whether a gauge's colored part sits against the end rather than from
    /// the start, which the fill mode and the gauge's shape decide, not the
    /// look; and whether a value sits at the end of its column, so `--` ends
    /// where the digits do.
    pub fill_end: u32,
    pub text_end: u32,
    /// How a cell's gauges are arranged: stacked (0), side by side (1), or as
    /// rings on one dial (2); the space between them, between a label and its
    /// value, a ring's thickness, and the labels' color.
    pub arrange: u32,
    pub spacing: u32,
    pub label_gap: u32,
    pub ring: u32,
    pub label_color: u32,
    /// How heavy a warning's mark is. The mark is drawn a character at a
    /// time, each from a whole pixel, and heavier than the digits, so every
    /// mark is as clear as the first at any size or scale.
    pub mark_weight: u32,
}

/// What the hook accepts for each size, as (name, least, most); it refuses a
/// label with any value outside. Must match the Min/Max pairs in
/// native/taskbar_tap.cpp.
pub const RANGES: [(&str, u32, u32); 24] = [
    ("Font", 800, 2400),
    ("Weight", 100, 900),
    ("Height", 1600, 4800),
    ("Padding", 0, 2400),
    ("Gap", 0, 4800),
    ("Gauge", 0, 800),
    ("GaugeGap", 0, 800),
    ("Radius", 0, 1600),
    ("Layout", 0, 2),
    ("GaugeWidth", 0, 12000),
    ("CellRadius", 0, 1600),
    ("CellPadding", 0, 2400),
    ("CellInset", 0, 1600),
    ("Motion", 0, 400),
    ("Icon", 0, 3200),
    ("IconGap", 0, 1600),
    ("Uniform", 0, 1),
    ("FillEnd", 0, 1),
    ("TextEnd", 0, 1),
    ("Arrange", 0, 2),
    ("Spacing", 0, 1600),
    ("LabelGap", 0, 1600),
    ("Ring", 0, 800),
    ("MarkWeight", 100, 900),
];

impl Style {
    /// The sizes in RANGES order.
    fn ranged(&self) -> [u32; 24] {
        [
            self.font,
            self.weight,
            self.height,
            self.padding,
            self.gap,
            self.gauge,
            self.gauge_gap,
            self.radius,
            self.layout,
            self.gauge_width,
            self.cell_radius,
            self.cell_padding,
            self.cell_inset,
            self.motion,
            self.icon,
            self.icon_gap,
            self.uniform,
            self.fill_end,
            self.text_end,
            self.arrange,
            self.spacing,
            self.label_gap,
            self.ring,
            self.mark_weight,
        ]
    }
    /// Whether the hook will take it. It keeps its previous label otherwise.
    pub fn fits(&self) -> bool {
        self.ranged()
            .iter()
            .zip(RANGES)
            .all(|(value, (_, least, most))| (least..=most).contains(value))
    }
    /// The words from StyleBox, in the hook's order.
    pub fn words(&self) -> [u32; 29] {
        [
            self.box_color,
            self.track_color,
            self.font,
            self.weight,
            self.height,
            self.padding,
            self.gap,
            self.gauge,
            self.gauge_gap,
            self.radius,
            self.layout,
            self.gauge_width,
            self.cell_color,
            self.cell_border,
            self.cell_radius,
            self.cell_padding,
            self.cell_inset,
            self.motion,
            self.icon,
            self.icon_gap,
            self.uniform,
            self.fill_end,
            self.text_end,
            self.arrange,
            self.spacing,
            self.label_gap,
            self.ring,
            self.label_color,
            self.mark_weight,
        ]
    }
    /// Whether the gauge is a dial rather than a bar.
    pub fn dial(&self) -> bool {
        self.layout == 2
    }
    /// Whether a cell's gauges sit side by side, rather than one above the
    /// other or as rings on one dial.
    pub fn side(&self) -> bool {
        self.arrange == 1
    }
}

/// Which share of a gauge the service's color covers. A value is read as the
/// share used. Used colors it from the start: a bar from the left, a dial
/// clockwise from twelve o'clock. Left colors what is left, which the used
/// share eats into as it grows: a bar from the right, and a dial clockwise
/// from twelve o'clock, as a countdown timer does; its number is what is left
/// too.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fill {
    Used,
    Left,
}

impl Fill {
    pub const ALL: [Fill; 2] = [Fill::Used, Fill::Left];
    pub fn name(self) -> &'static str {
        match self {
            Fill::Used => "used",
            Fill::Left => "left",
        }
    }
    pub fn find(name: &str) -> Option<Fill> {
        Self::ALL.into_iter().find(|fill| fill.name() == name)
    }
}

/// How a look writes each value: with a `%`, or as digits alone for a dial
/// that already shows it is a share. Values sit at the end of their column, so
/// they need no padding to line up.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Numbers {
    Percent,
    Digits,
}

/// A look: the style sent to the hook, and what the controller decides on
/// its own, which is the values' color and how values are written. Each
/// service brings its own color. `backdrop` is the opaque color the box's
/// content is seen against: its color over the taskbar's, which decides
/// whether the content is dark.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub name: &'static str,
    pub style: Style,
    pub text: u32,
    pub numbers: Numbers,
    pub backdrop: u32,
}

/// The box's color until one is chosen for it.
pub const BOX_COLOR: u32 = 0xFF1F_2227;
/// The taskbar's own color in each theme, which a translucent box is seen
/// over.
const DARK_TASKBAR: u32 = 0xFF20_2020;
const LIGHT_TASKBAR: u32 = 0xFFF3_F3F3;
/// Below this opacity a box is as good as not there, so a card draws no
/// border round it.
const FAINT: u32 = 0x10;

impl Look {
    /// This look on a box of `color`, or of BOX_COLOR with none chosen, on a
    /// light or dark taskbar: the content turns dark on a light backdrop,
    /// the values, labels and tracks black at their usual opacity, so they
    /// read on a light color or on a light taskbar showing through.
    pub fn colored(&self, color: Option<u32>, light_taskbar: bool) -> Look {
        let color = color.unwrap_or(BOX_COLOR);
        let taskbar = if light_taskbar {
            LIGHT_TASKBAR
        } else {
            DARK_TASKBAR
        };
        let backdrop = over(color, taskbar);
        let dark = dark_content(backdrop);
        let black = |argb: u32| if dark { argb & 0xFF00_0000 } else { argb };
        Look {
            style: Style {
                box_color: color,
                track_color: black(self.style.track_color),
                label_color: black(self.style.label_color),
                ..self.style
            },
            text: black(self.text),
            backdrop,
            ..*self
        }
    }
    /// Whether the content is dark: on a light backdrop.
    pub fn dark(&self) -> bool {
        dark_content(self.backdrop)
    }
    /// A service's color as this look draws it: deepened until it stands
    /// out on a light backdrop, as the popup does in light mode, and as it
    /// is on a dark one. The warning colors do not go through this.
    pub fn service_color(&self, color: u32) -> u32 {
        if self.dark() {
            legible(color, self.backdrop)
        } else {
            color
        }
    }
}

/// Whether what is drawn on an opaque `backdrop` reads better dark: above
/// the luminance where black and white text stand out alike.
fn dark_content(backdrop: u32) -> bool {
    luminance(backdrop) > 0.179
}

/// `fore` laid over an opaque `back` by its opacity, opaque.
pub fn over(fore: u32, back: u32) -> u32 {
    let alpha = f64::from(fore >> 24) / 255.0;
    let channel = |shift: u32| {
        let f = f64::from((fore >> shift) & 0xFF);
        let b = f64::from((back >> shift) & 0xFF);
        ((f * alpha + b * (1.0 - alpha)).round() as u32) << shift
    };
    0xFF00_0000 | channel(16) | channel(8) | channel(0)
}

/// An ARGB color's relative luminance, 0 black to 1 white, and the contrast
/// ratio of two, 1 to 21, as WCAG counts them.
pub fn luminance(argb: u32) -> f64 {
    let channel = |shift: u32| {
        let c = f64::from((argb >> shift) & 0xff) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

pub fn contrast(a: u32, b: u32) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// `color` as it is drawn on `background`: as it is when it stands out 3:1,
/// and otherwise darker on a light background or lighter on a dark one
/// until it does. Service colors are pale for a dark taskbar, so GPT's
/// white would vanish on a light one.
pub fn legible(color: u32, background: u32) -> u32 {
    let toward = if luminance(background) > 0.5 {
        0.0
    } else {
        255.0
    };
    let mut color = color;
    for _ in 0..30 {
        if contrast(color, background) >= 3.0 {
            break;
        }
        let channel = |shift: u32| {
            let c = f64::from((color >> shift) & 0xff);
            ((c + (toward - c) * 0.1).round() as u32) << shift
        };
        color = color & 0xFF00_0000 | channel(16) | channel(8) | channel(0);
    }
    color
}

/// A color as the settings file, the settings window and the popup write
/// it, `#AARRGGBB`, and one read back from that, in either case; anything
/// else is None.
pub fn argb_text(argb: u32) -> String {
    format!("#{argb:08X}")
}
pub fn parse_argb(text: &str) -> Option<u32> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(digits, 16).ok()
}

const SOFT_WHITE: u32 = 0xF2FF_FFFF;
/// Labels name a limit; the value beside it is what is read.
const MUTED_WHITE: u32 = 0x99FF_FFFF;
/// A text's box ends at the baseline, so the gap below it is the whole space
/// between the digits and a gauge under them. The box is 40 of the taskbar's
/// 48 DIP. Each cell is as wide as its own gauges, so a service with one
/// limit leaves no gap after it.
const NO_CARD: Style = Style {
    box_color: BOX_COLOR,
    track_color: 0x1FFF_FFFF,
    font: 1300,
    weight: 400,
    height: 4000,
    padding: 1000,
    gap: 1400,
    gauge: 200,
    gauge_gap: 600,
    radius: 800,
    layout: 0,
    gauge_width: 0,
    cell_color: 0,
    cell_border: 0,
    cell_radius: 0,
    cell_padding: 0,
    cell_inset: 0,
    motion: 200,
    icon: 1600,
    icon_gap: 600,
    uniform: 0,
    fill_end: 0,
    text_end: 1,
    arrange: 1,
    spacing: 1000,
    label_gap: 400,
    ring: 0,
    label_color: MUTED_WHITE,
    mark_weight: 700,
};

/// The looks the design compared side by side. Each shows an icon for the
/// service; one box or a card per service is chosen apart, as the
/// background.
pub const LOOKS: [Look; 5] = [
    // The eight principles: a rounded box, a 2 DIP gauge on a faint track, a
    // color per service on the fill only; a limit's gauges side by side.
    Look {
        name: "slim",
        style: NO_CARD,
        text: SOFT_WHITE,
        numbers: Numbers::Percent,
        backdrop: BOX_COLOR,
    },
    // A pill with each limit's gauge beside its text, one limit a row.
    Look {
        name: "inline",
        style: Style {
            track_color: 0x26FF_FFFF,
            padding: 1200,
            gap: 1600,
            gauge: 400,
            gauge_gap: 800,
            radius: 1400,
            layout: 1,
            gauge_width: 3200,
            arrange: 0,
            spacing: 400,
            ..NO_CARD
        },
        text: SOFT_WHITE,
        numbers: Numbers::Percent,
        backdrop: BOX_COLOR,
    },
    // Each limit's text over its bar, one limit above the other, in a smaller
    // font to fit two.
    Look {
        name: "stack",
        style: Style {
            font: 1100,
            gauge_gap: 300,
            arrange: 0,
            spacing: 300,
            ..NO_CARD
        },
        text: SOFT_WHITE,
        numbers: Numbers::Percent,
        backdrop: BOX_COLOR,
    },
    // A drawn dial before each value, which a glyph could not do evenly or in
    // color; one limit a row.
    Look {
        name: "pie",
        style: Style {
            track_color: 0x33FF_FFFF,
            layout: 2,
            gauge_width: 1100,
            gauge_gap: 500,
            motion: 0,
            arrange: 0,
            spacing: 300,
            ..NO_CARD
        },
        text: SOFT_WHITE,
        numbers: Numbers::Digits,
        backdrop: BOX_COLOR,
    },
    // One dial per service with a ring per limit, the first outermost, and
    // the values stacked beside it.
    Look {
        name: "rings",
        style: Style {
            track_color: 0x33FF_FFFF,
            font: 1100,
            layout: 2,
            gauge_width: 2400,
            gauge_gap: 600,
            motion: 0,
            arrange: 2,
            spacing: 150,
            ring: 300,
            // Values start their column, right by the rings.
            text_end: 0,
            ..NO_CARD
        },
        text: SOFT_WHITE,
        numbers: Numbers::Digits,
        backdrop: BOX_COLOR,
    },
];

/// The look the product starts with until one is chosen in settings.
pub const DEFAULT: &Look = &LOOKS[0];

pub fn find(name: &str) -> Option<&'static Look> {
    LOOKS.iter().find(|look| look.name == name)
}

/// Whether the services share one box or each has its own card, chosen apart
/// from the look so any look can be drawn either way.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Background {
    Box,
    Cards,
}

/// A card's faint border, the room between cards, and between the strip's
/// edge and the first card.
const CARD_BORDER: u32 = 0x33FF_FFFF;
const CARD_GAP: u32 = 600;
const CARD_EDGE: u32 = 200;

impl Background {
    pub const ALL: [Background; 2] = [Background::Box, Background::Cards];
    pub fn name(self) -> &'static str {
        match self {
            Background::Box => "box",
            Background::Cards => "cards",
        }
    }
    pub fn find(name: &str) -> Option<Background> {
        Self::ALL
            .into_iter()
            .find(|background| background.name() == name)
    }
    /// The look's style drawn this way. A card takes the box's color, corners
    /// and padding, and a faint border, as tall as the box, which goes clear.
    pub fn apply(self, style: Style) -> Style {
        match self {
            Background::Box => style,
            Background::Cards => Style {
                box_color: 0,
                padding: CARD_EDGE,
                gap: CARD_GAP,
                radius: 0,
                cell_color: style.box_color,
                // A box too faint to see has no border floating round
                // each card.
                cell_border: if style.box_color >> 24 < FAINT {
                    0
                } else {
                    CARD_BORDER
                },
                cell_radius: style.radius,
                cell_padding: style.padding,
                cell_inset: 0,
                ..style
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_look_is_one_the_hook_takes() {
        for look in &LOOKS {
            for background in Background::ALL {
                let style = background.apply(look.style);
                assert!(style.fits(), "{} is out of range", look.name);
            }
        }
        // A box taller than the taskbar at 100% is refused, and so is a
        // layout or an arrangement the hook does not know.
        for wrong in [
            Style {
                height: 4801,
                ..DEFAULT.style
            },
            Style {
                layout: 3,
                ..DEFAULT.style
            },
            Style {
                fill_end: 2,
                ..DEFAULT.style
            },
            Style {
                arrange: 3,
                ..DEFAULT.style
            },
        ] {
            assert!(!wrong.fits());
        }
    }

    #[test]
    fn cells_take_their_own_width_in_a_taller_box() {
        // Values line up at the end of their column, but right by the rings
        // beside them; the box is 40 of the taskbar's 48 DIP, and a cell is
        // as wide as its own gauges. The fill mode, not the look, decides
        // where the color sits.
        for look in &LOOKS {
            let style = look.style;
            let end = u32::from(style.arrange != 2);
            assert_eq!((style.text_end, style.fill_end, style.uniform), (end, 0, 0));
            assert!(style.icon > 0, "{}", look.name);
            assert_eq!(style.height - 2 * style.cell_inset, 4000, "{}", look.name);
        }
        assert_eq!(Fill::find("left"), Some(Fill::Left));
        assert_eq!(Fill::find("used"), Some(Fill::Used));
        assert!(Fill::find("remaining").is_none());
    }

    #[test]
    fn cards_draw_the_box_once_per_service() {
        // The box's color, corners and padding move to each service's card,
        // as tall as the box; the look is otherwise the same.
        for look in &LOOKS {
            let style = look.style;
            assert_eq!(Background::Box.apply(style), style);
            let cards = Background::Cards.apply(style);
            assert_eq!(cards.box_color, 0);
            assert_eq!(
                (cards.cell_color, cards.cell_radius, cards.cell_padding),
                (style.box_color, style.radius, style.padding)
            );
            assert_eq!(cards.height - 2 * cards.cell_inset, style.height);
            assert_eq!(
                (cards.layout, cards.arrange, cards.font),
                (style.layout, style.arrange, style.font)
            );
        }
        assert_eq!(Background::find("cards"), Some(Background::Cards));
        assert!(Background::find("card").is_none());
    }

    #[test]
    fn a_box_of_its_own_color_turns_dark_on_a_light_one() {
        // The default color changes nothing. A light color makes the
        // values, labels and tracks black at their opacity, and deepens a
        // pale service color; a dark one leaves them; a translucent one
        // follows the taskbar it shows through.
        let slim = DEFAULT;
        let plain = slim.colored(None, false);
        assert_eq!(plain.style, slim.style);
        assert_eq!(
            (plain.text, plain.backdrop, plain.dark()),
            (slim.text, BOX_COLOR, false)
        );
        let white = slim.colored(Some(0xFFF0_F0F0), false);
        assert_eq!(white.style.box_color, 0xFFF0_F0F0);
        assert_eq!(white.text, 0xF200_0000);
        assert_eq!(white.style.label_color, 0x9900_0000);
        assert_eq!(white.style.track_color, 0x1F00_0000);
        assert!(white.dark());
        let navy = slim.colored(Some(0xFF10_2040), true);
        assert_eq!(
            (navy.text, navy.style.label_color),
            (slim.text, slim.style.label_color)
        );
        assert!(!navy.dark());
        assert!(slim.colored(Some(0x1000_0000), true).dark());
        assert!(!slim.colored(Some(0x1000_0000), false).dark());
        assert!(slim.colored(Some(0x80FF_FFFF), false).dark());
        let gpt = 0xFFF2_F2F2;
        assert!(contrast(white.service_color(gpt), white.backdrop) >= 3.0);
        assert_eq!(navy.service_color(gpt), gpt);
        // The words sent carry the color first.
        assert_eq!(white.style.words()[0], 0xFFF0_F0F0);
        // Cards take the color, with no border round a box too faint to see.
        let cards = Background::Cards.apply(white.style);
        assert_eq!(
            (cards.cell_color, cards.cell_border),
            (0xFFF0_F0F0, CARD_BORDER)
        );
        let gone = Background::Cards.apply(slim.colored(Some(0x0F00_0000), false).style);
        assert_eq!((gone.cell_color, gone.cell_border), (0x0F00_0000, 0));
        let faint = Background::Cards.apply(slim.colored(Some(0x1000_0000), false).style);
        assert_eq!(faint.cell_border, CARD_BORDER);
    }

    #[test]
    fn a_color_is_written_as_hex_and_read_back() {
        // #AARRGGBB, upper case out and either case in.
        assert_eq!(argb_text(0x80FF_A001), "#80FFA001");
        assert_eq!(argb_text(BOX_COLOR), "#FF1F2227");
        assert_eq!(parse_argb("#80ffa001"), Some(0x80FF_A001));
        assert_eq!(parse_argb("#80FFA001"), Some(0x80FF_A001));
        for wrong in ["80FFA001", "#80FFA0", "#80FFA0011", "#80FFA00G", "", "#"] {
            assert_eq!(parse_argb(wrong), None, "{wrong}");
        }
        // Laid over the taskbar by its opacity.
        assert_eq!(over(0xFF11_2233, 0xFFFF_FFFF), 0xFF11_2233);
        assert_eq!(over(0x0011_2233, 0xFFF3_F3F3), 0xFFF3_F3F3);
        assert_eq!(over(0x80FF_FFFF, 0xFF00_0000), 0xFF80_8080);
    }

    #[test]
    fn looks_are_found_by_name() {
        let names: Vec<_> = LOOKS.iter().map(|l| l.name).collect();
        assert_eq!(names, ["slim", "inline", "stack", "pie", "rings"]);
        assert_eq!(DEFAULT.name, "slim");
        assert_eq!(find("rings").map(|l| l.name), Some("rings"));
        assert!(find("card").is_none());
        assert!(find("cycle").is_none());
    }
}
