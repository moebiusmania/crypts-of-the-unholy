use std::cell::{Cell, RefCell};

use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use crate::art;
use crate::cards::{Card, CardId, Rarity};
use crate::combat::Combat;
use crate::enemies::encounter;
use crate::fx::Fx;
use crate::model::{Class, Relic};
use crate::run::{RoomKind, RunState};
use crate::theme;
use crate::tutorial::{self, Goal, Tutorial};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Item(usize),
    Card(usize),
    Enemy(usize),
    EndTurn,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Player,
    Enemy { next: usize },
    Won,
    Lost,
}

pub struct CombatView {
    pub c: Combat,
    pub target: usize,
    pub phase: Phase,
    pub timer: f32,
    pub room: RoomKind,
    pub msg: Option<(String, f32)>,
    pub turn_t0: f32,
}

pub struct RewardView {
    pub gold: i32,
    pub cards: Vec<Card>,
    pub relic: Option<Relic>,
    pub healed: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    Continue,
    Rest,
    Pray,
    Open,
    BuyCard(usize),
    BuyRelic,
    Purge,
    Event(u8, u8),
}

#[derive(Clone)]
pub struct Opt {
    pub label: String,
    pub enabled: bool,
    pub card: Option<Card>,
    pub act: Act,
}

impl Opt {
    fn new(label: impl Into<String>, act: Act) -> Self {
        Opt {
            label: label.into(),
            enabled: true,
            card: None,
            act,
        }
    }
    fn when(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

#[derive(Clone)]
pub struct Shop {
    pub cards: Vec<(Card, i32, bool)>,
    pub relic: Option<(Relic, i32, bool)>,
    pub purge_cost: i32,
    pub purged: bool,
}

#[derive(Clone)]
pub struct ChoiceView {
    pub title: String,
    pub art: &'static [&'static str],
    pub art_color: Color,
    pub text: String,
    pub opts: Vec<Opt>,
    pub shop: Option<Shop>,
    pub t0: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Purpose {
    Upgrade,
    Remove,
}

pub struct DeckPick {
    pub purpose: Purpose,
    pub back: Box<ChoiceView>,
    pub advance_on_done: bool,
    pub cost: i32,
}

impl DeckPick {
    pub fn eligible(&self, deck: &[Card]) -> Vec<usize> {
        (0..deck.len())
            .filter(|&i| match self.purpose {
                Purpose::Upgrade => deck[i].upgradable(),
                Purpose::Remove => true,
            })
            .collect()
    }
}

pub struct EndStats {
    pub victory: bool,
    pub class: Class,
    pub floor: usize,
    pub kills: u32,
    pub secs: f64,
    pub deck: usize,
    pub cause: String,
}

pub enum Screen {
    Title,
    ClassSelect,
    Map,
    Combat(Box<CombatView>),
    Reward(RewardView),
    Choice(ChoiceView),
    DeckPick(DeckPick),
    End(EndStats),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Overlay {
    None,
    Pause,
    Deck,
    Help,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Input {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    Num(usize),
    Char(char),
    Click(Hit),
    Hover(Hit),
    ScrollUp,
    ScrollDown,
}

pub struct App {
    pub screen: Screen,
    pub overlay: Overlay,
    pub run: Option<RunState>,
    pub sel: usize,
    pub osel: usize,
    pub scroll: usize,
    pub fx: Fx,
    pub screen_t0: f32,
    pub hits: RefCell<Vec<(Rect, Hit)>>,
    pub ohits: RefCell<Vec<(Rect, Hit)>>,
    pub has_save: bool,
    pub quit: bool,
    pub deck_cols: Cell<usize>,
    /// Set while the guided tutorial encounter is running; nothing is saved meanwhile.
    pub tutorial: Option<Tutorial>,
}

pub const TITLE_ITEMS: [&str; 5] = ["New Descent", "Continue", "Tutorial", "Codex", "Quit"];
pub const PAUSE_ITEMS: [&str; 4] = ["Resume", "Codex", "Save & Quit to Title", "Abandon Run"];
pub const TUTORIAL_PAUSE_ITEMS: [&str; 3] = ["Resume", "Codex", "Leave Tutorial"];

impl App {
    pub fn new() -> Self {
        let has_save = RunState::has_save();
        App {
            screen: Screen::Title,
            overlay: Overlay::None,
            run: None,
            sel: if has_save { 1 } else { 0 },
            osel: 0,
            scroll: 0,
            fx: Fx::default(),
            screen_t0: 0.0,
            hits: RefCell::new(Vec::new()),
            ohits: RefCell::new(Vec::new()),
            has_save,
            quit: false,
            deck_cols: Cell::new(4),
            tutorial: None,
        }
    }

    pub fn pause_items(&self) -> &'static [&'static str] {
        if self.tutorial.is_some() {
            &TUTORIAL_PAUSE_ITEMS
        } else {
            &PAUSE_ITEMS
        }
    }

    pub fn hit(&self, r: Rect, h: Hit) {
        self.hits.borrow_mut().push((r, h));
    }

    pub fn ohit(&self, r: Rect, h: Hit) {
        self.ohits.borrow_mut().push((r, h));
    }

    pub fn screen_age(&self) -> f32 {
        self.fx.t - self.screen_t0
    }

    fn set_screen(&mut self, s: Screen) {
        self.screen = s;
        self.sel = 0;
        self.scroll = 0;
        self.screen_t0 = self.fx.t;
    }

    fn run(&mut self) -> &mut RunState {
        self.run.as_mut().expect("no active run")
    }

    fn save(&mut self) {
        if self.tutorial.is_some() {
            return;
        }
        if let Some(r) = &self.run
            && r.save().is_err()
        {
            self.fx.toast("Could not write save file!");
        }
    }

    // ------------------------------------------------------------------ update

    pub fn update(&mut self, dt: f32) {
        self.fx.update(dt);
        let in_run = !matches!(
            self.screen,
            Screen::Title | Screen::End(_) | Screen::ClassSelect
        );
        if in_run
            && self.overlay == Overlay::None
            && let Some(r) = self.run.as_mut()
        {
            r.elapsed += dt as f64;
        }
        if self.overlay != Overlay::None {
            return;
        }
        let Screen::Combat(cv) = &mut self.screen else {
            return;
        };
        let class = cv.c.class;
        for ev in cv.c.events.drain(..) {
            if let crate::combat::Ev::Banner(s) = &ev
                && s.contains("MORVATH")
            {
                self.fx.banner(s.clone(), theme::BLOOD_LIGHT);
                self.fx.shake = self.fx.t;
                continue;
            }
            self.fx.ingest(ev, class);
        }
        if let Some((_, t)) = &cv.msg
            && self.fx.t - t > 2.0
        {
            cv.msg = None;
        }
        if (cv.target >= cv.c.enemies.len() || cv.c.enemies[cv.target].dead)
            && let Some(t) = cv.c.first_alive()
        {
            cv.target = t;
        }
        cv.timer -= dt;
        match cv.phase {
            Phase::Player => {}
            Phase::Enemy { next } => {
                if cv.timer > 0.0 {
                    return;
                }
                if cv.c.lost() {
                    cv.phase = Phase::Lost;
                    cv.timer = 1.8;
                } else if next < cv.c.enemies.len() {
                    if cv.c.enemies[next].dead {
                        cv.timer = 0.0;
                    } else {
                        cv.c.enemy_act(next);
                        cv.timer = 0.7;
                    }
                    cv.phase = Phase::Enemy { next: next + 1 };
                    if cv.c.won() {
                        cv.phase = Phase::Won;
                        cv.timer = 1.4;
                    }
                } else {
                    cv.c.start_player_turn();
                    cv.turn_t0 = self.fx.t;
                    cv.phase = if cv.c.lost() {
                        Phase::Lost
                    } else {
                        Phase::Player
                    };
                    cv.timer = 1.8;
                    self.sel = 0;
                    if let Some(tu) = &mut self.tutorial
                        && tu.goal() == Goal::Watch
                    {
                        tu.next(self.fx.t);
                    }
                }
            }
            Phase::Won => {
                if let Some(tu) = &mut self.tutorial {
                    if tu.goal() != Goal::Done {
                        tu.step = tutorial::STEPS.len() - 1;
                        tu.t0 = self.fx.t;
                    }
                } else if cv.timer <= 0.0 {
                    self.finish_combat();
                }
            }
            Phase::Lost => {
                if cv.timer <= 0.0 {
                    if self.tutorial.is_some() {
                        self.leave_tutorial();
                        self.fx
                            .toast("Even the best pilgrims stumble. Try again from the title.");
                        return;
                    }
                    let cause =
                        cv.c.enemies
                            .iter()
                            .find(|e| !e.dead)
                            .map(|e| format!("Slain by {}", e.kind.name()))
                            .unwrap_or_else(|| "Bled dry".into());
                    self.game_over(false, cause);
                }
            }
        }
    }

    // ------------------------------------------------------------------ input

    pub fn on_event(&mut self, ev: Event) {
        let input = match ev {
            Event::Key(k) => self.map_key(k),
            Event::Mouse(m) => self.map_mouse(m),
            _ => None,
        };
        if let Some(i) = input {
            self.handle(i);
        }
    }

    fn map_key(&mut self, k: KeyEvent) -> Option<Input> {
        if k.kind == KeyEventKind::Release {
            return None;
        }
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            self.quit = true;
            return None;
        }
        Some(match k.code {
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('w') => Input::Up,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('s') => Input::Down,
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('a') => Input::Left,
            KeyCode::Right | KeyCode::Char('l') => Input::Right,
            KeyCode::Tab => Input::Down,
            KeyCode::BackTab => Input::Up,
            KeyCode::Enter | KeyCode::Char(' ') => Input::Confirm,
            KeyCode::Esc => Input::Back,
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let d = c.to_digit(10).unwrap() as usize;
                Input::Num(if d == 0 { 9 } else { d - 1 })
            }
            KeyCode::Char(c) => Input::Char(c.to_ascii_lowercase()),
            _ => return None,
        })
    }

    fn hit_at(&self, x: u16, y: u16) -> Option<Hit> {
        let p = Position { x, y };
        let list = if self.overlay != Overlay::None {
            &self.ohits
        } else {
            &self.hits
        };
        list.borrow()
            .iter()
            .rev()
            .find(|(r, _)| r.contains(p))
            .map(|(_, h)| *h)
    }

    fn map_mouse(&mut self, m: MouseEvent) -> Option<Input> {
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.hit_at(m.column, m.row).map(Input::Click)
            }
            MouseEventKind::Down(MouseButton::Right) => Some(Input::Back),
            MouseEventKind::Moved => self.hit_at(m.column, m.row).map(Input::Hover),
            MouseEventKind::ScrollUp => Some(Input::ScrollUp),
            MouseEventKind::ScrollDown => Some(Input::ScrollDown),
            _ => None,
        }
    }

    fn handle(&mut self, i: Input) {
        if self.overlay != Overlay::None {
            self.handle_overlay(i);
            return;
        }
        match i {
            Input::Char('?') => {
                self.open_overlay(Overlay::Help);
                return;
            }
            Input::Char('d') if self.run.is_some() && !matches!(self.screen, Screen::End(_)) => {
                self.open_overlay(Overlay::Deck);
                return;
            }
            _ => {}
        }
        match &self.screen {
            Screen::Title => self.input_title(i),
            Screen::ClassSelect => self.input_class(i),
            Screen::Map => self.input_map(i),
            Screen::Combat(_) => self.input_combat(i),
            Screen::Reward(_) => self.input_reward(i),
            Screen::Choice(_) => self.input_choice(i),
            Screen::DeckPick(_) => self.input_deckpick(i),
            Screen::End(_) => {
                if matches!(i, Input::Confirm | Input::Back | Input::Click(_)) {
                    self.has_save = RunState::has_save();
                    self.set_screen(Screen::Title);
                }
            }
        }
    }

    fn open_overlay(&mut self, o: Overlay) {
        self.overlay = o;
        self.osel = 0;
        self.scroll = 0;
    }

    /// Generic list navigation; returns the activated index if any.
    fn nav(sel: &mut usize, n: usize, i: Input, horizontal: bool) -> Option<usize> {
        if n == 0 {
            return None;
        }
        let (prev, next) = if horizontal {
            (Input::Left, Input::Right)
        } else {
            (Input::Up, Input::Down)
        };
        match i {
            x if x == prev => *sel = (*sel + n - 1) % n,
            x if x == next => *sel = (*sel + 1) % n,
            Input::Confirm => return Some((*sel).min(n - 1)),
            Input::Num(k) if k < n => {
                *sel = k;
                return Some(k);
            }
            Input::Hover(Hit::Item(k)) if k < n => *sel = k,
            Input::Click(Hit::Item(k)) if k < n => {
                *sel = k;
                return Some(k);
            }
            _ => {}
        }
        None
    }

    fn handle_overlay(&mut self, i: Input) {
        match self.overlay {
            Overlay::Pause => {
                if i == Input::Back {
                    self.overlay = Overlay::None;
                    return;
                }
                let n = self.pause_items().len();
                let Some(k) = Self::nav(&mut self.osel, n, i, false) else {
                    return;
                };
                match k {
                    0 => self.overlay = Overlay::None,
                    1 => self.open_overlay(Overlay::Help),
                    _ if self.tutorial.is_some() => self.leave_tutorial(),
                    2 => {
                        if matches!(self.screen, Screen::Map) {
                            self.save();
                        }
                        self.run = None;
                        self.overlay = Overlay::None;
                        self.has_save = RunState::has_save();
                        self.set_screen(Screen::Title);
                        self.sel = 1;
                    }
                    _ => {
                        RunState::delete_save();
                        self.run = None;
                        self.overlay = Overlay::None;
                        self.has_save = false;
                        self.set_screen(Screen::Title);
                    }
                }
            }
            Overlay::Deck | Overlay::Help => match i {
                Input::Up | Input::ScrollUp => self.scroll = self.scroll.saturating_sub(1),
                Input::Down | Input::ScrollDown => self.scroll += 1,
                Input::Back
                | Input::Confirm
                | Input::Char('d')
                | Input::Char('?')
                | Input::Char('q') => {
                    self.overlay = Overlay::None;
                }
                Input::Click(_) => self.overlay = Overlay::None,
                _ => {}
            },
            Overlay::None => {}
        }
    }

    fn input_title(&mut self, i: Input) {
        if i == Input::Char('q') || i == Input::Back {
            self.quit = true;
            return;
        }
        let act = Self::nav(&mut self.sel, TITLE_ITEMS.len(), i, false);
        if self.sel == 1 && !self.has_save {
            // Skip the disabled Continue entry when moving with keys.
            match i {
                Input::Down => self.sel = 2,
                Input::Up => self.sel = 0,
                _ => {}
            }
        }
        match act {
            Some(0) => self.set_screen(Screen::ClassSelect),
            Some(1) if self.has_save => self.continue_run(),
            Some(2) => self.start_tutorial(),
            Some(3) => self.open_overlay(Overlay::Help),
            Some(4) => self.quit = true,
            _ => {}
        }
    }

    fn input_class(&mut self, i: Input) {
        if i == Input::Back {
            self.set_screen(Screen::Title);
            return;
        }
        if let Some(k) = Self::nav(&mut self.sel, 3, i, true) {
            self.new_run(Class::ALL[k]);
        }
    }

    fn input_map(&mut self, i: Input) {
        if i == Input::Back {
            self.open_overlay(Overlay::Pause);
            return;
        }
        let r = self.run.as_ref().unwrap();
        let n = r.map[r.floor].len();
        if let Some(k) = Self::nav(&mut self.sel, n, i, true) {
            self.choose_door(k);
        }
    }

    fn input_combat(&mut self, i: Input) {
        if i == Input::Back {
            self.open_overlay(Overlay::Pause);
            return;
        }
        if let Some(tu) = &mut self.tutorial
            && matches!(i, Input::Confirm | Input::Click(_))
        {
            match tu.goal() {
                Goal::Read => {
                    tu.next(self.fx.t);
                    return;
                }
                Goal::Done => {
                    self.leave_tutorial();
                    return;
                }
                _ => {}
            }
        }
        let Screen::Combat(cv) = &mut self.screen else {
            return;
        };
        if cv.phase != Phase::Player {
            return;
        }
        let n = cv.c.hand.len();
        let alive: Vec<usize> = (0..cv.c.enemies.len())
            .filter(|&e| !cv.c.enemies[e].dead)
            .collect();
        let cycle_target = |cv: &mut CombatView, dir: i32| {
            if alive.is_empty() {
                return;
            }
            let pos = alive.iter().position(|&e| e == cv.target).unwrap_or(0) as i32;
            let len = alive.len() as i32;
            cv.target = alive[((pos + dir + len) % len) as usize];
        };
        let mut play: Option<usize> = None;
        match i {
            Input::Left if n > 0 => self.sel = (self.sel + n - 1) % n,
            Input::Right if n > 0 => self.sel = (self.sel + 1) % n,
            Input::Up => cycle_target(cv, -1),
            Input::Down => cycle_target(cv, 1),
            Input::Confirm => play = Some(self.sel),
            Input::Num(k) if k < n => {
                self.sel = k;
                play = Some(k);
            }
            Input::Char('e') | Input::Click(Hit::EndTurn) => {
                if let Some(tu) = &mut self.tutorial {
                    if !tu.allows_end_turn() {
                        cv.msg = Some((
                            "Not yet — follow the guide above your hand.".into(),
                            self.fx.t,
                        ));
                        return;
                    }
                    if tu.goal() == Goal::EndTurn {
                        tu.next(self.fx.t);
                    }
                }
                cv.c.end_player_turn();
                if cv.c.won() {
                    cv.phase = Phase::Won;
                    cv.timer = 1.4;
                } else {
                    cv.phase = Phase::Enemy { next: 0 };
                    cv.timer = 0.6;
                    self.fx.banner("ENEMY TURN", theme::BLOOD_LIGHT);
                }
            }
            Input::Hover(Hit::Card(k)) => self.sel = k,
            Input::Click(Hit::Card(k)) => {
                self.sel = k;
                play = Some(k);
            }
            Input::Hover(Hit::Enemy(_)) => {}
            Input::Click(Hit::Enemy(e)) if !cv.c.enemies[e].dead => {
                cv.target = e;
            }
            _ => {}
        }
        if let Some(k) = play {
            if k >= cv.c.hand.len() {
                return;
            }
            if let Some(why) = cv.c.unplayable_reason(k) {
                cv.msg = Some((why.to_string(), self.fx.t));
                return;
            }
            let card = cv.c.hand[k];
            if let Some(why) = self.tutorial.as_ref().and_then(|tu| tu.blocks_card(card)) {
                if !why.is_empty() {
                    cv.msg = Some((why.to_string(), self.fx.t));
                }
                return;
            }
            cv.c.play(k, Some(cv.target));
            if let Some(tu) = &mut self.tutorial {
                tu.on_play(card, self.fx.t);
            }
            if cv.c.won() {
                cv.phase = Phase::Won;
                cv.timer = 1.4;
            } else if cv.c.lost() {
                cv.phase = Phase::Lost;
                cv.timer = 1.8;
            }
            let n = cv.c.hand.len();
            if self.sel >= n {
                self.sel = n.saturating_sub(1);
            }
        }
    }

    fn input_reward(&mut self, i: Input) {
        let Screen::Reward(rv) = &self.screen else {
            return;
        };
        let n = rv.cards.len() + 1;
        let cards = rv.cards.clone();
        let i = match i {
            Input::Hover(Hit::Card(k)) => Input::Hover(Hit::Item(k)),
            Input::Click(Hit::Card(k)) => Input::Click(Hit::Item(k)),
            Input::Back => Input::Click(Hit::Item(n - 1)),
            x => x,
        };
        if let Some(k) = Self::nav(&mut self.sel, n, i, true) {
            if let Some(c) = cards.get(k) {
                self.run().deck.push(*c);
                self.fx.toast(format!("{} added to your deck.", c.name()));
            }
            self.save();
            self.set_screen(Screen::Map);
        }
    }

    fn input_choice(&mut self, i: Input) {
        let Screen::Choice(cv) = &self.screen else {
            return;
        };
        let n = cv.opts.len();
        if let Some(k) = Self::nav(&mut self.sel, n, i, false) {
            let Screen::Choice(cv) = &self.screen else {
                return;
            };
            let opt = cv.opts[k].clone();
            if opt.enabled {
                self.choice_act(opt.act);
            }
        }
    }

    fn input_deckpick(&mut self, i: Input) {
        let Screen::DeckPick(dp) = &self.screen else {
            return;
        };
        if i == Input::Back {
            let Screen::DeckPick(dp) = std::mem::replace(&mut self.screen, Screen::Title) else {
                return;
            };
            self.set_screen(Screen::Choice(*dp.back));
            return;
        }
        let deck = &self.run.as_ref().unwrap().deck;
        let elig = dp.eligible(deck);
        let n = elig.len();
        let i = match i {
            Input::Hover(Hit::Card(k)) => Input::Hover(Hit::Item(k)),
            Input::Click(Hit::Card(k)) => Input::Click(Hit::Item(k)),
            Input::ScrollUp => Input::Up,
            Input::ScrollDown => Input::Down,
            x => x,
        };
        let cols = self.deck_cols.get().max(1);
        let mut act = None;
        match i {
            Input::Up => self.sel = self.sel.saturating_sub(cols),
            Input::Down => self.sel = (self.sel + cols).min(n.saturating_sub(1)),
            _ => act = Self::nav(&mut self.sel, n, i, true),
        }
        let Some(k) = act else { return };
        let idx = elig[k];
        let Screen::DeckPick(dp) = std::mem::replace(&mut self.screen, Screen::Title) else {
            return;
        };
        let run = self.run.as_mut().unwrap();
        let name = run.deck[idx].name();
        run.gold -= dp.cost;
        let msg = match dp.purpose {
            Purpose::Upgrade => {
                run.deck[idx].up = true;
                format!("{name} glows with unholy light. Upgraded!")
            }
            Purpose::Remove => {
                run.deck.remove(idx);
                format!("{name} is buried and forgotten.")
            }
        };
        self.fx.toast(msg.clone());
        if dp.advance_on_done {
            self.advance();
            self.set_screen(Screen::Map);
        } else {
            let mut back = *dp.back;
            if let Some(shop) = back.shop.as_mut() {
                shop.purged = true;
            }
            back.text = msg;
            back.t0 = self.fx.t;
            let run = self.run.as_ref().unwrap();
            if let Some(shop) = &back.shop {
                back.opts = shop_opts(shop, run);
            }
            self.set_screen(Screen::Choice(back));
        }
    }

    // ------------------------------------------------------------------ flow

    fn new_run(&mut self, class: Class) {
        RunState::delete_save();
        self.run = Some(RunState::new(class));
        self.save();
        self.has_save = true;
        self.fx.toast("The crypt doors grind shut behind you...");
        self.set_screen(Screen::Map);
    }

    fn start_tutorial(&mut self) {
        self.fx.clear_combat();
        let run = tutorial::run();
        let c = tutorial::combat(&run);
        self.run = Some(run);
        self.tutorial = Some(Tutorial::new(self.fx.t));
        self.fx.banner("TUTORIAL", theme::GOLD);
        let t = self.fx.t;
        self.set_screen(Screen::Combat(Box::new(CombatView {
            c,
            target: 0,
            phase: Phase::Player,
            timer: 1.0,
            room: RoomKind::Combat,
            msg: None,
            turn_t0: t,
        })));
    }

    fn leave_tutorial(&mut self) {
        self.tutorial = None;
        self.run = None;
        self.overlay = Overlay::None;
        self.fx.clear_combat();
        self.has_save = RunState::has_save();
        self.set_screen(Screen::Title);
        self.sel = 2;
    }

    fn continue_run(&mut self) {
        match RunState::load() {
            Some(r) => {
                let pending = r.pending.is_some();
                self.run = Some(r);
                if pending {
                    self.enter_room();
                } else {
                    self.set_screen(Screen::Map);
                }
                self.fx.toast("Your descent continues.");
            }
            None => {
                self.fx.toast("The save file is unreadable.");
                self.has_save = false;
            }
        }
    }

    fn choose_door(&mut self, k: usize) {
        self.run().pending = Some(k);
        self.save();
        self.enter_room();
    }

    /// Moves to the next floor and saves. Does not change screen.
    fn advance(&mut self) {
        let run = self.run();
        let door = run.pending.take().unwrap_or(0);
        run.path.push(door);
        run.floor += 1;
        self.save();
    }

    fn enter_room(&mut self) {
        self.fx.clear_combat();
        let t = self.fx.t;
        let run = self.run.as_mut().unwrap();
        let Some(room) = run.current_room() else {
            self.set_screen(Screen::Map);
            return;
        };
        let screen = match room {
            RoomKind::Combat | RoomKind::Elite | RoomKind::Boss => {
                let kinds = encounter(
                    run.floor,
                    room == RoomKind::Elite,
                    room == RoomKind::Boss,
                    &mut run.rng,
                );
                let seed = run.rng.next_u64();
                let mut c = Combat::new(run, &kinds, seed);
                c.start();
                let target = c.first_alive().unwrap_or(0);
                let title = match room {
                    RoomKind::Boss => "THE UNHOLY ABBOT",
                    RoomKind::Elite => "A GREATER HORROR",
                    _ => "THE DEAD STIR",
                };
                self.fx.banner(title, theme::BLOOD_LIGHT);
                Screen::Combat(Box::new(CombatView {
                    c,
                    target,
                    phase: Phase::Player,
                    timer: 1.0,
                    room,
                    msg: None,
                    turn_t0: t,
                }))
            }
            RoomKind::Shrine => {
                let heal = run.max_hp * 3 / 10;
                Screen::Choice(ChoiceView {
                    title: "The Ruined Chapel".into(),
                    art: art::CHAPEL,
                    art_color: theme::HEAL,
                    text: "Moonlight falls through a shattered rose window onto a cracked altar. For a \
                           moment the whispers fall silent, and you can almost remember what peace felt like."
                        .into(),
                    opts: vec![
                        Opt::new(format!("Rest — heal {heal} HP"), Act::Rest),
                        Opt::new("Pray — upgrade a card", Act::Pray),
                        Opt::new("Move on", Act::Continue),
                    ],
                    shop: None,
                    t0: t,
                })
            }
            RoomKind::Treasure => Screen::Choice(ChoiceView {
                title: "The Sealed Sarcophagus".into(),
                art: art::SARCOPHAGUS,
                art_color: theme::GOLD,
                text: "A sarcophagus of black marble, its lid carved with a weeping saint. Fresh \
                       scratches mark the stone... from the inside."
                    .into(),
                opts: vec![
                    Opt::new("Pry open the lid", Act::Open),
                    Opt::new("Leave it be", Act::Continue),
                ],
                shop: None,
                t0: t,
            }),
            RoomKind::Merchant => {
                let cards = run.reward_cards(3, 0.08);
                let mut items: Vec<(Card, i32, bool)> = cards
                    .into_iter()
                    .map(|c| {
                        let base = c.def().rarity.price() + if c.up { 20 } else { 0 };
                        let price = base + run.rng.range(-5, 8);
                        (c, price, false)
                    })
                    .collect();
                items.sort_by_key(|i| i.1);
                let relic = run
                    .random_relic()
                    .map(|r| (r, 110 + run.rng.range(0, 25), false));
                let shop = Shop {
                    cards: items,
                    relic,
                    purge_cost: 60,
                    purged: false,
                };
                let opts = shop_opts(&shop, run);
                Screen::Choice(ChoiceView {
                    title: "The Hollow Merchant".into(),
                    art: art::MERCHANT,
                    art_color: theme::STEEL,
                    text: "A gaunt figure in a moth-eaten cassock spreads his wares across a coffin lid. \
                           \"Gold for power, pilgrim. Everything else costs more.\""
                        .into(),
                    opts,
                    shop: Some(shop),
                    t0: t,
                })
            }
            RoomKind::Event => Screen::Choice(event_view(run.rng.below(5) as u8, run, t)),
        };
        self.set_screen(screen);
    }

    fn finish_combat(&mut self) {
        let Screen::Combat(cv) = std::mem::replace(&mut self.screen, Screen::Title) else {
            return;
        };
        let run = self.run.as_mut().unwrap();
        run.hp = cv.c.p.hp.max(1);
        run.kills += cv.c.kills;
        if cv.room == RoomKind::Boss {
            self.game_over(true, "Morvath is no more".into());
            return;
        }
        let mut healed = 0;
        if run.relics.contains(&Relic::CrackedChalice) {
            let before = run.hp;
            run.heal(5);
            healed = run.hp - before;
        }
        let elite = cv.room == RoomKind::Elite;
        let mut gold = if elite {
            run.rng.range(28, 40)
        } else {
            run.rng.range(12, 22)
        };
        if run.relics.contains(&Relic::OssuaryKey) {
            gold = gold * 3 / 2;
        }
        run.gold += gold;
        let relic = if elite { run.random_relic() } else { None };
        if let Some(r) = relic {
            run.add_relic(r);
        }
        let cards = run.reward_cards(3, if elite { 0.1 } else { 0.0 });
        self.advance();
        self.fx.clear_combat();
        self.set_screen(Screen::Reward(RewardView {
            gold,
            cards,
            relic,
            healed,
        }));
    }

    fn game_over(&mut self, victory: bool, cause: String) {
        RunState::delete_save();
        self.has_save = false;
        let run = self.run.take().unwrap();
        self.fx.clear_combat();
        self.set_screen(Screen::End(EndStats {
            victory,
            class: run.class,
            floor: run.floor + 1,
            kills: run.kills,
            secs: run.elapsed,
            deck: run.deck.len(),
            cause,
        }));
    }

    fn resolve_choice(&mut self, text: String) {
        let t = self.fx.t;
        if let Screen::Choice(cv) = &mut self.screen {
            cv.text = text;
            cv.t0 = t;
            cv.opts = vec![Opt::new("Continue deeper", Act::Continue)];
            self.sel = 0;
        }
    }

    fn choice_act(&mut self, act: Act) {
        let run = self.run.as_mut().unwrap();
        match act {
            Act::Continue => {
                self.advance();
                self.set_screen(Screen::Map);
            }
            Act::Rest => {
                let heal = run.max_hp * 3 / 10;
                run.heal(heal);
                self.resolve_choice(format!(
                    "You kneel among the broken pews and sleep without dreams. (+{heal} HP)"
                ));
            }
            Act::Pray => self.start_pick(Purpose::Upgrade, true, 0),
            Act::Open => {
                let gold = run.rng.range(20, 41);
                run.gold += gold;
                let relic = run.random_relic();
                let bitten = run.rng.chance(0.25);
                if bitten {
                    run.hp = (run.hp - 5).max(1);
                }
                let mut text = String::from("The lid slides away with a groan. ");
                if bitten {
                    text.push_str("A withered hand lashes out and rakes your arm (-5 HP) before crumbling to dust. ");
                }
                match relic {
                    Some(r) => {
                        run.add_relic(r);
                        text.push_str(&format!(
                            "Amid the bones you find {gold} gold and the {}: {}",
                            r.name(),
                            r.desc()
                        ));
                    }
                    None => text.push_str(&format!("Amid the bones you find {gold} gold.")),
                }
                self.resolve_choice(text);
            }
            Act::BuyCard(k) => {
                let Screen::Choice(cv) = &mut self.screen else {
                    return;
                };
                let shop = cv.shop.as_mut().unwrap();
                let (card, price, _) = shop.cards[k];
                run.gold -= price;
                run.deck.push(card);
                shop.cards[k].2 = true;
                cv.opts = shop_opts(shop, run);
                cv.text = format!(
                    "\"{} — a fine choice. It will serve you... for a while.\"",
                    card.name()
                );
                cv.t0 = self.fx.t;
            }
            Act::BuyRelic => {
                let Screen::Choice(cv) = &mut self.screen else {
                    return;
                };
                let shop = cv.shop.as_mut().unwrap();
                let Some((relic, price, _)) = shop.relic else {
                    return;
                };
                run.gold -= price;
                run.add_relic(relic);
                shop.relic = Some((relic, price, true));
                cv.opts = shop_opts(shop, run);
                cv.text = format!(
                    "\"The {}. Its last owner won't be needing it.\"",
                    relic.name()
                );
                cv.t0 = self.fx.t;
            }
            Act::Purge => {
                let cost = match &self.screen {
                    Screen::Choice(ChoiceView { shop: Some(s), .. }) => s.purge_cost,
                    _ => 0,
                };
                self.start_pick(Purpose::Remove, false, cost);
            }
            Act::Event(id, opt) => self.event_act(id, opt),
        }
    }

    fn start_pick(&mut self, purpose: Purpose, advance_on_done: bool, cost: i32) {
        let Screen::Choice(cv) = std::mem::replace(&mut self.screen, Screen::Title) else {
            return;
        };
        let dp = DeckPick {
            purpose,
            back: Box::new(cv),
            advance_on_done,
            cost,
        };
        if dp.eligible(&self.run.as_ref().unwrap().deck).is_empty() {
            self.fx.toast("You have no cards that can be chosen.");
            self.set_screen(Screen::Choice(*dp.back));
            return;
        }
        self.set_screen(Screen::DeckPick(dp));
    }

    fn event_act(&mut self, id: u8, opt: u8) {
        let run = self.run.as_mut().unwrap();
        let text: String = match (id, opt) {
            (0, 0) => {
                run.hp -= 8;
                match run.random_relic() {
                    Some(r) => {
                        run.add_relic(r);
                        format!(
                            "Your blood hisses as it touches the stone. The altar drinks deeply, and \
                             from the gore rises the {}: {}",
                            r.name(),
                            r.desc()
                        )
                    }
                    None => "The altar drinks your blood... and gives nothing back.".into(),
                }
            }
            (1, 0) => {
                let pool = crate::cards::class_pool(run.class);
                let rares: Vec<CardId> = pool
                    .into_iter()
                    .filter(|&c| Card::new(c).def().rarity == Rarity::Rare)
                    .collect();
                let card = Card::new(*run.rng.pick(&rares));
                run.deck.push(card);
                run.deck.push(Card::new(CardId::Wound));
                format!(
                    "The skull whispers a secret that should never have been spoken. You learn {}, \
                     but the knowledge leaves a Wound on your soul.",
                    card.name()
                )
            }
            (1, _) => {
                run.gold += 25;
                "You grind the skull beneath your heel. Its gold teeth scatter across the floor. (+25 gold)".into()
            }
            (2, 0) => {
                run.heal(20);
                "The water is cold as a grave and tastes of iron. Your wounds knit closed. (+20 HP)"
                    .into()
            }
            (2, 1) => {
                run.hp = (run.hp - 6).max(1);
                let mut idx: Vec<usize> = (0..run.deck.len())
                    .filter(|&i| run.deck[i].upgradable())
                    .collect();
                run.rng.shuffle(&mut idx);
                let names: Vec<String> = idx
                    .iter()
                    .take(2)
                    .map(|&i| {
                        run.deck[i].up = true;
                        run.deck[i].name()
                    })
                    .collect();
                if names.is_empty() {
                    "The black water burns your skin, but nothing within you changes. (-6 HP)"
                        .into()
                } else {
                    format!(
                        "The black water burns like acid. You emerge changed. (-6 HP, upgraded: {})",
                        names.join(", ")
                    )
                }
            }
            (3, 0) => {
                run.gold += 45;
                run.hp = (run.hp - 5).max(1);
                "As you cut the purse from his belt, the pilgrim's dead hand claws your face. (+45 gold, -5 HP)".into()
            }
            (3, 1) => {
                run.max_hp += 5;
                run.hp += 5;
                "You cut him down and whisper the old rites. Something unseen thanks you. (+5 Max HP)".into()
            }
            (4, 0) => {
                self.start_pick(Purpose::Remove, true, 35);
                return;
            }
            (4, 1) => {
                let before = run.deck.len();
                run.deck.retain(|c| c.id != CardId::Wound);
                format!(
                    "The gravedigger buries your wounds in unmarked earth. ({} Wound(s) removed)",
                    before - run.deck.len()
                )
            }
            _ => "You leave this place behind.".into(),
        };
        self.resolve_choice(text);
    }
}

