use std::{fs::File, io::{self, Read}, process::{exit, Command, Stdio}, thread::sleep, time::{Duration, Instant}};

use crossterm::{event::{poll, read, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers}, terminal::{disable_raw_mode, enable_raw_mode, size}};

fn game_width() -> usize {
    40
}

fn game_height() -> usize {
    35
}

const FPS_MUL: f32 = 18.0;
const OFFSET: f32 = 48.0;
const DEBUG: bool = true;

macro_rules! color {
    ($r:tt : $g:tt : $b:tt , $c:expr) => {
        format!("\x1b[38;2;{};{};{}m{}\x1b[0m", $r, $g, $b, vec![$c; game_width() / 2].join(""))
    };
    ($r:tt : $g:tt : $b:tt) => {
        color!($r:$g:$b, "@")
    };
}

#[derive(Clone, Debug, PartialEq)]
enum NoteKind {
    Red,
    Blue,
    Purple,
}

#[derive(Clone)]
struct Note {
    delay: f32,
    kind: NoteKind,
}

#[derive(Clone)]
enum ChartEventKind {
    BpmChange(i32),
    SpeedChange(f32),
}

#[derive(Clone)]
struct ChartEvent {
    delay: f32,
    kind: ChartEventKind,
}

struct Chart {
    notes: Vec<Note>,
    events: Vec<ChartEvent>,
    bpm: i32,
    speed: f32,
    target_speed: f32,
    score: i32,
    last_render: Vec<String>,
    last_width: u16,
}

enum GameResult {
    Win,
    Lose,
}

impl Chart {
    fn from_str(s: &str, bpm: i32) -> Option<Self> {
        let s = s.to_string()
            .replace(" ", "")
            .replace("\t", "")
            .replace("\n", "");
        let mut notes = vec![];
        let mut events = vec![];
        let offset = OFFSET * bpm as f32 / 60.0;

        let fields = s.split(",");
        let mut acc = 0.0;

        for field in fields {
            let parts = field.split(":").collect::<Vec<_>>();
            let kind = match parts[0] {
                "R" => NoteKind::Red,
                "B" => NoteKind::Blue,
                "P" => NoteKind::Purple,
                "SPEED" => {
                    let val = parts.get(1)?.parse::<f32>().ok()?;
                    let delay = parts.get(2)?.parse::<f32>().ok()?;

                    events.push(ChartEvent { delay: delay + acc - offset, kind: ChartEventKind::SpeedChange(val) });
                    acc += delay;
                    continue;
                },
                "BPM" => {
                    let val = parts.get(1)?.parse::<i32>().ok()?;
                    let delay = parts.get(2)?.parse::<f32>().ok()?;
                    events.push(ChartEvent { delay: delay + acc - offset, kind: ChartEventKind::BpmChange(val) });
                    acc += delay;
                    continue;
                },
                "" => break,
                _ => return None,
            };

            let delay = parts[1].parse::<f32>().ok()?;

            notes.push(Note { delay: delay + acc - offset, kind });
            acc += delay;
        }

        Some(Self::new(notes, events, bpm))
    }

    fn new(notes: Vec<Note>, events: Vec<ChartEvent>, bpm: i32) -> Self {
        Self {
            notes,
            events,
            bpm,
            speed: 1.0,
            target_speed: 8.0,
            score: 0,
            last_render: vec![],
            last_width: 0,
        }
    }

