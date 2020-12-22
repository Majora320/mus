use std::sync::{Arc, RwLock};

use druid::{AppLauncher, Color, Data, Lens, RenderContext, Size, Widget, WidgetExt, WindowDesc};
use druid::widget::{Flex, Label, Painter};
use rodio::{OutputStream, Sink};

use crate::db::{Database, Track};
use crate::tracklist::{TrackList, TrackListData};
use crate::colors::ALT_BACKGROUND_COLOR;

mod db;
mod tracklist;
mod colors;

type WrappedTrackList = Arc<RwLock<Vec<Track>>>;

#[derive(Clone, Data, Lens)]
struct AppData {
    db: Arc<RwLock<Database>>,
    stream: Arc<RwLock<OutputStream>>,
    sink: Arc<RwLock<Sink>>,
    main_tracklist_data: TrackListData
}

fn main() {
    pretty_env_logger::init();

    let mut db = Database::new().expect("Launch failed.");
    let (stream, handle) = OutputStream::try_default().unwrap();
    let sink = Sink::try_new(&handle).unwrap();

    if db.libraries().unwrap().len() <= 1 {
        let library = db.add_library("/data/Music".to_string(), "Music".to_string()).unwrap();
        db.scan_library(library, true).unwrap();
    }

    let tracks = db.dump_all_tracks().expect("Could not dump tracks.")
        .into_iter().collect();

    let initial_state = AppData {
        db: Arc::new(RwLock::new(db)),
        stream: Arc::new(RwLock::new(stream)),
        sink: Arc::new(RwLock::new(sink)),
        main_tracklist_data: TrackListData::new(tracks)
    };

    let main_window = WindowDesc::new(make_ui)
        .title("mus")
        .window_size(Size::new(1920.0, 1080.0));

    AppLauncher::with_window(main_window)
        .configure_env(|env, _state| {
            env.set(ALT_BACKGROUND_COLOR, Color::grey8(60));
        })
        .launch(initial_state)
        .expect("launch failed");
}

fn make_ui() -> impl Widget<AppData> {
    let sep = Painter::new(|ctx, _data, _env| {
        let bounds = ctx.size().to_rect();
        ctx.fill(bounds, &Color::WHITE);
    });

    let bottom_bar = Label::new("Welcome to mus v0.0.0");

    let table = TrackList::new();

    let main_view = Flex::column()
        .with_flex_child(Flex::row()
            .with_flex_child(
                table.lens(AppData::main_tracklist_data)
                    .padding((5., 5.)),
                1.0), 1.0)
        .with_child(sep
            .fix_height(2.)
            .expand_width())
        .with_child(bottom_bar
            .padding(4.)
            .expand_width()
            .align_left());

    main_view
}
