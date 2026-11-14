//! Headless smoke tests: a bot plays full runs through the real input and render paths.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Overlay, Phase, Screen};
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

fn play_run(class: usize, seed: u64, w: u16, h: u16) -> Outcome {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    let mut app = App::new();
    let mut turns = 0;
    // Title -> New Descent
    app.sel = 0;
    key(&mut app, KeyCode::Enter);
    assert!(matches!(app.screen, Screen::ClassSelect));
    for _ in 0..class {
        key(&mut app, KeyCode::Right);
    }
    key(&mut app, KeyCode::Enter);
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
        if steps % 3 == 0 {
            term.draw(|f| crate::ui::render(&app, f)).unwrap();
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
                    key(&mut app, KeyCode::Char('d'));
                    term.draw(|f| crate::ui::render(&app, f)).unwrap();
                    key(&mut app, KeyCode::Esc);
                    key(&mut app, KeyCode::Char('?'));
                    term.draw(|f| crate::ui::render(&app, f)).unwrap();
                    key(&mut app, KeyCode::Esc);
                    assert_eq!(app.overlay, Overlay::None);
                }
                let k = rng.below(n);
                key(
                    &mut app,
                    KeyCode::Char(char::from_digit(k as u32 + 1, 10).unwrap()),
                );
            }
            Screen::Combat(cv) => {
                if cv.phase != Phase::Player || cv.timer > 0.8 {
                    continue;
                }
                match choose_card(&app) {
                    Some(i) => {
                        app.sel = i;
                        key(&mut app, KeyCode::Enter);
                    }
                    None => {
                        turns += 1;
                        key(&mut app, KeyCode::Char('e'));
                    }
                }
            }
            Screen::Reward(rv) => {
                let pick = if rv.cards.is_empty() {
                    0
                } else {
                    rng.below(rv.cards.len() + 1)
                };
                app.sel = pick;
                key(&mut app, KeyCode::Enter);
            }
            Screen::Choice(cv) => {
                if app.screen_age() < 0.2 {
                    continue;
                }
                let enabled: Vec<usize> =
                    (0..cv.opts.len()).filter(|&i| cv.opts[i].enabled).collect();
                let k = *rng.pick(&enabled);
                app.sel = k;
                key(&mut app, KeyCode::Enter);
            }
            Screen::DeckPick(_) => {
                if rng.chance(0.2) {
                    key(&mut app, KeyCode::Esc);
                } else {
                    key(&mut app, KeyCode::Enter);
                }
            }
            Screen::End(es) => {
                return Outcome {
                    victory: es.victory,
                    floor: floor_reached,
                    secs,
                    turns,
                };
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
            let o = play_run(class, seed * 7919 + class as u64, w, h);
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
