//! Headless smoke tests: a bot plays full runs through the real input and render paths.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

use crate::app::{Act, App, Hit, Overlay, Phase, Screen};
use crate::cards::{Eff, Kind};
use crate::enemies::Intent;
use crate::model::{Class, St};

fn key(app: &mut App, code: KeyCode) {
    app.on_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn incoming(app: &App) -> i32 {
    let Screen::Combat(cv) = &app.screen else {
        return 0;
    };
    let c = &cv.c;
    (0..c.enemies.len())
        .filter(|&i| !c.enemies[i].dead)
        .map(|i| {
            let me = crate::combat::Who::Enemy(i);
            match c.enemies[i].intent {
                Intent::Attack(n) | Intent::AttackDefend(n, _) | Intent::AttackDebuff(n, _, _) => {
                    c.attack_value(me, crate::combat::Who::Player, n)
                }
                Intent::Multi(n, k) => c.attack_value(me, crate::combat::Who::Player, n) * k,
                _ => 0,
            }
        })
        .sum()
}

/// Picks a hand index to play, or None to end the turn.
fn choose_card(app: &App) -> Option<usize> {
    let Screen::Combat(cv) = &app.screen else {
        return None;
    };
    let c = &cv.c;
    let need_block = c.p.block < incoming(app);
    let mut best: Option<(i32, usize)> = None;
    for i in 0..c.hand.len() {
        if c.unplayable_reason(i).is_some() {
            continue;
        }
        let d = c.hand[i].def();
        if d.self_damage() > 0 && c.p.hp < 20 {
            continue;
        }
        let blocks = d.effs.iter().any(|e| matches!(e, Eff::Block(_)));
        let mut score = d.cost * 10 + 5;
        if need_block && blocks {
            score += 50;
        }
        if d.kind == Kind::Power {
            score += 30;
        }
        if d.effs
            .iter()
            .any(|e| matches!(e, Eff::Energy(_) | Eff::Draw(_)))
        {
            score += 40;
        }
        if best.is_none_or(|b| score > b.0) {
            best = Some((score, i));
        }
    }
    best.map(|b| b.1)
}

struct Outcome {
    victory: bool,
    floor: usize,
    secs: f64,
    turns: u32,
}

fn draw(term: &mut Terminal<TestBackend>, app: &App) {
    term.draw(|f| crate::ui::render(app, f)).unwrap();
}

fn mouse(app: &mut App, kind: MouseEventKind, x: u16, y: u16) {
    app.on_event(Event::Mouse(MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }));
}

/// The clickable area the last frame registered for `hit`, if any.
fn hit_rect(app: &App, hit: Hit) -> Option<Rect> {
    let list = if app.overlay != Overlay::None {
        app.ohits.borrow()
    } else {
        app.hits.borrow()
    };
    list.iter().rev().find(|(_, h)| *h == hit).map(|(r, _)| *r)
}

/// Renders a frame, then hovers and left-clicks the area registered for `hit`.
/// Returns false if that frame had no such area (e.g. a card still sliding in).
fn try_click(app: &mut App, term: &mut Terminal<TestBackend>, hit: Hit) -> bool {
    draw(term, app);
    let Some(r) = hit_rect(app, hit) else {
        return false;
    };
    mouse(app, MouseEventKind::Moved, r.x, r.y);
    mouse(app, MouseEventKind::Down(MouseButton::Left), r.x, r.y);
    true
}

fn click(app: &mut App, term: &mut Terminal<TestBackend>, hit: Hit) {
    assert!(try_click(app, term, hit), "no clickable area for {hit:?}");
}

fn right_click(app: &mut App) {
    mouse(app, MouseEventKind::Down(MouseButton::Right), 0, 0);
}

