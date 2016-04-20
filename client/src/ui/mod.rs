use std;
use time;
use time::{Tm, Duration};
use rustbox;
use rustbox::{ RustBox, Event, Key };

use model::{CoreModel, Buffer, BufKey};
use conn::ConnThread;

mod buffer;
mod entry;
mod bar;
mod alert;
mod util;

use self::entry::TextEntry;
use self::buffer::BufferView;
use self::bar::{StatusBar, MainBar, AlertBar};
use self::alert::{AlertList, Alert, AlertKind};
use self::util::RustBoxExt;


/// Stores the terminal UI's state.
pub struct TermUi {
    rb: RustBox,
    pub entry: TextEntry,
    pub model: CoreModel,
    pub alerts: AlertList,
    pub view: BufferView,
    key: BufKey,
    quit: bool,
    /// Status message shown at the bottom of the screen.
    status: Option<StatusMsg>,
}

struct StatusMsg {
    msg: String,
    time: Tm,
}

impl TermUi {
    pub fn new(status: Buffer, conn: ConnThread) -> Result<TermUi, rustbox::InitError> {
        let mut rb = try!(RustBox::init(rustbox::InitOptions {
            input_mode: rustbox::InputMode::Current,
            buffer_stderr: true,
        }));

        let model = CoreModel::new(status, conn);

        let key = (None, None);
        let buf = model.get(&key).unwrap().clone();

        Ok(TermUi {
            view: BufferView::new(buf, &mut rb),
            rb: rb,
            entry: TextEntry::new(),
            key: key,
            model: model,
            alerts: AlertList::new(),
            quit: false,
            status: None,
        })
    }

    /// The main function. Runs the client.
    pub fn main(&mut self) {
        // Status bars below the buffer.
        let mut upper_bars: Vec<Box<StatusBar>> = vec![
            Box::new(AlertBar) as Box<StatusBar>,
        ];
        // Status bars above the buffer.
        let mut lower_bars: Vec<Box<StatusBar>> = vec![
            Box::new(MainBar) as Box<StatusBar>,
        ];

        'main: loop {
            self.model.update();
            self.send_ping_alerts();
            self.alerts.update();

            if self.status.is_some() {
                if time::now() - self.status.as_ref().unwrap().time > Duration::seconds(5) {
                    self.status = None;
                }
            }

            for bar in upper_bars.iter_mut() { bar.update(self); }
            for bar in lower_bars.iter_mut() { bar.update(self); }
            self.render(&mut upper_bars, &mut lower_bars);

            if let Ok(e) = self.rb.peek_event(std::time::Duration::from_millis(200), false) {
                if let Event::KeyEvent(Key::Ctrl('c')) = e {
                    break 'main;
                }

                if !self.entry.handle(&e) {
                    self.handle_event(&e);
                } else {
                    if let Some(line) = self.entry.next_entry() {
                        self.handle_input(line);
                    }
                }
            }

            if self.quit { break 'main; }
        }
    }


    /// Sends alerts for any new pings or PMs.
    pub fn send_ping_alerts(&mut self) {
        for key in self.model.take_pings().into_iter() {
            let name = match key {
                (Some(ref net), Some(ref buf)) => format!("{}<{}>", buf, net),
                (None, Some(ref buf)) => format!("global buffer {}", buf),
                (Some(ref net), None) => format!("network buffer for {}", net),
                (None, None) => format!("status buffer"),
            };
            if !self.alerts.iter().any(|a| a.kind == AlertKind::Ping(key.clone())) && self.key != key {
                let a = Alert::ping(key.clone(), format!("Pinged in {}", name))
                    .action(move |ui| {
                        ui.switch_buf(key.clone())
                    });
                self.alerts.push(a);
            }
        }
    }


    /// Handles something typed into the text entry.
    pub fn handle_input(&mut self, line: String) {
        trace!("Typed: {}", &line);
        if line.starts_with("/") {
            let line = &line[1..];
            if let Some(spc) = line.find(' ') {
                self.handle_command(&line[..spc], &line[spc + 1..])
            } else {
                self.handle_command(line, &"");
            }
        } else {
            self.model.send_privmsg(&self.key, line);
        }
    }