    fn render(&mut self, kind: Option<NoteKind>) -> Vec<String> {
        let (width, _) = size().unwrap();

        if self.last_width != width {
            print!("\x1b[2J\x1b[H");
        }

        self.last_width = width;

        let mut s1 = vec![color!(32:32:32, "."); game_height()];
        let mut s2 = vec![color!(32:32:32, "."); game_height()];

        let left = format!("|{}", vec!["-"; game_width() / 2 - 1].join(""));
        let right = format!("{}|", vec!["-"; game_width() / 2 - 1].join(""));
        s1[game_height() - 5] = format!("\x1b[38;2;{};{};{}m{}\x1b[0m", 96, 96, 96, left);
        s2[game_height() - 5] = format!("\x1b[38;2;{};{};{}m{}\x1b[0m", 96, 96, 96, right);
    
        s1[game_height() - 4] = format!("\x1b[38;2;{};{};{}m{}\x1b[0m", 96, 96, 96, left);
        s2[game_height() - 4] = format!("\x1b[38;2;{};{};{}m{}\x1b[0m", 96, 96, 96, right);

        match kind {
            None => {},
            Some(NoteKind::Red) => {
                s1[game_height() - 5] = format!("\x1b[38;2;{};{};{}m{}\x1b[0m", 255, 0, 0, left);
                s1[game_height() - 4] = format!("\x1b[38;2;{};{};{}m{}\x1b[0m", 255, 0, 0, left);
            },
            Some(NoteKind::Blue) => {
                s2[game_height() - 5] = format!("\x1b[38;2;{};{};{}m{}\x1b[0m", 0, 255, 255, right);
                s2[game_height() - 4] = format!("\x1b[38;2;{};{};{}m{}\x1b[0m", 0, 255, 255, right);
            },
            Some(NoteKind::Purple) => {
                let str = format!("\x1b[38;2;{};{};{}m{}\x1b[0m", 255, 0, 255, left);
                s1[game_height() - 5] = str.clone();
                s1[game_height() - 4] = str;
                let str = format!("\x1b[38;2;{};{};{}m{}\x1b[0m", 255, 0, 255, right);
                s2[game_height() - 5] = str.clone();
                s2[game_height() - 4] = str;
            },
        };

        for note in &self.notes {
            let fy = note.delay * self.speed as f32;
            //let y = fy * (note.delay.clamp(0.0, note.delay.abs()) + 4.0) * 0.333;
            let y = fy + 4.5;
            let y = game_height() as i32 - y as i32;
            if y >= 0 && y < game_height() as i32 {
                let fill_char = match fy % 1.0 {
                    0.0..=0.5 => "*",
                    _ => "^",
                };
                let filled = format!("<{}>", vec![fill_char; game_width()/2 - 2].join(""));
                let empty = vec![" "; game_width()/2 - 2].join("");
                match note.kind {
                    NoteKind::Red => {
                        let str = format!("\x1b[48;2;64;0;0m\x1b[38;2;{};{};{}m{}\x1b[0m", 255, 0, 0, filled);
                        s1[y as usize] = str.clone();
                        if y > 1 {
                            let str = format!("\x1b[48;2;64;0;0m \x1b[48;2;32;0;0m{}\x1b[48;2;64;0;0m \x1b[0m", empty);
                            s1[y as usize - 1] = str;
                            let str = format!("\x1b[48;2;32;0;0m \x1b[48;2;16;0;0m{}\x1b[48;2;32;0;0m \x1b[0m", empty);
                            s1[y as usize - 2] = str;
                        }
                    },
                    NoteKind::Blue => {
                        let str = format!("\x1b[48;2;0;64;64m\x1b[38;2;{};{};{}m{}\x1b[0m", 0, 255, 255, filled);
                        s2[y as usize] = str.clone();
                        if y > 1 {
                            let str = format!("\x1b[48;2;0;64;64m \x1b[48;2;0;32;32m{}\x1b[48;2;0;64;64m \x1b[0m", empty);
                            s2[y as usize - 1] = str;
                            let str = format!("\x1b[48;2;0;32;32m \x1b[48;2;0;16;16m{}\x1b[48;2;0;32;32m \x1b[0m", empty);
                            s2[y as usize - 2] = str;
                        }
                    },
                    NoteKind::Purple => {
                        let left = format!("<{}", vec![fill_char; game_width()/2 - 1].join(""));
                        let right = format!("{}>", vec![fill_char; game_width()/2 - 1].join(""));
                        let str = format!("\x1b[48;2;64;32;64m\x1b[38;2;{};{};{}m{}\x1b[0m", 255, 0, 255, left); 
                        s1[y as usize] = str.clone();
                        let str = format!("\x1b[48;2;64;32;64m\x1b[38;2;{};{};{}m{}\x1b[0m", 255, 0, 255, right); 
                        s2[y as usize] = str.clone();
                        if y > 1 {
                            let empty = vec![" "; game_width()/2 - 1].join("");
                            let str = format!("\x1b[48;2;64;32;64m \x1b[48;2;32;16;32m{}\x1b[0m", empty);
                            s1[y as usize - 1] = str.clone();
                            let str = format!("\x1b[48;2;32;16;32m{}\x1b[48;2;64;32;64m \x1b[0m", empty);
                            s2[y as usize - 1] = str;
                            let str = format!("\x1b[48;2;32;16;32m \x1b[48;2;16;8;16m{}\x1b[0m", empty);
                            s1[y as usize - 2] = str.clone();
                            let str = format!("\x1b[48;2;16;8;16m{}\x1b[48;2;32;16;32m \x1b[0m", empty);
                            s2[y as usize - 2] = str;
                        }
                    },
                };
            }
        }
        
        s1[0] = format!("{:width$}", format!("Score: {}", self.score), width = game_width()/2);
        s2[0] = format!("{}", vec![" "; game_width()/2].join(""));
        /*
        let bar = vec!["="; game_width() / 2].join("");
            
        s1[game_height() - 4] = format!("\x1b[38;2;{};{};{}m{}\x1b[0m", 96, 96, 96, bar);
        s2[game_height() - 4] = format!("\x1b[38;2;{};{};{}m{}\x1b[0m", 96, 96, 96, bar);

        match kind {
            None => {},
            Some(NoteKind::Red) => {
                s1[game_height() - 4] = format!("\x1b[38;2;{};{};{}m{}\x1b[0m", 255, 0, 0, bar);
            },
            Some(NoteKind::Blue) => {
                s2[game_height() - 4] = format!("\x1b[38;2;{};{};{}m{}\x1b[0m", 0, 255, 255, bar);
            },
            Some(NoteKind::Purple) => {
                let str = format!("\x1b[38;2;{};{};{}m{}\x1b[0m", 255, 0, 255, bar);
                s1[game_height() - 4] = str.clone();
                s2[game_height() - 4] = str;
            },
        };
        */

        s1.push(format!("\x1b[38;2;{};{};{}m{}", 64, 64, 64, vec!["*"; game_width() / 2].join("")));
        s2.push(format!("\x1b[38;2;{};{};{}m{}", 64, 64, 64, vec!["*"; game_width() / 2].join("")));

        let s = s1.into_iter()
            .zip(&s2)
            .map(|(l1, l2)| format!(
                "\x1b[38;2;48;48;48m{}\x1b[0m{}{}\x1b[38;2;48;48;48m{}\x1b[0m",
                vec!["["; width as usize / 2 - game_width() / 2].join(""),
                l1,
                l2,
                vec!["]"; width as usize / 2 - game_width() / 2].join(""),
            ))
            .collect::<Vec<_>>();

        s
    }