/// Plays one full run. With `use_mouse`, every action is a click on the area the
/// renderer registered for it instead of a key press.
fn play_run(class: usize, seed: u64, w: u16, h: u16, use_mouse: bool) -> Outcome {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    let mut app = App::new();
    let mut turns = 0;
    // Title -> New Descent -> class
    if use_mouse {
        click(&mut app, &mut term, Hit::Item(0));
        assert!(matches!(app.screen, Screen::ClassSelect));
        click(&mut app, &mut term, Hit::Item(class));
    } else {
        app.sel = 0;
        key(&mut app, KeyCode::Enter);
        assert!(matches!(app.screen, Screen::ClassSelect));
        for _ in 0..class {
            key(&mut app, KeyCode::Right);
        }
        key(&mut app, KeyCode::Enter);
    }
    assert!(matches!(app.screen, Screen::Map));
    // Make each run different but reproducible.
    app.run.as_mut().unwrap().rng = crate::rng::Rng::new(seed);
    let mut rng = crate::rng::Rng::new(seed ^ 77);
    let mut steps = 0;
    let mut floor_reached = 0;
    let mut secs = 0.0;
    loop {
        steps += 1;
        assert!(steps < 200_000, "run did not finish");
        app.update(0.05);
        if use_mouse || steps % 3 == 0 {
            draw(&mut term, &app);
        }
        if let Some(r) = &app.run {
            floor_reached = r.floor;
            secs = r.elapsed;
        }
        match &app.screen {
            Screen::Map => {
                let r = app.run.as_ref().unwrap();
                let n = r.map[r.floor].len();
                // Exercise overlays occasionally.
                if steps % 7 == 0 {
                    if use_mouse {
                        right_click(&mut app);
                        assert_eq!(app.overlay, Overlay::Pause);
                        click(&mut app, &mut term, Hit::Item(0)); // Resume
                        key(&mut app, KeyCode::Char('d'));
                        mouse(&mut app, MouseEventKind::ScrollDown, 0, 0);
                        mouse(&mut app, MouseEventKind::ScrollUp, 0, 0);
                        click(&mut app, &mut term, Hit::Item(0)); // click closes it
                    } else {
                        key(&mut app, KeyCode::Char('d'));
                        draw(&mut term, &app);
                        key(&mut app, KeyCode::Esc);
                        key(&mut app, KeyCode::Char('?'));
                        draw(&mut term, &app);
                        key(&mut app, KeyCode::Esc);
                    }
                    assert_eq!(app.overlay, Overlay::None);
                }
                let k = rng.below(n);
                if use_mouse {
                    click(&mut app, &mut term, Hit::Item(k));
                } else {
                    key(
                        &mut app,
                        KeyCode::Char(char::from_digit(k as u32 + 1, 10).unwrap()),
                    );
                }
            }
            Screen::Combat(cv) => {
                if cv.phase != Phase::Player || cv.timer > 0.8 {
                    continue;
                }
                // Now and then, look at the piles and try a card that can't be played.
                if steps % 13 == 0 {
                    key(&mut app, KeyCode::Char('d'));
                    draw(&mut term, &app);
                    key(&mut app, KeyCode::Esc);
                    let Screen::Combat(cv) = &app.screen else {
                        unreachable!()
                    };
                    if let Some(i) =
                        (0..cv.c.hand.len()).find(|&i| cv.c.unplayable_reason(i).is_some())
                    {
                        let clicked = if use_mouse {
                            try_click(&mut app, &mut term, Hit::Card(i))
                        } else {
                            app.sel = i;
                            key(&mut app, KeyCode::Enter);
                            true
                        };
                        let Screen::Combat(cv) = &app.screen else {
                            unreachable!()
                        };
                        assert!(
                            !clicked || cv.msg.is_some(),
                            "refusing a card should explain why"
                        );
                        draw(&mut term, &app);
                    }
                }
                if use_mouse {
                    let Screen::Combat(cv) = &app.screen else {
                        unreachable!()
                    };
                    let alive: Vec<usize> = (0..cv.c.enemies.len())
                        .filter(|&e| !cv.c.enemies[e].dead)
                        .collect();
                    if alive.len() > 1 {
                        let e = *rng.pick(&alive);
                        click(&mut app, &mut term, Hit::Enemy(e));
                        let Screen::Combat(cv) = &app.screen else {
                            unreachable!()
                        };
                        assert_eq!(cv.target, e, "clicking an enemy should target it");
                    }
                }
                match choose_card(&app) {
                    // Cards still sliding into the hand can't be clicked yet; wait a frame.
                    Some(i) if use_mouse => {
                        try_click(&mut app, &mut term, Hit::Card(i));
                    }
                    Some(i) => {
                        app.sel = i;
                        key(&mut app, KeyCode::Enter);
                    }
                    None => {
                        turns += 1;
                        if use_mouse {
                            click(&mut app, &mut term, Hit::EndTurn);
                        } else {
                            key(&mut app, KeyCode::Char('e'));
                        }
                    }
                }
            }
            Screen::Reward(rv) => {
                // Let the reward cards finish appearing before choosing.
                if app.screen_age() < 0.8 {
                    continue;
                }
                let n = rv.cards.len();
                let pick = if n == 0 { 0 } else { rng.below(n + 1) };
                if !use_mouse {
                    app.sel = pick;
                    key(&mut app, KeyCode::Enter);
                } else if pick < n {
                    click(&mut app, &mut term, Hit::Card(pick));
                } else if rng.chance(0.5) {
                    click(&mut app, &mut term, Hit::Item(n)); // skip button
                } else {
                    right_click(&mut app); // also skips
                }
            }
            Screen::Choice(cv) => {
                if app.screen_age() < 0.2 {
                    continue;
                }
                let enabled: Vec<usize> =
                    (0..cv.opts.len()).filter(|&i| cv.opts[i].enabled).collect();
                let k = *rng.pick(&enabled);
                if use_mouse {
                    click(&mut app, &mut term, Hit::Item(k));
                } else {
                    app.sel = k;
                    key(&mut app, KeyCode::Enter);
                }
            }
            Screen::DeckPick(_) => {
                if rng.chance(0.2) {
                    if use_mouse {
                        right_click(&mut app);
                    } else {
                        key(&mut app, KeyCode::Esc);
                    }
                } else if use_mouse {
                    mouse(&mut app, MouseEventKind::ScrollDown, 0, 0);
                    mouse(&mut app, MouseEventKind::ScrollUp, 0, 0);
                    click(&mut app, &mut term, Hit::Card(0));
                } else {
                    key(&mut app, KeyCode::Down);
                    key(&mut app, KeyCode::Up);
                    key(&mut app, KeyCode::Enter);
                }
            }
            Screen::End(_) if app.screen_age() < 1.2 => continue,
            Screen::End(es) => {
                let outcome = Outcome {
                    victory: es.victory,
                    floor: floor_reached,
                    secs,
                    turns,
                };
                if use_mouse {
                    click(&mut app, &mut term, Hit::Item(0));
                } else {
                    key(&mut app, KeyCode::Enter);
                }
                assert!(matches!(app.screen, Screen::Title));
                return outcome;
            }
            Screen::Title | Screen::ClassSelect => panic!("unexpected screen"),
        }
    }
}

