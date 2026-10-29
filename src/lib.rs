use clap::{Parser, Subcommand};
use crossterm::event::{self, Event, KeyCode};
use std::io;
pub mod config;
pub mod timer;
pub mod ui;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[allow(unused_imports)]
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, BorderType, Borders, ListState},
    Frame, Terminal,
};

#[derive(Parser, Debug)]
#[command(
    name = "flowst",
    author = "Ben Toker <btoker.dev>",
    version = "1.0",
    about = r#"                                                            
                                                              
          ,--,                                        ___     
  .--., ,--.'|                                      ,--.'|_   
,--.'  \|  | :     ,---.           .---.            |  | :,'  
|  | /\/:  : '    '   ,'\         /. ./|  .--.--.   :  : ' :  
:  : :  |  ' |   /   /   |     .-'-. ' | /  /    '.;__,'  /   
:  | |-,'  | |  .   ; ,. :    /___/ \: ||  :  /`./|  |   |    
|  : :/||  | :  '   | |: : .-'.. '   ' .|  :  ;_  :__,'| :    
|  |  .''  : |__'   | .; :/___/ \:     ' \  \    `. '  : |__  
'  : '  |  | '.'|   :    |.   \  ' .\     `----.   \|  | '.'| 
|  | |  ;  :    ;\   \  /  \   \   ' \ | /  /`--'  /;  :    ; 
|  : \  |  ,   /  `----'    \   \  |--" '--'.     / |  ,   /  
|  |,'   ---`-'              \   \ |      `--'---'   ---`-'   
`--'                          '---"                           

    Basic Pomodoro (Flow) timer in Rust."#,
    long_about = None,
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Action,
}

#[derive(Subcommand, Debug)]
pub enum Action {
    ///Starts the timer. Uses -w and -r flags.
    Start(TimeArgs),
    Toggle,
    App,
    Reset,
}

#[derive(Parser, Debug)]
pub struct TimeArgs {
    /// Work time.
    #[arg(short, long, default_value = "25")]
    pub work: u32,
    /// Rest time.
    #[arg(short, long, default_value = "5")]
    pub rest: u32,
}

pub fn parse_args() -> Args {
    Args::parse()
}

enum Message {
    Quit,
    PauseOrUnpauseTimer,
    SelectedIndex(usize),
    Enter,
}

pub async fn get_sound(apikey: &str, sound: &str) {
    let search = "https://www.googleapis.com/youtube/v3/search";
    let params = [
        ("part", "snippet"),
        ("q", sound),
        ("type", "video"),
        ("key", apikey),
        ("maxResults", "1"),
    ];

    let client = reqwest::Client::new();
    let response: serde_json::Value = client
        .get(search)
        .query(&params)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let video_url = format!(
        "https://www.youtube.com/watch?v={}",
        response["items"][0]["id"]["videoId"].as_str().unwrap()
    );

    if webbrowser::open(&video_url).is_err() {
        eprintln!("Failed to open web browser.");
    }
}

pub async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut cancel: Arc<AtomicBool>,
    mut receiver: tokio::sync::mpsc::Receiver<String>,
) -> io::Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();

    let mut list_state = ListState::default();
    list_state.select(Some(0));

    std::thread::spawn(move || {
        let mut selected = 0;
        loop {
            if let Event::Key(key) = event::read().unwrap() {
                //only registers key presses (i.e. not key releases and repeats)
                //only affects Windows
                if key.kind != event::KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') => {
                        tx.send(Message::Quit).unwrap();
                    }
                    KeyCode::Char('p') => {
                        tx.send(Message::PauseOrUnpauseTimer).unwrap();
                    }

                    KeyCode::Up | KeyCode::Char('k') => {
                        if selected > 0 {
                            selected -= 1;
                        }
                        tx.send(Message::SelectedIndex(selected)).unwrap();
                    }

                    KeyCode::Down | KeyCode::Char('j') => {
                        if selected < 2 {
                            selected += 1;
                        }
                        tx.send(Message::SelectedIndex(selected)).unwrap();
                    }

                    KeyCode::Enter => {
                        tx.send(Message::Enter).unwrap();
                    }

                    _ => {}
                }
            }
        }
    });

    let mut timer_message = String::from("Please enter timer.");

    loop {
        let run_state = config::load_timer().unwrap().run_state;
        match receiver.try_recv() {
            Ok(msg) => {
                timer_message = String::from(msg);
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                // no messsages left to handle right now!
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                timer_message = String::from("Please enter timer.");
            }
        }

        match rx.try_recv() {
            Ok(Message::Quit) => {
                cancel.store(true, Ordering::Relaxed); // Set the cancel flag
                tokio::time::sleep(std::time::Duration::from_millis(500)).await; // Allow some time for tasks to recognize the cancelation
                break;
            }
            Ok(Message::PauseOrUnpauseTimer) => {
                let mut timer_info = config::load_timer().unwrap();
                if run_state {
                    // Pause timer
                    timer_info.pause_time = Some(chrono::Utc::now());
                    timer_info.run_state = false;
                    let _ = config::save_timer(&timer_info);
                    cancel.store(true, Ordering::Relaxed);
                } else {
                    // Unpause timer
                    timer_info.run_state = true;
                    let start_work_elapsed =
                        chrono::Utc::now().signed_duration_since(timer_info.start_work.unwrap());
                    if start_work_elapsed.num_seconds() <= timer_info.work_duration.num_seconds() {
                        timer_info.work_duration = timer_info.work_duration
                            - timer_info
                                .pause_time
                                .unwrap()
                                .signed_duration_since(timer_info.start_work.unwrap());
                        timer_info.start_work = Some(chrono::Utc::now());
                    } else {
                        timer_info.rest_duration = timer_info.rest_duration
                            - timer_info
                                .pause_time
                                .unwrap()
                                .signed_duration_since(timer_info.start_rest.unwrap());
                        timer_info.start_rest = Some(chrono::Utc::now());
                    };
                    let _ = config::save_timer(&timer_info);
                    cancel.store(false, Ordering::Relaxed);
                }
                receiver.close();
                (receiver, cancel) = timer::start_timer().await;
            }
            Ok(Message::SelectedIndex(index)) => list_state.select(Some(index)),
            Ok(Message::Enter) => {
                cancel.store(true, Ordering::Relaxed);
                receiver.close();
                let _ = match list_state.selected().unwrap() {
                    0 => config::save_cstm(25, 5),
                    1 => config::save_cstm(50, 10),
                    2 => config::save_cstm(60, 15),
                    _ => Ok({}),
                };
                (receiver, cancel) = timer::start_timer().await;
            }
            Err(_) => {}
        }

        terminal.draw(|f| {
            ui::ui(f);
            ui::tim_display(f, &timer_message);
            ui::config_display(f, &mut list_state);
        })?;

        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    }

    Ok(())
}