    fn update(&mut self) {
        self.notes = self.notes.clone().into_iter()
            .map(|note| Note { delay: note.delay - 1.0/FPS_MUL, kind: note.kind })
            .collect();

        self.events = self.events.clone().into_iter()
            .map(|event| ChartEvent { delay: event.delay - 1.0/FPS_MUL, kind: event.kind })
            .inspect(|event| {
                if event.delay <= 0.0 {
                    match event.kind {
                        ChartEventKind::SpeedChange(new) => self.target_speed = new,
                        ChartEventKind::BpmChange(new) => self.bpm = new,
                    }
                }
            })
            .collect();
    }

    fn should_press(&self, kind: NoteKind) -> bool {
        self.notes.iter()
            .find(|note| note.delay > -1.0 && note.delay <= 1.0 && note.kind == kind)
            .is_some()
    }

    fn consume_note(&mut self) {
        for (i, note) in self.notes.iter().enumerate() {
            if note.delay > -1.5 as f32 && note.delay < 1.5 as f32 {
                let d = note.delay.abs();
                self.score += (1.0 / (d + 0.5) * 500.0) as i32;
                self.notes.remove(i);
                return;
            }
        }
    }

    fn should_lose(&self) -> bool {
        self.notes.iter()
            .find(|note| note.delay < -1.0 as f32)
            .is_some()
    }