fn isolate_saves() {
    let dir = std::env::temp_dir().join(format!("cotu-test-{}", std::process::id()));
    // SAFETY: tests touching the environment run in one thread (see below).
    unsafe { std::env::set_var("XDG_DATA_HOME", dir) };
}

#[test]
fn bot_runs_all_classes() {
    isolate_saves();
    for class in 0..3 {
        let mut wins = 0;
        let mut floors = 0;
        let mut secs = 0.0;
        let mut turns = 0;
        let mut hist = [0u32; 11];
        let n = 30;
        for seed in 0..n {
            let (w, h) = if seed % 2 == 0 { (120, 40) } else { (90, 30) };
            let o = play_run(class, seed * 7919 + class as u64, w, h, false);
            wins += o.victory as u32;
            floors += o.floor;
            secs += o.secs;
            turns += o.turns;
            if !o.victory {
                hist[o.floor + 1] += 1;
            }
        }
        eprintln!(
            "{:<12} wins {wins}/{n}  avg floor {:.1}  avg turns {:.1}  avg sim-time {:.0}s",
            Class::ALL[class].info().name,
            floors as f32 / n as f32 + 1.0,
            turns as f32 / n as f32,
            secs / n as f64
        );
        eprintln!("   deaths by floor: {:?}", &hist[1..]);
    }
}

#[test]
fn save_and_resume_mid_room() {
    isolate_saves();
    let mut app = App::new();
    app.sel = 0;
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Enter);
    assert!(matches!(app.screen, Screen::Map));
    key(&mut app, KeyCode::Char('1'));
    let Screen::Combat(cv) = &app.screen else {
        panic!("floor 1 should be combat")
    };
    let hp: Vec<i32> = cv.c.enemies.iter().map(|e| e.f.hp).collect();
    // Quit via pause menu and resume.
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.overlay, Overlay::Pause);
    for _ in 0..2 {
        key(&mut app, KeyCode::Down);
    }
    key(&mut app, KeyCode::Enter);
    assert!(matches!(app.screen, Screen::Title));
    assert!(app.has_save);
    let mut app = App::new();
    assert_eq!(app.sel, 1);
    key(&mut app, KeyCode::Enter);
    let Screen::Combat(cv) = &app.screen else {
        panic!("should resume into the same combat")
    };
    let hp2: Vec<i32> = cv.c.enemies.iter().map(|e| e.f.hp).collect();
    assert_eq!(hp, hp2, "resumed room should be identical");
    assert!(cv.c.p.st.get(St::Poison) == 0);
}

