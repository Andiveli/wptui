use wp_tui::app::App;
use wp_tui::app::message_action_diagnostics::debug_enabled as mac_debug_enabled;
use wp_tui::app::presence::debug_enabled as presence_debug_enabled;

use clap::Parser;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long)]
    phone: Option<String>,
}

fn main() {
    let _ = tui_logger::init_logger(tui_logger::LevelFilter::Trace);
    tui_logger::set_default_level(tui_logger::LevelFilter::Warn);

    let args = Args::parse();

    let mut app = App::default();
    app.enable_message_action_diagnostics(mac_debug_enabled(
        std::env::var("WPTUI_MESSAGE_ACTION_DEBUG").ok().as_deref(),
    ));
    app.enable_presence_diagnostics(presence_debug_enabled(
        std::env::var("WPTUI_PRESENCE_DEBUG").ok().as_deref(),
    ));
    app.run(args.phone);
}
