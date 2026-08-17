use wp_tui::app::App;
use wp_tui::app::message_action_diagnostics::debug_enabled as mac_debug_enabled;
use wp_tui::app::presence::debug_enabled as presence_debug_enabled;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long)]
    phone: Option<String>,
    #[clap(long, hide = true)]
    no_view: bool,
    /// Write a persistent panic report here (default: ./wptui-crash.log).
    #[clap(
        long,
        hide = true,
        value_name = "PATH",
        default_value = "wptui-crash.log"
    )]
    crash_log: PathBuf,
}

fn main() {
    let _ = tui_logger::init_logger(tui_logger::LevelFilter::Trace);
    tui_logger::set_default_level(tui_logger::LevelFilter::Warn);

    let args = Args::parse();
    wp_tui::crash_diagnostics::install(args.crash_log.clone());

    let mut app = App::default();
    app.initialize_read_receipts(!args.no_view);
    app.enable_message_action_diagnostics(mac_debug_enabled(
        std::env::var("WPTUI_MESSAGE_ACTION_DEBUG").ok().as_deref(),
    ));
    app.enable_presence_diagnostics(presence_debug_enabled(
        std::env::var("WPTUI_PRESENCE_DEBUG").ok().as_deref(),
    ));
    app.run(args.phone);
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use super::Args;

    #[test]
    fn no_view_is_accepted_but_hidden_from_long_help() {
        let args = Args::try_parse_from(["wp-tui", "--no-view"]).unwrap();
        assert!(args.no_view);
        assert!(
            !Args::command()
                .render_long_help()
                .to_string()
                .contains("--no-view")
        );
        assert!(
            !Args::command()
                .render_long_help()
                .to_string()
                .contains("--crash-log")
        );
    }
}