fn dump(term: &Terminal<TestBackend>) -> String {
    let buf = term.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
#[ignore]
fn snapshot_screens() {
    isolate_saves();
    let dir = std::env::var("SNAP_DIR").unwrap_or_else(|_| ".".into());
    let (w, h) = (
        std::env::var("SNAP_W")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(120),
        std::env::var("SNAP_H")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(40),
    );
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    let mut app = App::new();
    let snap = |term: &mut Terminal<TestBackend>, app: &mut App, name: &str| {
        app.update(1.5);
        term.draw(|f| crate::ui::render(app, f)).unwrap();
        std::fs::write(format!("{dir}/{name}.txt"), dump(term)).unwrap();
    };
    snap(&mut term, &mut app, "01_title");
    app.sel = 0;
    key(&mut app, KeyCode::Enter);
    snap(&mut term, &mut app, "02_class");
    key(&mut app, KeyCode::Right);
    key(&mut app, KeyCode::Right);
    key(&mut app, KeyCode::Enter);
    snap(&mut term, &mut app, "03_map");
    key(&mut app, KeyCode::Char('1'));
    snap(&mut term, &mut app, "04_combat");
    key(&mut app, KeyCode::Char('1'));
    app.update(0.1);
    term.draw(|f| crate::ui::render(&app, f)).unwrap();
    std::fs::write(format!("{dir}/05_combat_hit.txt"), dump(&term)).unwrap();
    key(&mut app, KeyCode::Char('?'));
    snap(&mut term, &mut app, "06_help");
    key(&mut app, KeyCode::Esc);
    // Jump to a merchant room on the current floor.
    let r = app.run.as_mut().unwrap();
    r.floor = 1;
    r.map[1] = vec![
        crate::run::RoomKind::Merchant,
        crate::run::RoomKind::Event,
        crate::run::RoomKind::Elite,
    ];
    r.gold = 200;
    app.screen = Screen::Map;
    snap(&mut term, &mut app, "07_map2");
    key(&mut app, KeyCode::Char('1'));
    snap(&mut term, &mut app, "08_merchant");
    let r = app.run.as_mut().unwrap();
    r.pending = Some(2);
    r.map[1][2] = crate::run::RoomKind::Boss;
    app.screen = Screen::Map;
    key(&mut app, KeyCode::Char('3'));
    snap(&mut term, &mut app, "09_boss");
}

#[test]
fn tutorial_plays_through() {
    use crate::tutorial::Goal;
    isolate_saves();
    let mut term = Terminal::new(TestBackend::new(100, 34)).unwrap();
    let mut app = App::new();
    app.sel = 2;
    key(&mut app, KeyCode::Enter);
    assert!(app.tutorial.is_some());
    let hand_index = |app: &App, pred: &dyn Fn(crate::cards::Card) -> bool| {
        let Screen::Combat(cv) = &app.screen else {
            panic!("tutorial should be in combat")
        };
        (0..cv.c.hand.len()).find(|&i| pred(cv.c.hand[i]) && cv.c.unplayable_reason(i).is_none())
    };
    // Ending the turn early is refused.
    key(&mut app, KeyCode::Char('e'));
    let Screen::Combat(cv) = &app.screen else {
        panic!()
    };
    assert_eq!(cv.phase, Phase::Player);
    // Card keys are ignored while the guide is still talking.
    let goal = app.tutorial.as_ref().unwrap().goal();
    assert_eq!(goal, Goal::Read);
    key(&mut app, KeyCode::Char('1'));
    assert_eq!(app.tutorial.as_ref().unwrap().goal(), goal);
    // The pause menu works mid-tutorial.
    key(&mut app, KeyCode::Esc);
    term.draw(|f| crate::ui::render(&app, f)).unwrap();
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.overlay, Overlay::None);
    let mut tried = None;
    for _ in 0..2_000 {
        app.update(0.05);
        term.draw(|f| crate::ui::render(&app, f)).unwrap();
        let Some(tu) = &app.tutorial else { break };
        let Screen::Combat(cv) = &app.screen else {
            panic!()
        };
        if cv.phase != Phase::Player && tu.goal() != Goal::Done {
            continue;
        }
        // First try a card that doesn't fit the step: the guide should refuse it.
        let goal = tu.goal();
        if tried != Some(goal) {
            tried = Some(goal);
            let wrong = match goal {
                Goal::PlayAttack => hand_index(&app, &|c| c.def().kind != Kind::Attack),
                Goal::GainBlock => hand_index(&app, &|c| {
                    !c.def().effs.iter().any(|e| matches!(e, Eff::Block(_)))
                }),
                Goal::Play(id) => hand_index(&app, &|c| c.id != id),
                _ => None,
            };
            if let Some(i) = wrong {
                let hand = cv.c.hand.len();
                app.sel = i;
                key(&mut app, KeyCode::Enter);
                let Screen::Combat(cv) = &app.screen else {
                    panic!()
                };
                assert_eq!(cv.c.hand.len(), hand, "the tutorial should refuse {goal:?}");
                assert_eq!(app.tutorial.as_ref().unwrap().goal(), goal);
                term.draw(|f| crate::ui::render(&app, f)).unwrap();
                continue;
            }
        }
        let pick = match tu.goal() {
            Goal::Read | Goal::Done => {
                key(&mut app, KeyCode::Enter);
                continue;
            }
            Goal::Watch => continue,
            Goal::PlayAttack | Goal::EndTurn | Goal::Win => {
                hand_index(&app, &|c| c.def().kind == Kind::Attack)
            }
            Goal::GainBlock => hand_index(&app, &|c| {
                c.def().effs.iter().any(|e| matches!(e, Eff::Block(_)))
            }),
            Goal::Play(id) => hand_index(&app, &|c| c.id == id),
        };
        match pick {
            Some(i) => {
                app.sel = i;
                key(&mut app, KeyCode::Enter);
            }
            None => key(&mut app, KeyCode::Char('e')),
        }
    }
    assert!(app.tutorial.is_none(), "tutorial should finish");
    assert!(matches!(app.screen, Screen::Title));
    assert!(app.run.is_none());
}

