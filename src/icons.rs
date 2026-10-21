//! The original PinePaper line icons, drawn as vectors without a theme or icon font.
use crate::{
    model::Stroke,
    render::{Box2, Surface, View},
};
use std::f64::consts::{PI, TAU};

#[derive(Clone, Copy)]
pub enum Icon {
    Back,
    Next,
    Close,
    Plus,
    PlusCircle,
    ChevronDown,
    List,
    Menu,
    Settings,
    Search,
    More,
    Grid,
    Folder,
    Book,
    Pen,
    Highlighter,
    Brush,
    Eraser,
    Lasso,
    Text,
    Diagram,
    Select,
    Magnet,
    Undo,
    Redo,
    Download,
    Bookmark,
    Tag,
    Trash,
    Check,
    Hide,
    Sort,
    Expand,
    Contract,
    Backspace,
    Refresh,
    Share,
}

impl Icon {
    pub fn draw(self, surface: &mut Surface, bounds: Box2, color: u32) {
        let size = bounds.w.min(bounds.h);
        let view = View {
            x: bounds.x + (bounds.w - size) / 2.,
            y: bounds.y + (bounds.h - size) / 2.,
            zoom: size / 24.,
            clip: surface.clip,
        };
        surface.begin_stroke();
        let mut p = Painter {
            surface,
            view,
            stroke: Stroke {
                pen: "Fineliner".into(),
                width: 1.5 * (24. / size).min(1.),
                color: (color & 0xff) as f64 / 255.,
                ..Stroke::default()
            },
            mirror: matches!(self, Self::Next | Self::Redo),
        };
        match self {
            Self::Magnet => {
                p.path(&[(3., 13.), (3., 3.), (8., 3.), (8., 13.)]);
                p.arc((12., 13.), (4., 4.), 0., PI);
                p.path(&[(16., 13.), (16., 3.), (21., 3.), (21., 13.)]);
                p.arc((12., 13.), (9., 9.), 0., PI);
                p.path(&[(3., 8.), (8., 8.)]);
                p.path(&[(16., 8.), (21., 8.)]);
            }
            Self::Select => {
                p.path(&[
                    (5., 2.),
                    (5., 20.),
                    (10., 15.),
                    (14., 23.),
                    (18., 21.),
                    (14., 13.),
                    (21., 13.),
                    (5., 2.),
                ]);
            }
            Self::Diagram => {
                p.rect(2., 2., 8., 7.);
                p.path(&[(6., 9.), (6., 17.), (14., 17.)]);
                p.path(&[(11., 14.), (14., 17.), (11., 20.)]);
                p.arc((19., 17.), (4., 4.), 0., TAU);
            }
            Self::Share => {
                p.path(&[(8., 11.), (16., 6.)]);
                p.path(&[(8., 13.), (16., 18.)]);
                p.arc((5., 12.), (3., 3.), 0., TAU);
                p.arc((19., 5.), (3., 3.), 0., TAU);
                p.arc((19., 19.), (3., 3.), 0., TAU);
            }
            Self::Back | Self::Next => {
                p.path(&[(14., 5.), (7., 12.), (14., 19.)]);
                p.path(&[(7., 12.), (21., 12.)]);
            }
            Self::Close => {
                p.path(&[(6., 6.), (18., 18.)]);
                p.path(&[(18., 6.), (6., 18.)]);
            }
            Self::Plus | Self::PlusCircle => {
                if matches!(self, Self::PlusCircle) {
                    p.arc((12., 12.), (10., 10.), 0., TAU);
                }
                p.path(&[(12., 5.), (12., 19.)]);
                p.path(&[(5., 12.), (19., 12.)]);
            }
            Self::ChevronDown => p.path(&[(5., 9.), (12., 16.), (19., 9.)]),
            Self::List => {
                for y in [5., 12., 19.] {
                    p.rect(3., y - 1., 2., 2.);
                    p.path(&[(9., y), (22., y)]);
                }
            }
            Self::Menu => {
                for y in [6., 12., 18.] {
                    p.path(&[(4., y), (20., y)]);
                }
            }
            Self::Settings => {
                for (x, y) in [(8., 5.), (16., 12.), (8., 19.)] {
                    p.path(&[(3., y), (x - 2.6, y)]);
                    p.path(&[(x + 2.6, y), (21., y)]);
                    p.arc((x, y), (2.6, 2.6), 0., TAU);
                }
            }
            Self::Search => {
                p.arc((10., 10.), (6., 6.), 0., TAU);
                p.path(&[(15., 15.), (21., 21.)]);
            }
            Self::More => {
                for y in [5., 12., 19.] {
                    p.dot((12., y), 1.3);
                }
            }
            Self::Grid => {
                for x in [4., 14.] {
                    for y in [4., 14.] {
                        p.rect(x, y, 6., 6.);
                    }
                }
            }
            Self::Folder => {
                p.path(&[
                    (3., 7.),
                    (10., 7.),
                    (12., 10.),
                    (21., 10.),
                    (21., 20.),
                    (3., 20.),
                    (3., 7.),
                ]);
                p.path(&[(3., 7.), (3., 4.), (10., 4.), (12., 7.), (20., 7.)]);
            }
            Self::Book => {
                p.rect(4., 3., 16., 18.);
                p.path(&[(8., 3.), (8., 21.)]);
                for y in [8., 12., 16.] {
                    p.path(&[(11., y), (17., y)]);
                }
            }
            Self::Pen => {
                p.path(&[
                    (4., 20.),
                    (6., 14.),
                    (16., 4.),
                    (20., 8.),
                    (10., 18.),
                    (4., 20.),
                ]);
                p.path(&[(14., 6.), (18., 10.)]);
            }
            Self::Brush => {
                p.path(&[(10., 15.), (18., 3.), (22., 5.), (14., 17.), (10., 15.)]);
                p.path(&[
                    (10., 15.),
                    (7., 15.),
                    (5., 17.),
                    (4., 20.),
                    (2., 22.),
                    (8., 22.),
                    (12., 20.),
                    (14., 17.),
                ]);
                p.path(&[(6., 20.), (8., 18.)]);
            }
            Self::Highlighter => {
                p.path(&[(4., 17.), (9., 12.), (13., 16.), (8., 21.), (4., 17.)]);
                p.path(&[(8., 11.), (16., 3.), (22., 9.), (14., 17.)]);
                p.path(&[(3., 22.), (12., 22.)]);
            }
            Self::Eraser => {
                p.path(&[
                    (3., 15.),
                    (14., 4.),
                    (21., 11.),
                    (11., 21.),
                    (9., 21.),
                    (3., 15.),
                ]);
                p.path(&[(8., 10.), (15., 17.)]);
                p.path(&[(11., 21.), (21., 21.)]);
            }
            Self::Lasso => {
                for n in 0..12 {
                    let start = n as f64 * TAU / 12.;
                    p.arc((12., 10.), (8., 5.6), start, start + TAU / 24.);
                }
                p.path(&[(6., 16.), (9., 20.), (7., 23.)]);
            }
            Self::Text => {
                p.path(&[(4., 8.), (4., 5.), (20., 5.), (20., 8.)]);
                p.path(&[(12., 5.), (12., 21.)]);
                p.path(&[(8., 21.), (16., 21.)]);
            }
            Self::Undo | Self::Redo => {
                p.path(&[(4., 4.), (4., 11.), (11., 11.)]);
                p.arc((12., 13.), (8., 8.), PI * 1.15, PI * 2.55);
            }
            Self::Download => {
                p.path(&[(12., 3.), (12., 15.)]);
                p.path(&[(7., 10.), (12., 15.), (17., 10.)]);
                p.path(&[(4., 16.), (4., 21.), (20., 21.), (20., 16.)]);
            }
            Self::Bookmark => {
                p.path(&[
                    (6., 3.),
                    (18., 3.),
                    (18., 22.),
                    (12., 17.),
                    (6., 22.),
                    (6., 3.),
                ]);
            }
            Self::Tag => {
                p.path(&[
                    (3., 4.),
                    (12., 4.),
                    (22., 14.),
                    (14., 22.),
                    (3., 11.),
                    (3., 4.),
                ]);
                p.arc((8., 8.), (1.3, 1.3), 0., TAU);
            }
            Self::Trash => {
                p.rect(6., 7., 12., 14.);
                p.path(&[(3., 7.), (21., 7.)]);
                p.path(&[(9., 3.), (15., 3.)]);
                p.path(&[(10., 11.), (10., 17.)]);
                p.path(&[(14., 11.), (14., 17.)]);
            }
            Self::Check => p.path(&[(4., 12.), (9., 17.), (21., 5.)]),
            Self::Hide => {
                p.path(&[(4., 6.), (4., 18.)]);
                p.path(&[(16., 6.), (10., 12.), (16., 18.)]);
            }
            Self::Sort => {
                p.path(&[(6., 4.), (6., 20.)]);
                p.path(&[(3., 17.), (6., 20.), (9., 17.)]);
                for (y, x) in [(6., 21.), (12., 18.), (18., 15.)] {
                    p.path(&[(12., y), (x, y)]);
                }
            }
            Self::Expand => {
                p.path(&[(4., 9.), (4., 4.), (9., 4.)]);
                p.path(&[(15., 4.), (20., 4.), (20., 9.)]);
                p.path(&[(4., 15.), (4., 20.), (9., 20.)]);
                p.path(&[(15., 20.), (20., 20.), (20., 15.)]);
            }
            Self::Contract => {
                p.path(&[(3., 8.), (8., 8.), (8., 3.)]);
                p.path(&[(16., 3.), (16., 8.), (21., 8.)]);
                p.path(&[(3., 16.), (8., 16.), (8., 21.)]);
                p.path(&[(16., 21.), (16., 16.), (21., 16.)]);
            }
            Self::Backspace => {
                p.path(&[
                    (9., 5.),
                    (21., 5.),
                    (21., 19.),
                    (9., 19.),
                    (2., 12.),
                    (9., 5.),
                ]);
                p.path(&[(12., 9.), (17., 15.)]);
                p.path(&[(17., 9.), (12., 15.)]);
            }
            Self::Refresh => {
                p.arc((12., 12.), (8., 8.), 0.25 * PI, 1.9 * PI);
                p.path(&[(20., 4.), (20., 10.), (14., 10.)]);
            }
        }
    }
}

