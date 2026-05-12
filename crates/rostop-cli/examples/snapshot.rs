//! Render a single frame of the demo TUI to stdout. Used to generate the
//! ASCII screenshot in the README.

use std::time::Duration;

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use rostop_cli::test_support::{render_once, AppHandle};

fn main() {
    let mut app = AppHandle::demo();
    app.tick(Duration::from_millis(400));
    // Second tick gives sparklines time to fill in.
    app.tick(Duration::from_millis(400));

    let backend = TestBackend::new(110, 28);
    let mut terminal = Terminal::new(backend).unwrap();
    render_once(&mut terminal, &mut app);

    let buf = terminal.backend().buffer().clone();
    for y in 0..buf.area.height {
        let mut line = String::new();
        for x in 0..buf.area.width {
            line.push_str(buf[(x, y)].symbol());
        }
        println!("{line}");
    }
}