fn digit(k: usize) -> KeyCode {
    KeyCode::Char(char::from_digit(k as u32 + 1, 10).unwrap())
}

fn toast(app: &App) -> &str {
    app.fx.toast.as_ref().map_or("", |t| t.0.as_str())
}

/// Starts a fresh run as the Blood Knight and returns on the map.
fn start_run(app: &mut App) {
    app.sel = 0;
    key(app, KeyCode::Enter);
    key(app, KeyCode::Enter);
    assert!(matches!(app.screen, Screen::Map));
}

#[test]
fn mouse_bot_runs_all_classes() {
    isolate_saves();
    for class in 0..3 {
        for seed in 0..2 {
            let (w, h) = if seed % 2 == 0 { (120, 40) } else { (90, 30) };
            play_run(class, 31 * seed + class as u64, w, h, true);
        }
    }
}

#[test]
fn keyboard_aliases_and_title_menu() {
    isolate_saves();
    crate::run::RunState::delete_save();
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut app = App::new();
    assert!(!app.has_save);
    // Continue is disabled without a save, so vertical movement skips it.
    for (code, sel) in [
        (KeyCode::Down, 2),
        (KeyCode::Up, 0),
        (KeyCode::Char('j'), 2),
        (KeyCode::Char('s'), 3),
        (KeyCode::Tab, 4),
        (KeyCode::BackTab, 3),
        (KeyCode::Char('k'), 2),
        (KeyCode::Char('w'), 0),
    ] {
        key(&mut app, code);
        assert_eq!(app.sel, sel, "after {code:?}");
        draw(&mut term, &app);
    }
    // Ignored input: unmapped keys, key releases, and non-input events.
    key(&mut app, KeyCode::F(1));
    let mut release = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
    release.kind = KeyEventKind::Release;
    app.on_event(Event::Key(release));
    app.on_event(Event::FocusGained);
    assert_eq!(app.sel, 0);
    // Codex from the title, closed with q.
    key(&mut app, KeyCode::Char('4'));
    assert_eq!(app.overlay, Overlay::Help);
    draw(&mut term, &app);
    key(&mut app, KeyCode::Char('q'));
    assert_eq!(app.overlay, Overlay::None);
    // Class select: move with l / h / a, then back out.
    app.sel = 0;
    key(&mut app, KeyCode::Enter);
    assert!(matches!(app.screen, Screen::ClassSelect));
    for (code, sel) in [
        (KeyCode::Char('l'), 1),
        (KeyCode::Char('h'), 0),
        (KeyCode::Char('a'), 2),
    ] {
        key(&mut app, code);
        assert_eq!(app.sel, sel, "after {code:?}");
        draw(&mut term, &app);
    }
    key(&mut app, KeyCode::Esc);
    assert!(matches!(app.screen, Screen::Title));
    // Every way to quit from the title.
    let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    for quit in [
        Event::Key(KeyEvent::new(KeyCode::Char('5'), KeyModifiers::NONE)),
        Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
        ctrl_c,
    ] {
        app.quit = false;
        app.on_event(quit.clone());
        assert!(app.quit, "{quit:?} should quit");
    }
}

