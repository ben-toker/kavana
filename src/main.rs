use chrono::{Duration, Utc};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use kavana::{parse_args, Action};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{error::Error, io};

use kavana::timer;

use kavana::config::{load_timer, reset_timer, save_timer, TimerInfo};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args();

    match &args.command {
        Action::Start(arg) => {
            let work = arg.work.into();
            let rest = arg.rest.into();

            let timer_info = TimerInfo {
                start_work: Some(Utc::now()),
                work_duration: Duration::minutes(work),
                rest_duration: Duration::minutes(rest),
                start_rest: Some(Utc::now() + Duration::minutes(work)),
                pause_time: Some(Utc::now()),
                run_state: true,
            };

            save_timer(&timer_info)?;

            let (mut rec, _) = timer::start_timer().await;
            if let Some(message) = rec.recv().await {
                println!("Timer started. {} until break", message);
            }

            Ok(())
        }
        Action::Toggle => {
            let mut timer_info = load_timer()?;

            if timer_info.run_state {
                println!("Timer paused.");
            } else {
                println!("Timer resumed.");
            }

            timer_info.pause_time = Some(chrono::Utc::now());
            timer_info.run_state = !timer_info.run_state;
            save_timer(&timer_info)?;

            Ok(())
        }
        Action::Reset => {
            reset_timer();
            Ok(())
        }
        Action::App => {
            enable_raw_mode()?;
            let mut stdout = io::stdout();
            execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
            let backend = CrosstermBackend::new(stdout);
            let mut terminal = Terminal::new(backend)?;

            let (rec, cancel) = timer::start_timer().await;

            kavana::run_app(&mut terminal, cancel, rec).await?;

            // restore terminal
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;
            terminal.show_cursor()?;

            Ok(())
        }
    }
}
