use std::ops::Deref;
use std::sync::{Arc, RwLock};

use druid::{Affine, BoxConstraints, Data, Env, Event, EventCtx, LayoutCtx, Lens, LifeCycle,
            LifeCycleCtx, MouseButton, PaintCtx, Point, Rect, RenderContext, Size, TextLayout,
            UpdateCtx, Vec2, Widget};
use druid::scroll_component::ScrollComponent;
use druid::theme::SELECTION_COLOR;
use druid::widget::Viewport;
use log::trace;

use crate::colors::ALT_BACKGROUND_COLOR;
use crate::db::{Track, TrackField};
use crate::WrappedTrackList;

// equal space on the top/bottom
const SPACER_SIZE: f64 = 6.0;

#[derive(Clone, Data, Lens)]
pub struct TrackListData {
    tracks: WrappedTrackList,
    selected_tracks: Arc<RwLock<Vec<usize>>>,
}

impl TrackListData {
    pub fn new(tracks: Vec<Track>) -> Self {
        TrackListData {
            tracks: Arc::new(RwLock::new(tracks)),
            selected_tracks: Arc::new(RwLock::new(Vec::new())),
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
        let height = n_rows as f64 * self.row_height();

        Size::new(avail_size.width, avail_size.height.max(height))
    }

    fn row_height(&self) -> f64 {
        self.dummy_text.size().height + SPACER_SIZE
    }
}

impl Widget<TrackListData> for TrackList {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut TrackListData, env: &Env) {
        println!("{:#?}", event);
        self.scroll.event(self.viewport.as_mut().unwrap_or(&mut Viewport::default()), ctx, event, env);
        self.scroll.handle_scroll(self.viewport.as_mut().unwrap_or(&mut Viewport::default()), ctx, event, env);

        if !ctx.is_handled() {
            match event {
                Event::MouseDown(evt) => {
                    if let MouseButton::Left = evt.button {
                        // Set selection
                        let abs_pos = self.viewport.unwrap().rect.y0 + evt.pos.y;
                        let mut tr = data.selected_tracks.write().unwrap();
                        tr.clear();
                        tr.push((abs_pos / self.row_height()) as usize);
                        trace!("Rows selected: {:?}", tr.deref());
                        ctx.request_paint();
                        ctx.set_handled();
                    }
                }
                _ => ()
            }
        }
    }

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, data: &TrackListData, env: &Env) {
        self.scroll.lifecycle(ctx, event, env);

        if let LifeCycle::WidgetAdded = event {
            self.update_children(data);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx, _old_data: &TrackListData, data: &TrackListData, _env: &Env) {
        self.update_children(data);
        ctx.request_layout();
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

    fn paint(&mut self, ctx: &mut PaintCtx, data: &TrackListData, env: &Env) {
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

        let mut y = SPACER_SIZE / 2.;

        for row in start_row..=end_row {
            if row >= self.children.len() / self.columns.len() {
                continue;
            }

            let background_rect = Rect::from_origin_size(
                Point::new(0., y - (SPACER_SIZE / 2.)),
                Size::new(ctx.size().width, self.row_height()),
            );

            // Draw background fill for odd numbered rows/selected
            if row % 2 != 0 {
                ctx.fill(background_rect, &env.get(ALT_BACKGROUND_COLOR));
            }

            if data.selected_tracks.read().unwrap().contains(&row) {
                ctx.fill(background_rect, &env.get(SELECTION_COLOR));
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