#[test]
fn pause_menu_paths() {
    isolate_saves();
    crate::run::RunState::delete_save();
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut app = App::new();
    start_run(&mut app);
    // Codex from the pause menu.
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.overlay, Overlay::Pause);
    draw(&mut term, &app);
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.overlay, Overlay::Help);
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.overlay, Overlay::None);
    // Right click closes the pause menu.
    key(&mut app, KeyCode::Esc);
    right_click(&mut app);
    assert_eq!(app.overlay, Overlay::None);
    // Save & Quit from the map, then Continue.
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('3'));
    assert!(matches!(app.screen, Screen::Title));
    assert!(app.has_save);
    assert_eq!(app.sel, 1);
    draw(&mut term, &app);
    key(&mut app, KeyCode::Enter);
    assert!(matches!(app.screen, Screen::Map));
    // Save & Quit mid-combat resumes into the same room.
    key(&mut app, KeyCode::Char('1'));
    assert!(matches!(app.screen, Screen::Combat(_)));
    key(&mut app, KeyCode::Esc);
    draw(&mut term, &app);
    key(&mut app, KeyCode::Char('3'));
    key(&mut app, KeyCode::Enter);
    assert!(matches!(app.screen, Screen::Combat(_)));
    // Abandon Run deletes the save.
    key(&mut app, KeyCode::Esc);
    key(&mut app, KeyCode::Char('4'));
    assert!(matches!(app.screen, Screen::Title));
    assert!(!app.has_save);
    assert!(!crate::run::RunState::has_save());
}

#[test]
fn unreadable_and_unwritable_saves() {
    isolate_saves();
    let path = crate::run::save_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "not json").unwrap();
    let mut app = App::new();
    assert!(app.has_save);
    key(&mut app, KeyCode::Enter); // Continue
    assert!(matches!(app.screen, Screen::Title));
    assert!(!app.has_save);
    assert_eq!(toast(&app), "The save file is unreadable.");
    crate::run::RunState::delete_save();

    // A regular file where the save directory should be makes every write fail.
    let blocker = std::env::temp_dir().join(format!("cotu-blocker-{}", std::process::id()));
    std::fs::write(&blocker, "").unwrap();
    // SAFETY: tests touching the environment run in one thread.
    unsafe { std::env::set_var("XDG_DATA_HOME", &blocker) };
    start_run(&mut app);
    key(&mut app, KeyCode::Char('1')); // entering a room saves again
    let failed = toast(&app) == "Could not write save file!";
    std::fs::remove_file(&blocker).unwrap();
    isolate_saves();
    assert!(failed, "a failed save should be reported");
}