struct Painter<'a> {
    surface: &'a mut Surface,
    view: View,
    stroke: Stroke,
    mirror: bool,
}
impl Painter<'_> {
    fn path(&mut self, points: &[(f64, f64)]) {
        for pair in points.windows(2) {
            let point = |(x, y): (f64, f64)| [if self.mirror { 24. - x } else { x }, y, 0.5];
            self.surface
                .segment(self.view, point(pair[0]), point(pair[1]), &self.stroke);
        }
    }
    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.path(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)]);
    }
    fn arc(&mut self, center: (f64, f64), radii: (f64, f64), start: f64, end: f64) {
        let steps =
            ((end - start).abs() * radii.0.max(radii.1) * self.view.zoom * self.surface.scale)
                .ceil()
                .max(2.) as usize;
        let points: Vec<_> = (0..=steps)
            .map(|i| {
                let angle = start + (end - start) * i as f64 / steps as f64;
                (
                    center.0 + radii.0 * angle.cos(),
                    center.1 + radii.1 * angle.sin(),
                )
            })
            .collect();
        self.path(&points);
    }
    fn dot(&mut self, center: (f64, f64), radius: f64) {
        let width = self.stroke.width;
        self.stroke.width = radius * 2.;
        self.path(&[center, center]);
        self.stroke.width = width;
    }
}