    fn play(&mut self) -> io::Result<GameResult> {
        let start_time = Instant::now();
        let mut expected = Duration::from_micros(0);
        let mut clocks = 0;
        let mut racing = false;

        let mut kind = None;
        let mut kind_timer = 0.0;
        
        loop {
            let delay = Duration::from_micros((60.0 / self.bpm as f32 * 1000000.0 / FPS_MUL) as u64);
            if self.should_lose() && !DEBUG {
                return Ok(GameResult::Lose);
            }
            
            if poll(Duration::from_millis(1))? {
                let event = read()?;

                if let Event::Key(event @ KeyEvent { code, kind: KeyEventKind::Press, .. }) = event {
                    match code {
                        KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => {
                            return Ok(GameResult::Lose);
                        },
                        KeyCode::Char('a') => {
                            kind = Some(NoteKind::Red);
                            kind_timer = self.bpm as f32 / 1000.0 * FPS_MUL;
                            if self.should_press(NoteKind::Red) {
                                self.consume_note();
                            }
                        },
                        KeyCode::Char('l') => {
                            kind = Some(NoteKind::Blue);
                            kind_timer = self.bpm as f32 / 1000.0 * FPS_MUL;
                            if self.should_press(NoteKind::Blue) {
                                self.consume_note();
                            }
                        },
                        KeyCode::Char(' ') => {
                            kind = Some(NoteKind::Purple);
                            kind_timer = self.bpm as f32 / 1000.0 * FPS_MUL;
                            if self.should_press(NoteKind::Purple) {
                                self.consume_note();
                            }
                        },
                        _ => {},
                    }
                }
            }
            
            if kind_timer < 0.0 {
                kind = None;
            }

            self.update();
            print!("\x1b[H");

            if !racing {
                let render = self.render(kind.clone());
                if self.last_render.len() == game_height() {
                    for (old, new) in self.last_render.iter().zip(&render) {
                        if old == new {
                            print!("\x1b[1B");
                        } else {
                            println!("{}", new);
                        }
                    }
                } else {
                    println!("{}", render.join("\r\n"));
                }
            }

            let s = start_time.elapsed();
            if s > expected {
                let elapsed = s - expected;

                if elapsed < delay {
                    racing = false;
                    sleep(delay - elapsed);
                } else {
                    racing = true;
                }
            } else {
                sleep(delay);
            }

            clocks += 1;
            kind_timer -= 1.0;

            self.speed += (self.target_speed - self.speed) * 0.1;
            
            expected = delay * clocks;
        }
    }
}

const SONG_PATH: &str = "twisted_garden.m4a";

fn run() -> io::Result<()> {
    println!("\x1b[2J");

    let mut mpv = Command::new("mpv")
        .arg(SONG_PATH)
        .arg(format!("--start={}", OFFSET))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let mut file = File::open("charts/twisted_garden.chart")?;

    let mut notes = String::new();
    file.read_to_string(&mut notes)?;

    let mut chart = Chart::from_str(&notes, 111)
        .ok_or(io::Error::new(io::ErrorKind::Other, "Failed to parse chart"))?;

    match chart.play()? {
        GameResult::Lose => {
            mpv.kill()?;
            Ok(())
        },
        _ => {
            Ok(())
        },
    }
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    print!("\x1b[?25l");
    sleep(Duration::from_millis(250));

    let result = run();
    disable_raw_mode()?;
    print!("\x1b[?25h");
    result
}