#[test]
fn events_and_rooms_edge_cases() {
    use crate::cards::{Card, CardId};
    use crate::model::Relic;
    use crate::run::RoomKind;
    isolate_saves();
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut app = App::new();
    start_run(&mut app);
    let open = |app: &mut App, id: u8| {
        let cv = crate::app::event_view(id, app.run.as_ref().unwrap(), app.fx.t);
        app.screen = Screen::Choice(cv);
    };
    let choose = |app: &mut App, term: &mut Terminal<TestBackend>, act: Act| {
        draw(term, app);
        let Screen::Choice(cv) = &app.screen else {
            panic!("not on a choice screen")
        };
        let k = cv.opts.iter().position(|o| o.act == act && o.enabled);
        key(
            app,
            digit(k.unwrap_or_else(|| panic!("{act:?} is not available"))),
        );
        draw(term, app);
    };
    let text = |app: &App| match &app.screen {
        Screen::Choice(cv) => cv.text.clone(),
        _ => panic!("not on a choice screen"),
    };

    // Every event option.
    for (id, opt) in [(1, 0), (1, 1), (2, 0), (2, 1), (3, 0), (3, 1), (0, 9)] {
        open(&mut app, id);
        choose(&mut app, &mut term, Act::Event(id, opt));
        assert!(matches!(app.screen, Screen::Choice(_)));
    }
    // The altar has nothing left to give once every relic is owned.
    run(&mut app).relics = Relic::ALL.to_vec();
    open(&mut app, 0);
    choose(&mut app, &mut term, Act::Event(0, 0));
    assert!(text(&app).contains("gives nothing back"));
    // Bathing changes nothing when every card is already upgraded.
    for c in &mut run(&mut app).deck {
        c.up = c.upgradable() || c.up;
    }
    open(&mut app, 2);
    choose(&mut app, &mut term, Act::Event(2, 1));
    assert!(text(&app).contains("nothing within you changes"));
    // The gravedigger buries Wounds, or a chosen card for gold.
    run(&mut app).deck.push(Card::new(CardId::Wound));
    open(&mut app, 4);
    choose(&mut app, &mut term, Act::Event(4, 1));
    assert!(run(&mut app).deck.iter().all(|c| c.id != CardId::Wound));
    run(&mut app).gold = 100;
    let deck_len = run(&mut app).deck.len();
    open(&mut app, 4);
    choose(&mut app, &mut term, Act::Event(4, 0));
    assert!(matches!(app.screen, Screen::DeckPick(_)));
    draw(&mut term, &app);
    key(&mut app, KeyCode::Enter);
    assert!(matches!(app.screen, Screen::Map));
    assert_eq!(run(&mut app).deck.len(), deck_len - 1);
    assert_eq!(run(&mut app).gold, 65);

    // A shrine can't upgrade anything when the deck is fully upgraded.
    let enter = |app: &mut App, room: RoomKind| {
        let r = app.run.as_mut().unwrap();
        r.map[r.floor] = vec![room];
        key(app, KeyCode::Char('1'));
        assert!(matches!(app.screen, Screen::Choice(_)));
    };
    enter(&mut app, RoomKind::Shrine);
    choose(&mut app, &mut term, Act::Pray);
    assert!(matches!(app.screen, Screen::Choice(_)));
    assert_eq!(toast(&app), "You have no cards that can be chosen.");
    choose(&mut app, &mut term, Act::Continue);
    // A chest holds only gold once every relic is owned.
    let gold = run(&mut app).gold;
    enter(&mut app, RoomKind::Treasure);
    choose(&mut app, &mut term, Act::Open);
    assert!(text(&app).contains("gold"));
    assert!(run(&mut app).gold > gold);
}

#[test]
fn tutorial_leave_and_loss() {
    isolate_saves();
    let mut term = Terminal::new(TestBackend::new(100, 34)).unwrap();
    let mut app = App::new();
    // Leave from the tutorial's own pause menu.
    app.sel = 2;
    key(&mut app, KeyCode::Enter);
    assert!(app.tutorial.is_some());
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.overlay, Overlay::Pause);
    assert_eq!(app.pause_items().len(), 3);
    draw(&mut term, &app);
    key(&mut app, KeyCode::Char('3'));
    assert!(app.tutorial.is_none());
    assert!(matches!(app.screen, Screen::Title));
    assert_eq!(app.sel, 2);
    // Dying in the tutorial returns to the title instead of ending a run.
    key(&mut app, KeyCode::Enter);
    let Screen::Combat(cv) = &mut app.screen else {
        panic!("tutorial should be in combat")
    };
    cv.c.p.hp = 0;
    cv.phase = Phase::Lost;
    cv.timer = 0.0;
    app.update(0.05);
    assert!(app.tutorial.is_none());
    assert!(app.run.is_none());
    assert!(matches!(app.screen, Screen::Title));
    assert!(toast(&app).contains("Try again"));
}

#[test]
fn tiny_terminal_shows_a_warning() {
    isolate_saves();
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let app = App::new();
    draw(&mut term, &app);
    assert!(dump(&term).contains("too narrow"));
}

fn run(app: &mut App) -> &mut crate::run::RunState {
    app.run.as_mut().unwrap()
}

