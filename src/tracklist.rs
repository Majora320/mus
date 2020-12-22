use std::sync::{Arc, RwLock};

use druid::{BoxConstraints, Color, Data, Env, Event, EventCtx, LayoutCtx, Lens, LifeCycle, LifeCycleCtx, PaintCtx, Point, Rect, RenderContext, Size, TextLayout, UpdateCtx, Widget, Affine, Vec2};
use druid::widget::Viewport;

use crate::db::{Track, TrackField};
use crate::WrappedTrackList;
use druid::scroll_component::ScrollComponent;

const SPACER_SIZE: f64 = 5.0;

#[derive(Clone, Data, Lens)]
pub struct TrackListData {
    tracks: WrappedTrackList,
    selected_track: isize,
}

impl TrackListData {
    pub fn new(tracks: Vec<Track>) -> Self {
        TrackListData {
            tracks: Arc::new(RwLock::new(tracks)),
            selected_track: -1,
        }
    }
}

pub struct TrackList {
    children: Vec<TextLayout<String>>,
    columns: Vec<(TrackField, f64)>,
    scroll: ScrollComponent,
    viewport: Option<Viewport>,
    dummy_text: TextLayout<String>,
}

impl TrackList {
    pub fn new() -> Self {
        // Viewport must be Some after LifeCycle::WidgetAdded
        TrackList {
            children: Vec::new(),
            columns: Vec::new(),
            scroll: ScrollComponent::new(),
            viewport: None,
            dummy_text: TextLayout::from_text("dummy"),
        }
    }

    fn update_children(&mut self, data: &TrackListData) {
        let data = data.tracks.read().unwrap();

        self.children = Vec::new();
        self.columns = vec![(TrackField::Title, 0.5), (TrackField::Artist, 0.5)];

        for elem in data.iter() {
            self.children.push(TextLayout::from_text(elem.title().unwrap_or_default()));
            self.children.push(TextLayout::from_text(elem.artist().unwrap_or_default()));
        }
    }

    fn total_size(&self, avail_size: Size) -> Size {
        let n_rows = self.children.len() / self.columns.len();
        let height = n_rows as f64 * self.row_height() + SPACER_SIZE;

        Size::new(avail_size.width, avail_size.height.max(height))
    }

    fn row_height(&self) -> f64 {
        self.dummy_text.size().height + SPACER_SIZE
    }
}

impl Widget<TrackListData> for TrackList {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, _data: &mut TrackListData, env: &Env) {
        self.scroll.event(self.viewport.as_mut().unwrap_or(&mut Viewport::default()), ctx, event, env);
        self.scroll.handle_scroll(self.viewport.as_mut().unwrap_or(&mut Viewport::default()), ctx, event, env);

        if !ctx.is_handled() {
            match event {
                // TODO
            }
        }
    }

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, data: &TrackListData, env: &Env) {
        self.scroll.lifecycle(ctx, event, env);

        if let LifeCycle::WidgetAdded = event {
            self.update_children(data);
        }
    }

    fn update(&mut self, _ctx: &mut UpdateCtx, _old_data: &TrackListData, data: &TrackListData, _env: &Env) {
        self.update_children(data);
    }

    // This widget DOES NOT WORK with infinite-width containers
    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, _data: &TrackListData, env: &Env) -> Size {
        self.dummy_text.rebuild_if_needed(ctx.text(), env);
        for elem in &mut self.children {
            elem.rebuild_if_needed(ctx.text(), env);
        }

        self.viewport = Some(Viewport {
            content_size: self.total_size(bc.max()),
            rect: if let Some(v) = self.viewport {
                Rect::new(0., v.rect.y0, bc.max().width, v.rect.y0 + bc.max().height)
            } else {
                Rect::new(0., 0., bc.max().width, bc.max().height)
            },
        });

        bc.max()
    }

    fn paint(&mut self, ctx: &mut PaintCtx, _data: &TrackListData, env: &Env) {
        // Now we have to draw the subset of the screen that fits into the viewport
        // It's a bit of a pain to do this custom but otherwise performance tanks hard

        let avail_width = ctx.size().width - SPACER_SIZE; // Accounting for the right spacer
        let viewport = self.viewport.expect("Something is seriously wrong with the layout code...");

        let div = viewport.rect.y0 / self.row_height();
        let div2 = viewport.rect.y1 / self.row_height();
        let start_row = div as usize;
        let end_row = div2 as usize;
        let offset = (div - div.floor()) * self.row_height();

        ctx.save().unwrap();
        let size = ctx.size();
        ctx.clip(Rect::from_origin_size(Point::default(), size));
        ctx.transform(Affine::translate(Vec2 {
            x: 0.,
            y: -offset,
        }));

        // Draw row seperators, starting from above start_row and ending above end_row + 1
        // We don't strictly need to draw the extra seperator here, but it avoids having to special
        // case the end of the list and it'll just get clipped off otherwise.
        for row in 0..=(end_row - start_row + 1) {
            let point = Point::new(2., row as f64 * self.row_height() + 2.);
            let width = ctx.size().width - 4.;
            ctx.fill(Rect::from_origin_size(point, Size::new(width, 1.)), &Color::grey8(128));
        }

        // Draw column separators.
        let mut x = 2.;
        for col in 0..=self.columns.len() {
            // col 0 is the leftmost separator, not attached to any column
            if col != 0 {
                x += self.columns[col - 1].1 * avail_width;
            }

            let point = Point::new(x, 2.);
            let height = ctx.size().height - 4. + offset; // 2 from the top and the bottom
            ctx.fill(Rect::from_origin_size(point, Size::new(1., height)), &Color::grey8(128));
        }

        // Now comes the "fun" part
        let mut y = SPACER_SIZE;

        for row in start_row..end_row {
            if row >= self.children.len() / self.columns.len() {
                continue;
            }

            let mut x = SPACER_SIZE;

            for col in 0..self.columns.len() {
                let point = Point::new(x, y);
                let size = Size::new(avail_width * self.columns[col].1 - SPACER_SIZE,
                                     self.row_height());
                let child = &self.children[row * self.columns.len() + col];
                let clip_rect = Rect::from_origin_size(point, size);

                ctx.with_save(|ctx| {
                    ctx.clip(clip_rect);
                    child.draw(ctx, point);
                });

                x += size.width + SPACER_SIZE;
            }

            y += self.row_height();
        }

        ctx.restore().unwrap();

        self.scroll.draw_bars(ctx, self.viewport.as_ref().unwrap(), env);
    }
}