fn shop_opts(shop: &Shop, run: &RunState) -> Vec<Opt> {
    let mut opts = Vec::new();
    for (k, (card, price, sold)) in shop.cards.iter().enumerate() {
        let label = if *sold {
            format!("{} — SOLD", card.name())
        } else {
            format!("{} — {price}g", card.name())
        };
        let mut o = Opt::new(label, Act::BuyCard(k)).when(!sold && run.gold >= *price);
        o.card = Some(*card);
        opts.push(o);
    }
    if let Some((relic, price, sold)) = shop.relic {
        let label = if sold {
            format!("Relic: {} — SOLD", relic.name())
        } else {
            format!("Relic: {} — {price}g  ({})", relic.name(), relic.desc())
        };
        opts.push(Opt::new(label, Act::BuyRelic).when(!sold && run.gold >= price));
    }
    let label = if shop.purged {
        "Purge a card — DONE".to_string()
    } else {
        format!("Purge a card from your deck — {}g", shop.purge_cost)
    };
    opts.push(
        Opt::new(label, Act::Purge)
            .when(!shop.purged && run.gold >= shop.purge_cost && run.deck.len() > 1),
    );
    opts.push(Opt::new("Leave", Act::Continue));
    opts
}

pub(crate) fn event_view(id: u8, run: &RunState, t: f32) -> ChoiceView {
    let e = |o: u8| Act::Event(id, o);
    let (title, art, color, text, opts): (&str, &'static [&'static str], Color, &str, Vec<Opt>) =
        match id {
            0 => (
                "The Bleeding Altar",
                art::ALTAR,
                theme::BLOOD_LIGHT,
                "An altar of fused vertebrae weeps a slow red tide. Carved above it: \
             GIVE AND BE GIVEN. The candles lean toward you as if hungry.",
                vec![
                    Opt::new("Offer your blood — lose 8 HP, gain a relic", e(0)).when(run.hp > 8),
                    Opt::new("Walk away", e(9)),
                ],
            ),
            1 => (
                "The Whispering Skull",
                art::SKULL,
                theme::BONE,
                "A skull rests in a niche of the wall, jaw moving soundlessly. As you near, the \
             whispers become words: secrets of murder, of craft, of the things below.",
                vec![
                    Opt::new("Listen — gain a Rare card, add a Wound to your deck", e(0)),
                    Opt::new("Crush it — gain 25 gold", e(1)),
                ],
            ),
            2 => (
                "The Pool of Black Water",
                art::POOL,
                theme::STEEL,
                "A still pool, black as ink, fills a collapsed ossuary. Your reflection does not \
             quite move when you do.",
                vec![
                    Opt::new("Drink — heal 20 HP", e(0)),
                    Opt::new("Bathe — upgrade 2 random cards, lose 6 HP", e(1)),
                    Opt::new("Leave it undisturbed", e(9)),
                ],
            ),
            3 => (
                "The Hanged Pilgrim",
                art::GALLOWS,
                theme::ASH,
                "A pilgrim dangles from a rusted gibbet, a heavy purse still on his belt. His eyes are \
             closed. You are almost certain his eyes were open a moment ago.",
                vec![
                    Opt::new("Search the corpse — gain 45 gold, lose 5 HP", e(0)),
                    Opt::new("Give him the last rites — gain 5 Max HP", e(1)),
                    Opt::new("Leave him to the crows", e(9)),
                ],
            ),
            _ => {
                let wounds = run.deck.iter().filter(|c| c.id == CardId::Wound).count();
                let mut opts = vec![
                    Opt::new("Pay 35 gold — bury a card of your choice", e(0))
                        .when(run.gold >= 35 && run.deck.len() > 1),
                ];
                if wounds > 0 {
                    opts.push(Opt::new(
                        format!("Bury your Wounds — remove {wounds} Wound(s)"),
                        e(1),
                    ));
                }
                opts.push(Opt::new("Leave", e(9)));
                (
                    "The Gravedigger",
                    art::GRAVE,
                    theme::EMBER,
                    "A hunched gravedigger leans on his spade beside a fresh, empty grave. \"Every pilgrim \
                 carries something they'd rather bury,\" he rasps. \"For a price.\"",
                    opts,
                )
            }
        };
    ChoiceView {
        title: title.into(),
        art,
        art_color: color,
        text: text.into(),
        opts,
        shop: None,
        t0: t,
    }
}

pub fn fmt_time(secs: f64) -> String {
    let s = secs as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}