    pub fn handle_command(&mut self, cmd: &str, args: &str) {
        match cmd {
            "quit" => { self.quit = true; },
            "s" | "switch" => {
                if args == "" {
                    self.switch_buf((None, None));
                } else {
                    if let Some((serv, buf)) = self.model.bufs.iter()
                        .map(|(k, _)| k.clone())
                        .find(|&(_, ref b)| b == &Some(args.to_owned()))
                    {
                        self.switch_buf((serv, buf));
                    } else {
                        self.status(format!("No buffer found matching {}", args));
                    }
                }
            },
            "j" | "join" => {
                let args = args.split(' ').collect::<Vec<_>>();
                if args.len() == 2 {
                    self.model.send_join(args[0].to_owned(), args[1].to_owned());
                } else {
                    self.status(format!("Usage: /join [network] [channel]"));
                }
            },
            "p" | "part" => {
                let args = args.split(' ').collect::<Vec<_>>();
                if args.len() >= 2 {
                    self.model.send_part(args[0].to_owned(), args[1].to_owned(), args[1..].join(" "));
                } else {
                    self.status(format!("Usage: /part [network] [channel] [message..]"));
                }
            },
            "a" => {
                if let Ok(id) = args.parse::<usize>() {
                    if id >= 1 && id-1 < self.alerts.count() {
                        trace!("Checking for alert action");
                        if let Some(mut act) = self.alerts.activate(id-1) {
                            debug!("Calling alert action");
                            act(self)
                        } else {
                            self.status(format!("Action has no alert"));
                        }
                    } else {
                        self.status(format!("No alert with ID: {}", id));
                    }
                } else {
                    self.status(format!("Not a valid alert ID: {}", args));
                }
            },
            _ => {
                self.status(format!("Unrecognized command: {}", cmd));
            },
        }
    }


    pub fn status(&mut self, msg: String) {
        self.status = Some(StatusMsg {
            msg: msg,
            time: time::now(),
        });
    }


    /// Switches to the buffer with the given key.
    pub fn switch_buf(&mut self, key: BufKey) {
        if let Some(ref mut buf) = self.model.get(&key) {
            info!("Switched buffer to {:?}", key);
            self.key = key;
            self.view = BufferView::new(buf.clone(), &mut self.rb);
            return;
        }
        self.status(format!("No such buffer: {:?}", key));
    }


    pub fn handle_event(&mut self, evt: &Event) {
        match *evt {
            Event::KeyEvent(key) => self.handle_key(&key),
            _ => {},
        }
    }

    pub fn handle_key(&mut self, key: &Key) {
        match *key {
            Key::PageUp => self.view.scroll_and_fetch(-10, &mut self.rb),
            Key::PageDown => self.view.scroll_by(10),
            _ => {},
        }
    }


    /// Renders the UI.
    ///
    /// `btop` and `bbot` are the status bars on the top and bottom
    /// of the buffer view.
    fn render(&mut self, btop: &mut Vec<Box<StatusBar>>, bbot: &mut Vec<Box<StatusBar>>) {
        self.rb.clear();

        self.entry.render(&mut self.rb);

        let top_height = btop.iter().fold(0, |acc, bar| acc + bar.height(&self));
        let bot_height = bbot.iter().fold(0, |acc, bar| acc + bar.height(&self));

        let y1 = top_height;
        let mut y2 = self.rb.height() - 1 - bot_height;

        if let Some(ref s) = self.status {
            use rustbox::Color::*;
            use rustbox::RB_NORMAL;
            self.rb.blank_line(y2-1, RB_NORMAL, White, Black);
            self.rb.print(0, y2-1, RB_NORMAL, White, Black, &s.msg);
            y2 -= 1;
        }

        self.view.render(&mut self.rb, y1, y2);

        for (y, bar) in btop.iter_mut().enumerate() {
            bar.render(y, self);
        }
        let h = self.rb.height();
        for (y, bar) in bbot.iter_mut().rev().enumerate() {
            bar.render(h - 2 - y, self);
        }



        self.rb.present();
    }
}