#[test]
fn combat_keyboard_controls() {
    isolate_saves();
    // A tall terminal gets the largest hand layout.
    let mut term = Terminal::new(TestBackend::new(140, 46)).unwrap();
    let mut app = App::new();
    start_run(&mut app);
    // Find a seed whose first fight has two enemies, so there is a target to switch to.
    let mut found = false;
    for seed in 0..200 {
        run(&mut app).rng = crate::rng::Rng::new(seed);
        app.screen = Screen::Map;
        key(&mut app, KeyCode::Char('1'));
        let Screen::Combat(cv) = &app.screen else {
            panic!("floor 1 should be combat")
        };
        if cv.c.enemies.len() > 1 {
            found = true;
            break;
        }
    }
    assert!(found, "no seed gave a fight with two enemies");
    for _ in 0..40 {
        app.update(0.05);
    }
    draw(&mut term, &app);
    let Screen::Combat(cv) = &app.screen else {
        unreachable!()
    };
    let n = cv.c.hand.len();
    assert!(n > 1);
    app.sel = 0;
    key(&mut app, KeyCode::Right);
    assert_eq!(app.sel, 1);
    key(&mut app, KeyCode::Left);
    key(&mut app, KeyCode::Left);
    assert_eq!(app.sel, n - 1, "selection wraps around");
    let target = |app: &App| match &app.screen {
        Screen::Combat(cv) => cv.target,
        _ => unreachable!(),
    };
    let first = target(&app);
    key(&mut app, KeyCode::Down);
    assert_ne!(target(&app), first);
    key(&mut app, KeyCode::Up);
    assert_eq!(target(&app), first);
    // A number key plays that card.
    let playable = |app: &App| match &app.screen {
        Screen::Combat(cv) => (0..cv.c.hand.len()).find(|&i| cv.c.unplayable_reason(i).is_none()),
        _ => unreachable!(),
    };
    let i = playable(&app).expect("a playable card");
    key(&mut app, digit(i));
    let Screen::Combat(cv) = &app.screen else {
        unreachable!()
    };
    assert_eq!(cv.c.hand.len(), n - 1);
    draw(&mut term, &app);
}

/// Writes an SVG of the Floor 1 combat screen to `$SVG_OUT` (used for the README).
#[test]
#[ignore]
fn snapshot_svg() {
    use ratatui::style::{Color, Modifier};
    isolate_saves();
    let out = std::env::var("SVG_OUT").unwrap_or_else(|_| "screenshot.svg".into());
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut app = App::new();
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Char('1'));
    for _ in 0..60 {
        app.update(0.1);
    }
    term.draw(|f| crate::ui::render(&app, f)).unwrap();
    let buf = term.backend().buffer();
    let (cw, ch) = (9.6f32, 20.0f32);
    let hex = |c: Color, d: &str| match c {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        _ => d.to_string(),
    };
    let (w, h) = (buf.area.width as f32 * cw, buf.area.height as f32 * ch);
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n<rect width=\"100%\" height=\"100%\" fill=\"#08060a\"/>\n<g font-family=\"'DejaVu Sans Mono','Menlo','Consolas',monospace\" font-size=\"16\" text-anchor=\"middle\">\n",
        w, h, w, h
    );
    for y in 0..buf.area.height {
        // Merge horizontal runs of identical backgrounds into one rect.
        let mut x = 0;
        while x < buf.area.width {
            let bg = buf[(x, y)].bg;
            let mut e = x + 1;
            while e < buf.area.width && buf[(e, y)].bg == bg {
                e += 1;
            }
            if matches!(bg, Color::Rgb(..)) && hex(bg, "") != "#08060a" {
                s += &format!(
                    "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\"/>\n",
                    x as f32 * cw,
                    y as f32 * ch,
                    (e - x) as f32 * cw,
                    ch,
                    hex(bg, "")
                );
            }
            x = e;
        }
        for x in 0..buf.area.width {
            let c = &buf[(x, y)];
            let sym = c.symbol();
            if sym.trim().is_empty() {
                continue;
            }
            let sym = sym
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            let bold = if c.modifier.contains(Modifier::BOLD) {
                " font-weight=\"bold\""
            } else {
                ""
            };
            s += &format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"{}\"{}>{}</text>\n",
                x as f32 * cw + cw / 2.0,
                y as f32 * ch + 15.0,
                hex(c.fg, "#ded2b8"),
                bold,
                sym
            );
        }
    }
    s += "</g>\n</svg>\n";
    std::fs::write(out, s).unwrap();
}
