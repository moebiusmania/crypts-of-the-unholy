use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget, Wrap};

use crate::app::*;
use crate::art;
use crate::cards::{Card, Kind};
use crate::combat::{Combat, Who};
use crate::enemies::Intent;
use crate::fx::SlashStyle;
use crate::model::{Class, Fighter, St};
use crate::run::{FLOORS, RunState};
use crate::theme::{self, flicker, hash, mix, scale};

const MIN_W: u16 = 90;
const MIN_H: u16 = 30;

// ---------------------------------------------------------------------- primitives

fn fill(buf: &mut Buffer, r: Rect, bg: Color) {
    let r = r.intersection(buf.area);
    for y in r.top()..r.bottom() {
        for x in r.left()..r.right() {
            if let Some(c) = buf.cell_mut((x, y)) {
                c.reset();
                c.set_bg(bg);
            }
        }
    }
}

fn put(buf: &mut Buffer, x: i32, y: i32, s: &str, st: Style) {
    let a = buf.area;
    if y < a.y as i32 || y >= a.bottom() as i32 {
        return;
    }
    for (cx, ch) in (x..).zip(s.chars()) {
        if cx >= a.right() as i32 {
            break;
        }
        if cx >= a.x as i32
            && let Some(c) = buf.cell_mut((cx as u16, y as u16))
        {
            c.set_char(ch);
            c.set_style(st);
        }
    }
}

fn width(s: &str) -> i32 {
    s.chars().count() as i32
}

fn put_center(buf: &mut Buffer, r: Rect, y: i32, s: &str, st: Style) {
    let s = truncate(s, r.width as usize);
    let x = r.x as i32 + (r.width as i32 - width(&s)) / 2;
    put(buf, x, y, &s, st);
}

fn fg(c: Color) -> Style {
    Style::new().fg(c)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut o: String = s.chars().take(n.saturating_sub(1)).collect();
        o.push('…');
        o
    }
}

fn art_width(art: &[&str]) -> i32 {
    art.iter().map(|l| width(l)).max().unwrap_or(0)
}

/// Draws ASCII art; spaces are transparent. `style` may return None to skip a cell.
fn draw_art(
    buf: &mut Buffer,
    x: i32,
    y: i32,
    art: &[&str],
    clip: Rect,
    mut style: impl FnMut(i32, i32, char) -> Option<Style>,
) {
    for (row, line) in art.iter().enumerate() {
        let yy = y + row as i32;
        if yy < clip.y as i32 || yy >= clip.bottom() as i32 {
            continue;
        }
        for (col, ch) in line.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            let xx = x + col as i32;
            if xx < clip.x as i32 || xx >= clip.right() as i32 {
                continue;
            }
            if let Some(st) = style(col as i32, row as i32, ch)
                && let Some(c) = buf.cell_mut((xx as u16, yy as u16))
            {
                c.set_char(ch);
                c.set_style(st);
            }
        }
    }
}

fn panel(f: &mut Frame, r: Rect, title: &str, color: Color, double: bool) -> Rect {
    let r = r.intersection(f.area());
    fill(f.buffer_mut(), r, theme::PANEL);
    let mut b = Block::default()
        .borders(Borders::ALL)
        .border_type(if double {
            BorderType::Double
        } else {
            BorderType::Rounded
        })
        .border_style(Style::new().fg(color).bg(theme::PANEL));
    if !title.is_empty() {
        b = b.title(Line::from(Span::styled(
            format!(" {title} "),
            Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
        )));
    }
    let inner = b.inner(r);
    f.render_widget(b, r);
    inner
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

fn paragraph(f: &mut Frame, r: Rect, text: &str, st: Style, align: Alignment) {
    let r = r.intersection(f.area());
    Paragraph::new(text)
        .style(st)
        .alignment(align)
        .wrap(Wrap { trim: true })
        .render(r, f.buffer_mut());
}

/// Rising embers; stateless so they animate from time alone.
fn embers(buf: &mut Buffer, r: Rect, t: f32, n: u32, base: Color) {
    if r.width < 2 || r.height < 2 {
        return;
    }
    for i in 0..n {
        let speed = 0.04 + (hash(i, 2, 0) % 100) as f32 / 100.0 * 0.08;
        let off = (hash(i, 3, 0) % 1000) as f32 / 1000.0;
        let phase = (t * speed + off).fract();
        let y = r.bottom() as f32 - 1.0 - phase * (r.height as f32 - 1.0);
        let hx = (hash(i, 1, 0) % r.width as u32) as f32;
        let x = r.x as f32 + hx + (t * 0.8 + i as f32).sin() * 1.5;
        if x < r.x as f32 || x >= r.right() as f32 {
            continue;
        }
        let ch = match (phase * 4.0) as u32 {
            0 => "*",
            1 => "·",
            2 => "'",
            _ => ".",
        };
        let c = mix(base, theme::BG, phase * 0.9);
        let cell = buf
            .cell((x as u16, y as u16))
            .map(|c| c.symbol() == " ")
            .unwrap_or(false);
        if cell {
            put(buf, x as i32, y as i32, ch, fg(c));
        }
    }
}

fn torch(buf: &mut Buffer, x: i32, y: i32, t: f32, seed: f32) {
    let fl = flicker(t, seed);
    let flame = mix(theme::EMBER, theme::GOLD, fl);
    let tips = ["(", ")", "{", "}", "^"];
    let k = ((t * 9.0 + seed * 3.0) as usize) % tips.len();
    let k2 = ((t * 7.0 + seed * 5.0) as usize) % tips.len();
    put(
        buf,
        x + 1,
        y - 1,
        if fl > 0.6 { "'" } else { "." },
        fg(mix(theme::EMBER, theme::BG, 0.4)),
    );
    put(buf, x, y, tips[k], fg(flame).add_modifier(Modifier::BOLD));
    put(
        buf,
        x + 1,
        y,
        tips[k2],
        fg(mix(flame, theme::BLOOD_LIGHT, 0.4)).add_modifier(Modifier::BOLD),
    );
    put(buf, x, y + 1, "\\/", fg(theme::ASH));
    put(buf, x, y + 2, "||", fg(theme::DIM));
    put(buf, x, y + 3, "||", fg(theme::DIM));
}

fn button(f: &mut Frame, r: Rect, label: &str, color: Color, selected: bool) {
    let r = r.intersection(f.area());
    let bg = if selected {
        theme::PANEL_HI
    } else {
        theme::PANEL
    };
    fill(f.buffer_mut(), r, bg);
    let b = Block::default()
        .borders(Borders::ALL)
        .border_type(if selected {
            BorderType::Double
        } else {
            BorderType::Rounded
        })
        .border_style(
            Style::new()
                .fg(if selected { color } else { scale(color, 0.6) })
                .bg(bg),
        );
    let inner = b.inner(r);
    f.render_widget(b, r);
    let st = Style::new()
        .fg(if selected { theme::BONE } else { theme::ASH })
        .bg(bg)
        .add_modifier(Modifier::BOLD);
    put_center(
        f.buffer_mut(),
        inner,
        inner.y as i32 + inner.height as i32 / 2,
        label,
        st,
    );
}

// ---------------------------------------------------------------------- entry

pub fn render(app: &App, f: &mut Frame) {
    app.hits.borrow_mut().clear();
    app.ohits.borrow_mut().clear();
    let area = f.area();
    fill(f.buffer_mut(), area, theme::BG);
    if area.width < MIN_W || area.height < MIN_H {
        let msg = format!(
            "The crypt is too narrow. Enlarge your terminal to at least {MIN_W}x{MIN_H} (now {}x{}).",
            area.width, area.height
        );
        paragraph(
            f,
            centered(area, area.width.saturating_sub(4), 5),
            &msg,
            fg(theme::BLOOD_LIGHT),
            Alignment::Center,
        );
        return;
    }
    match &app.screen {
        Screen::Title => render_title(app, f),
        Screen::ClassSelect => render_class_select(app, f),
        Screen::Map => render_map(app, f),
        Screen::Combat(cv) => render_combat(app, f, cv),
        Screen::Reward(rv) => render_reward(app, f, rv),
        Screen::Choice(cv) => render_choice(app, f, cv),
        Screen::DeckPick(dp) => render_deckpick(app, f, dp),
        Screen::End(es) => render_end(app, f, es),
    }
    match app.overlay {
        Overlay::None => {}
        Overlay::Pause => render_pause(app, f),
        Overlay::Deck => render_deck_overlay(app, f),
        Overlay::Help => render_help(app, f),
    }
    if let Some((msg, t0)) = &app.fx.toast {
        let age = app.fx.t - t0;
        let a = (age / 0.2).min(1.0) * ((2.5 - age) / 0.5).clamp(0.0, 1.0);
        let w = (width(msg) + 6).min(area.width as i32) as u16;
        let r = Rect {
            x: area.x + (area.width - w) / 2,
            y: area.bottom().saturating_sub(4),
            width: w,
            height: 3,
        };
        let buf = f.buffer_mut();
        fill(buf, r, theme::PANEL);
        let bc = mix(theme::BG, theme::GOLD, a);
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(bc).bg(theme::PANEL))
            .render(r, buf);
        put_center(
            buf,
            r,
            r.y as i32 + 1,
            msg,
            Style::new()
                .fg(mix(theme::BG, theme::BONE, a))
                .bg(theme::PANEL),
        );
    }
}

// ---------------------------------------------------------------------- shared widgets

fn status_bar(app: &App, f: &mut Frame, r: Rect, combat: Option<&Combat>) {
    let Some(run) = &app.run else { return };
    let info = run.class.info();
    let (hp, max, block) = match combat {
        Some(c) => (c.p.hp, c.p.max_hp, c.p.block),
        None => (run.hp, run.max_hp, 0),
    };
    let buf = f.buffer_mut();
    fill(buf, r, theme::PANEL);
    let mut spans = vec![
        Span::styled(
            format!(" ✝ {} ✝ ", info.name.to_uppercase()),
            Style::new().fg(info.color).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ♥ ", fg(theme::BLOOD_LIGHT)),
        Span::styled(
            format!("{hp}/{max}"),
            Style::new().fg(theme::BONE).add_modifier(Modifier::BOLD),
        ),
    ];
    if block > 0 {
        spans.push(Span::styled(format!("  ◆ {block}"), fg(theme::STEEL)));
    }
    spans.extend([
        Span::styled("   ● ", fg(theme::GOLD)),
        Span::styled(format!("{} gold", run.gold), fg(theme::BONE)),
        Span::styled("   Floor ", fg(theme::ASH)),
        Span::styled(format!("{}/{}", run.floor + 1, FLOORS), fg(theme::BONE)),
        Span::styled("   Deck ", fg(theme::ASH)),
        Span::styled(format!("{}", run.deck.len()), fg(theme::BONE)),
        Span::styled("   ", fg(theme::ASH)),
        Span::styled(fmt_time(run.elapsed), fg(theme::ASH)),
    ]);
    if !run.relics.is_empty() {
        spans.push(Span::styled("   Relics: ", fg(theme::ASH)));
        let names: Vec<&str> = run.relics.iter().map(|r| r.name()).collect();
        spans.push(Span::styled(names.join(", "), fg(theme::GOLD)));
    }
    Paragraph::new(Line::from(spans))
        .style(Style::new().bg(theme::PANEL))
        .render(r, buf);
}

fn footer(f: &mut Frame, r: Rect, text: &str) {
    let text = if width(text) > r.width as i32 {
        text.split(" · ").take(4).collect::<Vec<_>>().join(" · ")
    } else {
        text.to_string()
    };
    let text = text.as_str();
    let buf = f.buffer_mut();
    put_center(buf, r, r.y as i32, text, fg(theme::DIM));
}

#[allow(clippy::too_many_arguments)]
pub fn draw_card(
    f: &mut Frame,
    r: Rect,
    card: &Card,
    selected: bool,
    playable: bool,
    strength: i32,
    weak: bool,
    t: f32,
) {
    let r = r.intersection(f.area());
    if r.width < 6 || r.height < 3 {
        return;
    }
    let d = card.def();
    let kc = d.kind.color();
    let bg = if selected {
        theme::PANEL_HI
    } else {
        theme::PANEL
    };
    let pulse = 0.5 + 0.5 * (t * 4.0).sin();
    let border = if selected {
        mix(kc, theme::BONE, 0.2 + 0.3 * pulse)
    } else if playable {
        scale(kc, 0.7)
    } else {
        theme::DIM
    };
    let buf = f.buffer_mut();
    fill(buf, r, bg);
    let b = Block::default()
        .borders(Borders::ALL)
        .border_type(if selected {
            BorderType::Double
        } else {
            BorderType::Rounded
        })
        .border_style(Style::new().fg(border).bg(bg));
    let inner = b.inner(r);
    b.render(r, buf);
    let cost = if d.kind == Kind::Curse {
        " ✗ ".to_string()
    } else {
        format!(" {} ", d.cost)
    };
    put(
        buf,
        r.x as i32 + 1,
        r.y as i32,
        &cost,
        Style::new()
            .fg(theme::BG)
            .bg(if playable { theme::GOLD } else { theme::ASH })
            .add_modifier(Modifier::BOLD),
    );
    put(
        buf,
        r.right() as i32 - 3,
        r.y as i32,
        "◆",
        Style::new().fg(d.rarity.color()).bg(bg),
    );
    let name_col = if card.up {
        theme::HEAL
    } else if playable {
        theme::BONE
    } else {
        theme::ASH
    };
    let name = truncate(&card.name(), inner.width as usize);
    put_center(
        buf,
        inner,
        inner.y as i32,
        &name,
        Style::new()
            .fg(name_col)
            .bg(bg)
            .add_modifier(Modifier::BOLD),
    );
    let mut y = inner.y + 1;
    if inner.height >= 6 {
        put_center(
            buf,
            inner,
            y as i32,
            d.kind.label(),
            Style::new()
                .fg(scale(kc, 0.8))
                .bg(bg)
                .add_modifier(Modifier::ITALIC),
        );
        y += 1;
        let line: String = "─".repeat(inner.width as usize);
        put(
            buf,
            inner.x as i32,
            y as i32,
            &line,
            Style::new().fg(theme::DIM).bg(bg),
        );
        y += 1;
    }
    let desc_r = Rect {
        x: inner.x,
        y,
        width: inner.width,
        height: inner.bottom().saturating_sub(y),
    };
    let txt_col = if playable { theme::BONE } else { theme::ASH };
    Paragraph::new(d.desc(strength, weak))
        .style(Style::new().fg(txt_col).bg(bg))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .render(desc_r, buf);
}

fn hp_line(fi: &Fighter, max_w: i32, is_player: bool) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if fi.block > 0 {
        spans.push(Span::styled(
            format!("◆{} ", fi.block),
            Style::new().fg(theme::STEEL).add_modifier(Modifier::BOLD),
        ));
    }
    let txt = format!(" {}/{}", fi.hp.max(0), fi.max_hp);
    let used = spans.iter().map(|s| width(&s.content)).sum::<i32>() + width(&txt);
    let bar_w = (max_w - used).clamp(4, 18);
    let ratio = fi.hp.max(0) as f32 / fi.max_hp.max(1) as f32;
    let hp_col = if is_player {
        theme::BLOOD_LIGHT
    } else {
        mix(theme::BLOOD, theme::BLOOD_LIGHT, ratio)
    };
    let doom = fi.st.get(St::Doom);
    let poison = fi.st.get(St::Poison);
    let mut bar = Vec::new();
    for i in 0..bar_w {
        let v = (i as f32 + 0.5) / bar_w as f32 * fi.max_hp as f32;
        let (ch, c) = if v <= fi.hp as f32 {
            if doom > 0 && v <= doom as f32 {
                ("█", theme::VIOLET)
            } else if poison > 0 && v > (fi.hp - poison) as f32 {
                ("█", theme::POISON)
            } else {
                ("█", hp_col)
            }
        } else {
            ("░", theme::DIM)
        };
        bar.push(Span::styled(ch, fg(c)));
    }
    spans.extend(bar);
    let doomed = doom > 0 && doom >= fi.hp && fi.hp > 0;
    spans.push(Span::styled(
        txt,
        fg(if doomed { theme::VIOLET } else { theme::BONE }),
    ));
    if doomed {
        spans.push(Span::styled(
            " ☠",
            Style::new().fg(theme::VIOLET).add_modifier(Modifier::BOLD),
        ));
    }
    spans
}

fn status_spans(fi: &Fighter) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    for (s, v) in fi.st.active() {
        if !out.is_empty() {
            out.push(Span::raw(" "));
        }
        out.push(Span::styled(format!("{}{}", s.short(), v), fg(s.color())));
    }
    out
}

fn line_width(spans: &[Span]) -> i32 {
    spans.iter().map(|s| width(&s.content)).sum()
}

fn put_spans(buf: &mut Buffer, r: Rect, y: i32, spans: &[Span]) {
    let mut x = r.x as i32 + ((r.width as i32 - line_width(spans)) / 2).max(0);
    for s in spans {
        for ch in s.content.chars() {
            if x >= r.right() as i32 {
                return;
            }
            put(buf, x, y, &ch.to_string(), s.style);
            x += 1;
        }
    }
}

fn intent_label(c: &Combat, i: usize) -> (String, Color) {
    let e = &c.enemies[i];
    let me = Who::Enemy(i);
    let dmg = |n: i32| c.attack_value(me, Who::Player, n);
    match e.intent {
        Intent::Attack(n) => (format!("» ATTACK {}", dmg(n)), theme::BLOOD_LIGHT),
        Intent::Multi(n, k) => (format!("» ATTACK {}x{}", dmg(n), k), theme::BLOOD_LIGHT),
        Intent::Defend(_) => ("◆ DEFEND".into(), theme::STEEL),
        Intent::AttackDefend(n, _) => (format!("» {} + ◆", dmg(n)), theme::BLOOD_LIGHT),
        Intent::Buff(_) => ("▲ EMPOWER".into(), theme::EMBER),
        Intent::BuffDefend(_, _) => ("▲ EMPOWER + ◆".into(), theme::EMBER),
        Intent::Debuff(s, _) => (format!("✶ HEX: {}", s.short()), theme::VIOLET),
        Intent::AttackDebuff(n, s, _) => {
            (format!("» {} + {}", dmg(n), s.short()), theme::BLOOD_LIGHT)
        }
        Intent::Curse(_) => ("☠ CURSE".into(), theme::ASH),
    }
}

// ---------------------------------------------------------------------- title

fn render_title(app: &App, f: &mut Frame) {
    let area = f.area();
    let t = app.fx.t;
    let buf = f.buffer_mut();
    embers(buf, area, t, 90, theme::EMBER);

    let sx: i32 = if area.width >= 110 { 2 } else { 1 };
    let word_w = |w: &str| w.len() as i32 * 5 * sx + (w.len() as i32 - 1) * sx;
    let top = area.y as i32 + (area.height as i32 / 2 - 12).max(1);

    let draw_word = |buf: &mut Buffer, word: &str, y0: i32| {
        let x0 = area.x as i32 + (area.width as i32 - word_w(word)) / 2;
        for (gi, ch) in word.chars().enumerate() {
            let g = art::glyph(ch);
            let gx = x0 + gi as i32 * 6 * sx;
            for (row, line) in g.iter().enumerate() {
                for (col, c) in line.chars().enumerate() {
                    if c != '#' {
                        continue;
                    }
                    let x = gx + col as i32 * sx;
                    let y = y0 + row as i32;
                    let fl = flicker(t * 0.6, (gi * 7 + col) as f32);
                    let grad = row as f32 / 4.0;
                    let col_c = mix(
                        mix(theme::BLOOD_LIGHT, theme::DARK_BLOOD, grad),
                        theme::EMBER,
                        fl * 0.25,
                    );
                    for k in 0..sx {
                        put(
                            buf,
                            x + k + 1,
                            y + 1,
                            "▒",
                            fg(mix(theme::BG, theme::DARK_BLOOD, 0.45)),
                        );
                    }
                    for k in 0..sx {
                        put(buf, x + k, y, "█", fg(col_c));
                    }
                }
            }
            // Blood drips from the bottom row.
            let bottom = g[4];
            for (col, c) in bottom.chars().enumerate() {
                if c != '#' {
                    continue;
                }
                let x = gx + col as i32 * sx;
                let h = hash(x as u32, y0 as u32, 11);
                if !h.is_multiple_of(3) {
                    continue;
                }
                let period = 3.0 + (h % 50) as f32 / 10.0;
                let phase = ((t + (h % 100) as f32 / 10.0) / period).fract();
                let len = (phase * 6.0) as i32;
                for d in 1..=len.min(4) {
                    let ch = if d == len.min(4) { "•" } else { "│" };
                    put(
                        buf,
                        x,
                        y0 + 4 + d,
                        ch,
                        fg(mix(theme::BLOOD, theme::DARK_BLOOD, d as f32 / 5.0)),
                    );
                }
                if len > 4 {
                    put(buf, x, y0 + 4 + len, "·", fg(theme::DARK_BLOOD));
                }
            }
        }
    };
    draw_word(buf, "CRYPTS", top);
    let of_y = top + 7;
    let of = "─━━═══  † of the †  ═══━━─";
    put_center(
        buf,
        area,
        of_y,
        of,
        Style::new()
            .fg(mix(theme::ASH, theme::GOLD, flicker(t, 3.0) * 0.5))
            .add_modifier(Modifier::BOLD),
    );
    draw_word(buf, "UNHOLY", top + 9);

    // Torches flanking the logo.
    let tx = (area.width as i32 - word_w("CRYPTS")) / 2 - 8;
    if tx > 2 {
        torch(buf, area.x as i32 + tx, top + 6, t, 1.0);
        torch(buf, area.right() as i32 - tx - 2, top + 6, t, 4.0);
    }

    // Items share one fixed-width slot so the column reads as a centered block
    // instead of a staircase, and the selection markers don't jump around.
    let menu_y = top + 18;
    let slot_w = TITLE_ITEMS.iter().map(|s| width(s)).max().unwrap_or(0) + 8;
    let slot_x = area.x as i32 + (area.width as i32 - slot_w) / 2;
    let gap = if menu_y + TITLE_ITEMS.len() as i32 * 2 < area.bottom() as i32 - 1 {
        2
    } else {
        1
    };
    for (i, item) in TITLE_ITEMS.iter().enumerate() {
        let y = menu_y + i as i32 * gap;
        let disabled = i == 1 && !app.has_save;
        let sel = app.sel == i;
        let st = if disabled {
            fg(theme::DIM)
        } else if sel {
            Style::new()
                .fg(mix(theme::GOLD, theme::BONE, flicker(t, 9.0) * 0.4))
                .add_modifier(Modifier::BOLD)
        } else {
            fg(theme::ASH)
        };
        put(buf, slot_x + (slot_w - width(item)) / 2, y, item, st);
        if sel {
            put(buf, slot_x, y, "»", st);
            put(buf, slot_x + slot_w - 1, y, "«", st);
        }
        app.hit(
            Rect {
                x: slot_x.max(0) as u16,
                y: y.max(0) as u16,
                width: slot_w as u16,
                height: 1,
            },
            Hit::Item(i),
        );
    }
    let foot = Rect {
        y: area.bottom() - 1,
        height: 1,
        ..area
    };
    footer(
        f,
        foot,
        "↑↓ choose · Enter confirm · mouse supported · a run lasts about ten minutes",
    );
}

// ---------------------------------------------------------------------- class select

fn class_art(c: Class) -> &'static [&'static str] {
    match c {
        Class::BloodKnight => art::BLOOD_KNIGHT,
        Class::Assassin => art::ASSASSIN,
        Class::Warlock => art::WARLOCK,
    }
}

fn render_class_select(app: &App, f: &mut Frame) {
    let area = f.area();
    let t = app.fx.t;
    embers(f.buffer_mut(), area, t, 40, theme::DARK_BLOOD);
    let [head, body, foot] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(10),
        Constraint::Length(1),
    ])
    .areas(area);
    put_center(
        f.buffer_mut(),
        head,
        head.y as i32 + 1,
        "✝  CHOOSE YOUR PILGRIM  ✝",
        Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
    );
    let body = Rect {
        x: body.x + 1,
        width: body.width.saturating_sub(2),
        ..body
    };
    let cols = Layout::horizontal([Constraint::Ratio(1, 3); 3])
        .spacing(1)
        .split(body);
    for (i, &class) in Class::ALL.iter().enumerate() {
        let info = class.info();
        let sel = app.sel == i;
        let r = cols[i];
        app.hit(r, Hit::Item(i));
        let pulse = 0.5 + 0.5 * (t * 3.0).sin();
        let bc = if sel {
            mix(info.color, theme::BONE, pulse * 0.3)
        } else {
            theme::DIM
        };
        let inner = panel(f, r, "", bc, sel);
        let buf = f.buffer_mut();
        let a = class_art(class);
        let aw = art_width(a);
        let ax = inner.x as i32 + (inner.width as i32 - aw) / 2;
        let bob = if sel && (t * 2.0).sin() > 0.0 { 1 } else { 0 };
        let ay = inner.y as i32 + 1 - bob;
        draw_art(buf, ax, ay, a, inner, |_, _, ch| {
            let base = if sel {
                info.color
            } else {
                scale(info.color, 0.45)
            };
            let c = if "@o*".contains(ch) && sel {
                mix(theme::GOLD, theme::BONE, pulse)
            } else {
                base
            };
            Some(fg(c))
        });
        let mut y = ay + a.len() as i32 + 1;
        put_center(
            buf,
            inner,
            y,
            &info.name.to_uppercase(),
            Style::new()
                .fg(if sel { info.color } else { theme::ASH })
                .add_modifier(Modifier::BOLD),
        );
        y += 1;
        put_center(
            buf,
            inner,
            y,
            info.epithet,
            Style::new().fg(theme::ASH).add_modifier(Modifier::ITALIC),
        );
        y += 2;
        let wide = inner.width >= 34;
        let stats = vec![
            Span::styled("♥ ", fg(theme::BLOOD_LIGHT)),
            Span::styled(format!("{} HP", info.hp), fg(theme::BONE)),
            Span::raw(if wide { "   " } else { "  " }),
            Span::styled("◉ ", fg(theme::GOLD)),
            Span::styled(
                if wide {
                    format!("{} Energy", info.energy)
                } else {
                    format!("{} En", info.energy)
                },
                fg(theme::BONE),
            ),
            Span::raw(if wide { "   " } else { "  " }),
            Span::styled("✦ ", fg(theme::STEEL)),
            Span::styled(format!("{} Draw", info.draw), fg(theme::BONE)),
        ];
        put_spans(buf, inner, y, &stats);
        y += 2;
        let rest = Rect {
            x: inner.x + 1,
            y: y as u16,
            width: inner.width.saturating_sub(2),
            height: inner.bottom().saturating_sub(y as u16),
        };
        let text = vec![
            Line::from(Span::styled(
                info.blurb,
                fg(if sel { theme::BONE } else { theme::ASH }),
            )),
            Line::raw(""),
            Line::from(vec![Span::styled(
                format!("{}: ", info.passive_name),
                Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
            )]),
            Line::from(Span::styled(
                info.passive,
                fg(if sel { theme::BONE } else { theme::ASH }),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Starting deck:",
                Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
            )),
        ];
        let mut text = text;
        let mut counts: Vec<(String, usize)> = Vec::new();
        for c in crate::cards::starter_deck(class) {
            match counts.iter_mut().find(|e| e.0 == c.name()) {
                Some(e) => e.1 += 1,
                None => counts.push((c.name(), 1)),
            }
        }
        let deck: Vec<String> = counts.iter().map(|(n, k)| format!("{k}x {n}")).collect();
        text.push(Line::from(Span::styled(
            deck.join(" · "),
            fg(if sel { theme::BONE } else { theme::ASH }),
        )));
        Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Center)
            .render(rest, buf);
    }
    footer(f, foot, "←→ choose · Enter begin the descent · Esc back");
}

// ---------------------------------------------------------------------- map

fn render_map(app: &App, f: &mut Frame) {
    let area = f.area();
    let t = app.fx.t;
    let run = app.run.as_ref().unwrap();
    let [top, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(10),
        Constraint::Length(1),
    ])
    .areas(area);
    status_bar(app, f, top, None);
    let show_info = area.width >= 120;
    let [track, center, info] = Layout::horizontal([
        Constraint::Length(26),
        Constraint::Min(40),
        Constraint::Length(if show_info { 32 } else { 0 }),
    ])
    .areas(body);

    // Descent track
    let inner = panel(f, track, "The Descent", theme::DIM, false);
    let step = if inner.height as usize >= FLOORS * 2 {
        2
    } else {
        1
    };
    let buf = f.buffer_mut();
    for fl in 0..FLOORS {
        let y = inner.y as i32 + (fl * step) as i32;
        let rooms = &run.map[fl];
        let label = format!("{:>2} ", fl + 1);
        if fl < run.floor {
            let k = run.path.get(fl).copied().unwrap_or(0);
            let room = rooms[k.min(rooms.len() - 1)];
            put(buf, inner.x as i32, y, &label, fg(theme::DIM));
            put(buf, inner.x as i32 + 3, y, "† ", fg(theme::DARK_BLOOD));
            put(
                buf,
                inner.x as i32 + 5,
                y,
                room.name(),
                fg(scale(room.color(), 0.5)),
            );
        } else if fl == run.floor {
            let arrow = if (t * 3.0).sin() > 0.0 {
                "► "
            } else {
                "▸ "
            };
            put(
                buf,
                inner.x as i32,
                y,
                &label,
                Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
            );
            put(buf, inner.x as i32 + 3, y, arrow, fg(theme::GOLD));
            let mut x = inner.x as i32 + 5;
            for (i, room) in rooms.iter().enumerate() {
                let st = if i == app.sel {
                    Style::new().fg(room.color()).add_modifier(Modifier::BOLD)
                } else {
                    fg(scale(room.color(), 0.6))
                };
                let s = if i + 1 < rooms.len() {
                    format!("{} ", room.name())
                } else {
                    room.name().to_string()
                };
                put(buf, x, y, &s, st);
                x += width(&s);
            }
        } else {
            put(buf, inner.x as i32, y, &label, fg(theme::DIM));
            let names: Vec<&str> = rooms.iter().map(|r| r.name()).collect();
            let s = truncate(&names.join(" · "), inner.width as usize - 5);
            let c = if rooms.contains(&crate::run::RoomKind::Boss) {
                theme::DARK_BLOOD
            } else {
                theme::DIM
            };
            put(buf, inner.x as i32 + 5, y, &s, fg(c));
        }
        if step == 2 && fl + 1 < FLOORS {
            put(buf, inner.x as i32 + 4, y + 1, "┊", fg(theme::DIM));
        }
    }

    // Doors
    let rooms = &run.map[run.floor];
    let n = rooms.len() as i32;
    let door_w = 18;
    let gap = 7;
    let total = n * door_w + (n - 1) * gap;
    let x0 = center.x as i32 + (center.width as i32 - total) / 2;
    let door_y = center.y as i32 + (center.height as i32 / 2 - 8).max(3);
    let title = if run.floor + 1 == FLOORS {
        "The final door. Something vast breathes beyond it.".to_string()
    } else {
        format!("Floor {} — choose your path", run.floor + 1)
    };
    put_center(
        buf,
        center,
        door_y - 3,
        &title,
        Style::new().fg(theme::BONE).add_modifier(Modifier::BOLD),
    );
    embers(buf, center, t, 25, theme::DARK_BLOOD);
    for (i, room) in rooms.iter().enumerate() {
        let sel = app.sel == i;
        let dx = x0 + i as i32 * (door_w + gap);
        let glow = flicker(t, i as f32 * 3.3);
        let stone = if sel {
            mix(theme::ASH, room.color(), 0.35 + glow * 0.25)
        } else {
            theme::DIM
        };
        // Interior shimmer
        for yy in 3..8 {
            for xx in 3..15 {
                let h = hash(xx as u32, yy as u32, (t * 6.0) as u32 + i as u32 * 31);
                let ch = if sel {
                    match h % 9 {
                        0 => "░",
                        1 => "·",
                        2 => ".",
                        _ => " ",
                    }
                } else if h.is_multiple_of(13) {
                    "·"
                } else {
                    " "
                };
                let c = if sel {
                    mix(theme::BG, room.color(), 0.25 + 0.35 * glow)
                } else {
                    theme::DIM
                };
                put(
                    buf,
                    dx + xx,
                    door_y + yy,
                    ch,
                    Style::new().fg(c).bg(if sel {
                        mix(theme::BG, room.color(), 0.08)
                    } else {
                        theme::BG
                    }),
                );
            }
        }
        draw_art(buf, dx, door_y, art::DOOR, buf.area, |_, _, _| {
            Some(fg(stone))
        });
        let icon = room.icon();
        let ic = if sel {
            mix(room.color(), theme::BONE, glow * 0.3)
        } else {
            scale(room.color(), 0.55)
        };
        for (k, line) in icon.iter().enumerate() {
            put(
                buf,
                dx + 6,
                door_y + 3 + k as i32 + 1,
                line,
                Style::new().fg(ic).add_modifier(Modifier::BOLD),
            );
        }
        let label = if sel {
            format!("» {} «", room.name())
        } else {
            room.name().to_string()
        };
        let lr = Rect {
            x: dx.max(0) as u16,
            y: 0,
            width: door_w as u16,
            height: 1,
        };
        put_center(
            buf,
            lr,
            door_y + 10,
            &label,
            Style::new()
                .fg(if sel { room.color() } else { theme::ASH })
                .add_modifier(Modifier::BOLD),
        );
        put_center(
            buf,
            lr,
            door_y + 11,
            &format!("[{}]", i + 1),
            fg(theme::DIM),
        );
        app.hit(
            Rect {
                x: dx.max(0) as u16,
                y: door_y.max(0) as u16,
                width: door_w as u16,
                height: 12,
            },
            Hit::Item(i),
        );
        if i + 1 < rooms.len() {
            torch(
                buf,
                dx + door_w + gap / 2 - 1,
                door_y + 2,
                t,
                i as f32 * 2.7,
            );
        }
    }
    torch(buf, x0 - 5, door_y + 2, t, 9.1);
    torch(buf, x0 + total + 3, door_y + 2, t, 5.3);

    if let Some(room) = rooms.get(app.sel) {
        let dr = Rect {
            x: center.x + 4,
            y: (door_y + 13).max(0) as u16,
            width: center.width.saturating_sub(8),
            height: 3,
        };
        paragraph(f, dr, room.desc(), fg(theme::ASH), Alignment::Center);
    }

    if show_info {
        render_run_info(f, info, run);
    }
    footer(
        f,
        foot,
        "←→ / 1-3 choose door · Enter descend · D deck · ? codex · Esc menu",
    );
}

fn render_run_info(f: &mut Frame, r: Rect, run: &RunState) {
    let info = run.class.info();
    let inner = panel(f, r, "Pilgrim", theme::DIM, false);
    let mut lines = vec![
        Line::from(Span::styled(
            info.name,
            Style::new().fg(info.color).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            info.epithet,
            Style::new().fg(theme::ASH).add_modifier(Modifier::ITALIC),
        )),
        Line::raw(""),
        Line::from(vec![
            Span::styled(format!("{}: ", info.passive_name), fg(theme::GOLD)),
            Span::styled(info.passive, fg(theme::ASH)),
        ]),
        Line::raw(""),
        Line::from(Span::styled(
            "Relics",
            Style::new().fg(theme::BONE).add_modifier(Modifier::BOLD),
        )),
    ];
    if run.relics.is_empty() {
        lines.push(Line::from(Span::styled("None yet.", fg(theme::DIM))));
    }
    for rel in &run.relics {
        lines.push(Line::from(Span::styled(
            format!("◆ {}", rel.name()),
            fg(theme::GOLD),
        )));
        lines.push(Line::from(Span::styled(
            format!("  {}", rel.desc()),
            fg(theme::ASH),
        )));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "Progress is saved before each room.",
        fg(theme::DIM),
    )));
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(inner, f.buffer_mut());
}

// ---------------------------------------------------------------------- combat

struct Anchor {
    who: Who,
    art: Rect,
}

fn render_combat(app: &App, f: &mut Frame, cv: &CombatView) {
    let area = f.area();
    let t = app.fx.t;
    let c = &cv.c;
    let hand_h: u16 = if area.height >= 42 {
        13
    } else if area.height >= 36 {
        12
    } else {
        10
    };
    // The tutorial guide takes the place of the two-line combat log.
    let log_h = if app.tutorial.is_some() { 5 } else { 2 };
    let [top, field, log, hand, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(10),
        Constraint::Length(log_h),
        Constraint::Length(hand_h),
        Constraint::Length(1),
    ])
    .areas(area);
    status_bar(app, f, top, Some(c));

    // Battlefield with optional screen shake.
    let sa = t - app.fx.shake;
    let shake: i32 = if sa < 0.35 {
        [2, -2, 1, -1, 1, 0][((sa * 30.0) as usize) % 6]
    } else {
        0
    };
    let field = Rect {
        x: (field.x as i32 + shake).clamp(0, 2) as u16,
        ..field
    }
    .intersection(area);
    let room_name = match cv.room {
        crate::run::RoomKind::Boss => "The Throne of Bones",
        crate::run::RoomKind::Elite => "A Desecrated Sanctum",
        _ => "The Crypt",
    };
    let bc = mix(theme::DARK_BLOOD, theme::BLOOD, flicker(t * 0.5, 1.0) * 0.5);
    let inner = panel(
        f,
        field,
        &format!("✝ Floor {} — {} ✝", c.floor + 1, room_name),
        bc,
        false,
    );
    let buf = f.buffer_mut();
    embers(buf, inner, t, 30, theme::EMBER);
    // Ground
    for x in inner.left()..inner.right() {
        let ch = match hash(x as u32, 0, 5) % 7 {
            0 => "▁",
            1 => "_",
            2 => ".",
            _ => " ",
        };
        put(
            buf,
            x as i32,
            inner.bottom() as i32 - 1,
            ch,
            fg(mix(theme::BG, theme::DIM, 0.8)),
        );
    }

    let pw = (inner.width / 4).max(26).min(inner.width / 2);
    let player_col = Rect { width: pw, ..inner };
    let enemy_area = Rect {
        x: inner.x + pw,
        width: inner.width - pw,
        ..inner
    };
    let mut anchors: Vec<Anchor> = Vec::new();

    let info = c.class.info();
    let pr = draw_entity(
        app,
        f,
        player_col,
        Who::Player,
        class_art(c.class),
        info.color,
        info.name,
        &c.p,
        None,
        false,
        None,
    );
    anchors.push(Anchor {
        who: Who::Player,
        art: pr,
    });

    let n = c.enemies.len().max(1) as u16;
    let ew = enemy_area.width / n;
    for (i, e) in c.enemies.iter().enumerate() {
        let col = Rect {
            x: enemy_area.x + ew * i as u16,
            width: ew,
            ..enemy_area
        };
        let intent = if e.dead {
            None
        } else {
            Some(intent_label(c, i))
        };
        let targeted = cv.target == i
            && !e.dead
            && cv.phase == Phase::Player
            && c.enemies.iter().filter(|e| !e.dead).count() > 1;
        let r = draw_entity(
            app,
            f,
            col,
            Who::Enemy(i),
            e.kind.art(),
            e.kind.color(),
            e.kind.name(),
            &e.f,
            intent,
            targeted,
            if e.dead {
                Some(app.fx.death_age(i).unwrap_or(9.0))
            } else {
                None
            },
        );
        anchors.push(Anchor {
            who: Who::Enemy(i),
            art: r,
        });
        if !e.dead {
            app.hit(col, Hit::Enemy(i));
        }
    }

    let buf = f.buffer_mut();
    // Slashes
    for (who, t0, style) in &app.fx.slashes {
        let age = t - t0;
        let Some(a) = anchors.iter().find(|a| a.who == *who) else {
            continue;
        };
        draw_slash(buf, a.art, age, *style, *t0);
    }
    // Floating numbers
    for fl in &app.fx.floats {
        let age = t - fl.t0;
        if age < 0.0 {
            continue;
        }
        let Some(a) = anchors.iter().find(|a| a.who == fl.who) else {
            continue;
        };
        let y = a.art.y as i32 + 2 + fl.lane - (age * 5.0) as i32;
        let x = a.art.x as i32 + a.art.width as i32 / 2 - width(&fl.text) / 2 + (fl.lane - 1) * 3;
        let fade = ((age - 0.7) / 0.6).clamp(0.0, 1.0);
        let pop = if age < 0.1 { theme::BONE } else { fl.color };
        let st = Style::new()
            .fg(mix(pop, theme::BG, fade))
            .add_modifier(Modifier::BOLD);
        if y > inner.y as i32 {
            put(buf, x, y, &fl.text, st);
        }
    }
    // Played card ghost
    if let Some((card, t0)) = &app.fx.played {
        let age = t - t0;
        let d = card.def();
        let fade = (age / 0.9).clamp(0.0, 1.0);
        let pa = anchors[0].art;
        let y = pa.y as i32 - 1 - (age * 3.0) as i32;
        let s = format!("‹ {} ›", card.name());
        if y > inner.y as i32 {
            put_center(
                buf,
                player_col,
                y,
                &s,
                Style::new()
                    .fg(mix(d.kind.color(), theme::BG, fade))
                    .add_modifier(Modifier::BOLD),
            );
        }
    }
    // Banner
    if let Some((s, t0, col)) = &app.fx.banner {
        let age = t - t0;
        let a = (age / 0.25).min(1.0) * ((1.4 - age) / 0.4).clamp(0.0, 1.0);
        let y = inner.y as i32 + inner.height as i32 / 3;
        let spaced: String = s.chars().flat_map(|ch| [ch, ' ']).collect();
        let text = format!("  ✦  {}  ✦  ", spaced.trim_end());
        let bw = (width(&text) + 16).min(inner.width as i32);
        let bx = inner.x as i32 + (inner.width as i32 - bw) / 2;
        let band: String = "━".repeat(bw as usize);
        put(buf, bx, y - 1, &band, fg(mix(theme::BG, *col, a * 0.5)));
        put(buf, bx, y + 1, &band, fg(mix(theme::BG, *col, a * 0.5)));
        let blank: String = " ".repeat(bw as usize);
        put(buf, bx, y, &blank, Style::new().bg(theme::BG));
        put_center(
            buf,
            inner,
            y,
            &text,
            Style::new()
                .fg(mix(theme::BG, *col, a))
                .add_modifier(Modifier::BOLD),
        );
    }
    match cv.phase {
        Phase::Won => put_center(
            buf,
            inner,
            inner.y as i32 + 1,
            "✦ VICTORY ✦",
            Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
        ),
        Phase::Lost => put_center(
            buf,
            inner,
            inner.y as i32 + 1,
            "YOUR LIGHT FADES...",
            Style::new()
                .fg(theme::BLOOD_LIGHT)
                .add_modifier(Modifier::BOLD),
        ),
        _ => {}
    }

    // Log
    if let Some(tu) = &app.tutorial {
        render_tutorial(app, f, log, tu, cv);
    } else if let Some((msg, _)) = &cv.msg {
        put(
            buf,
            log.x as i32 + 2,
            log.y as i32,
            &format!("✗ {msg}"),
            Style::new()
                .fg(theme::BLOOD_LIGHT)
                .add_modifier(Modifier::BOLD),
        );
    } else {
        let n = c.log.len();
        for (k, line) in c.log.iter().skip(n.saturating_sub(2)).enumerate() {
            let latest = k == 1 || n == 1;
            put(
                buf,
                log.x as i32 + 2,
                log.y as i32 + k as i32,
                &truncate(line, log.width as usize - 4),
                fg(if latest { theme::BONE } else { theme::ASH }),
            );
        }
    }

    render_hand(app, f, hand, cv);

    let target_name = c
        .enemies
        .get(cv.target)
        .filter(|e| !e.dead)
        .map(|e| e.kind.name())
        .unwrap_or("-");
    footer(
        f,
        foot,
        &format!(
            "Target: {target_name} · ←→ card · ↑↓ target · Enter/1-9 play · E end turn · D deck · ? codex · Esc menu"
        ),
    );
}

fn render_tutorial(
    app: &App,
    f: &mut Frame,
    r: Rect,
    tu: &crate::tutorial::Tutorial,
    cv: &CombatView,
) {
    use crate::tutorial::{Goal, STEPS};
    let t = app.fx.t;
    let step = tu.current();
    let fresh = ((t - tu.t0) / 0.4).clamp(0.0, 1.0);
    let bc = mix(theme::BONE, theme::GOLD, fresh);
    let title = format!("✝ Guide {}/{} — {} ✝", tu.step + 1, STEPS.len(), step.title);
    let inner = panel(
        f,
        Rect {
            x: r.x + 1,
            width: r.width.saturating_sub(2),
            ..r
        },
        &title,
        bc,
        true,
    );
    let text_r = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    };
    paragraph(
        f,
        text_r,
        step.text,
        fg(mix(theme::BG, theme::BONE, fresh)),
        Alignment::Left,
    );
    // Hint or rejection message, drawn into the bottom border.
    let (hint, st) = match &cv.msg {
        Some((msg, _)) => (
            format!(" ✗ {msg} "),
            Style::new()
                .fg(theme::BLOOD_LIGHT)
                .bg(theme::PANEL)
                .add_modifier(Modifier::BOLD),
        ),
        None => {
            let h = match step.goal {
                Goal::Read => "▸ Enter or click to continue",
                Goal::Done => "▸ Enter to return to the title",
                Goal::Watch => "· enemy turn ·",
                _ => "▸ your move",
            };
            let pulse = 0.5 + 0.5 * (t * 3.0).sin();
            (
                format!(" {h} "),
                Style::new()
                    .fg(mix(theme::GOLD, theme::BONE, pulse * 0.4))
                    .bg(theme::PANEL)
                    .add_modifier(Modifier::BOLD),
            )
        }
    };
    let bottom = inner.bottom() as i32;
    put(
        f.buffer_mut(),
        inner.right() as i32 - width(&hint) - 1,
        bottom,
        &hint,
        st,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_entity(
    app: &App,
    f: &mut Frame,
    col: Rect,
    who: Who,
    art: &[&str],
    color: Color,
    name: &str,
    fi: &Fighter,
    intent: Option<(String, Color)>,
    targeted: bool,
    dead_age: Option<f32>,
) -> Rect {
    let t = app.fx.t;
    let buf = f.buffer_mut();
    let bottom = col.bottom() as i32 - 2;
    let st_y = bottom;
    let hp_y = bottom - 1;
    let name_y = bottom - 2;
    let art_h = art.len() as i32;
    let aw = art_width(art);
    let seed = match who {
        Who::Player => 0.0,
        Who::Enemy(i) => 1.3 + i as f32 * 1.7,
    };
    let bob = if dead_age.is_none() && (t * 1.2 + seed).sin() > 0.4 {
        1
    } else {
        0
    };
    let rest_top = name_y - 1 - art_h;
    let art_top = rest_top + bob;
    let hit = app.fx.hit_age(who);
    let dx = match hit {
        Some(a) if a < 0.35 => [2, -2, 1, -1, 1, 0][((a * 28.0) as usize) % 6],
        _ => 0,
    };
    let flash = matches!(hit, Some(a) if a < 0.12);
    let x0 = col.x as i32 + (col.width as i32 - aw) / 2 + dx;
    let clip = Rect {
        y: col.y + 1,
        height: col.height.saturating_sub(1),
        ..col
    };
    let pulse = 0.5 + 0.5 * (t * 3.0 + seed).sin();
    let is_enemy = matches!(who, Who::Enemy(_));
    let wraith = name == "Wraith";
    draw_art(buf, x0, art_top, art, clip, |cx, cy, ch| {
        if let Some(age) = dead_age {
            let p = (age / 0.9).clamp(0.0, 1.0);
            let h = (hash(cx as u32, cy as u32, 7) % 1000) as f32 / 1000.0;
            if h < p {
                return None;
            }
            return Some(fg(mix(theme::BLOOD_LIGHT, theme::DARK_BLOOD, p)));
        }
        if flash {
            return Some(
                Style::new()
                    .fg(Color::Rgb(255, 240, 230))
                    .add_modifier(Modifier::BOLD),
            );
        }
        let mut c = color;
        if wraith {
            c = mix(
                color,
                theme::BG,
                flicker(t, cx as f32 * 0.3 + cy as f32) * 0.5,
            );
        }
        if is_enemy && "o@O".contains(ch) {
            c = mix(theme::BLOOD_LIGHT, theme::GOLD, pulse);
        }
        if !is_enemy && ch == '@' {
            c = mix(color, theme::GOLD, pulse);
        }
        Some(fg(c))
    });
    let art_rect = Rect {
        x: x0.max(0) as u16,
        y: art_top.max(col.y as i32) as u16,
        width: aw as u16,
        height: art_h as u16,
    };
    if let Some(age) = dead_age {
        if age > 0.6 {
            let a = ((age - 0.6) / 0.6).clamp(0.0, 1.0);
            put_center(buf, col, name_y - 2, "†", fg(mix(theme::BG, theme::ASH, a)));
            put_center(
                buf,
                col,
                name_y - 1,
                "_/‾\\_",
                fg(mix(theme::BG, theme::DIM, a)),
            );
        }
        put_center(
            buf,
            col,
            name_y,
            name,
            Style::new()
                .fg(theme::DIM)
                .add_modifier(Modifier::CROSSED_OUT),
        );
        return art_rect;
    }
    if let Some((label, ic)) = intent {
        let iy = (rest_top - 2).max(col.y as i32 + 1);
        let pulse_c = mix(ic, theme::BONE, pulse * 0.25);
        put_center(
            buf,
            col,
            iy,
            &label,
            Style::new().fg(pulse_c).add_modifier(Modifier::BOLD),
        );
    }
    if targeted {
        let my = (rest_top - 1).max(col.y as i32 + 2);
        let mk = if (t * 4.0).sin() > 0.0 { "▼" } else { "▽" };
        put_center(
            buf,
            col,
            my,
            mk,
            Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
        );
    }
    let name_st = if targeted {
        Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme::BONE).add_modifier(Modifier::BOLD)
    };
    let nm = if targeted {
        format!("» {name} «")
    } else {
        name.to_string()
    };
    put_center(buf, col, name_y, &nm, name_st);
    let hp = hp_line(fi, col.width as i32 - 2, who == Who::Player);
    put_spans(buf, col, hp_y, &hp);
    let sts = status_spans(fi);
    put_spans(buf, col, st_y, &sts);
    art_rect
}

fn draw_slash(buf: &mut Buffer, a: Rect, age: f32, style: SlashStyle, seed: f32) {
    if !(0.0..=0.35).contains(&age) || a.height == 0 {
        return;
    }
    let p = (age / 0.15).min(1.0);
    let fade = ((age - 0.15) / 0.2).clamp(0.0, 1.0);
    let h = a.height as i32;
    let w = a.width as i32;
    let rows = ((h as f32) * p) as i32;
    match style {
        SlashStyle::Heavy => {
            for k in 0..rows {
                let x = a.x as i32 + w - 1 - k * w / h.max(1);
                let c = mix(
                    mix(
                        Color::Rgb(255, 230, 220),
                        theme::BLOOD_LIGHT,
                        k as f32 / h as f32,
                    ),
                    theme::BG,
                    fade,
                );
                put(buf, x - 1, a.y as i32 + k, "▓▒", fg(c));
            }
        }
        SlashStyle::Twin => {
            for k in 0..rows {
                let c = mix(Color::Rgb(220, 255, 210), theme::BG, fade);
                put(
                    buf,
                    a.x as i32 + 1 + k * w / h.max(1),
                    a.y as i32 + k,
                    "\\",
                    fg(c),
                );
                put(
                    buf,
                    a.x as i32 + w - 2 - k * w / h.max(1),
                    a.y as i32 + k,
                    "/",
                    fg(c),
                );
            }
        }
        SlashStyle::Arcane => {
            let n = (w * h / 5).max(6);
            for k in 0..n {
                let hh = hash(k as u32, seed.to_bits(), 3);
                if (hh % 100) as f32 / 100.0 > p {
                    continue;
                }
                let x = a.x as i32 + (hh % w.max(1) as u32) as i32;
                let y = a.y as i32 + ((hh >> 8) % h.max(1) as u32) as i32;
                let ch = ["*", "+", "·", "✶"][(hh >> 16) as usize % 4];
                let c = mix(
                    mix(
                        theme::VIOLET,
                        Color::Rgb(240, 200, 255),
                        (hh % 7) as f32 / 7.0,
                    ),
                    theme::BG,
                    fade,
                );
                put(buf, x, y, ch, fg(c));
            }
        }
        SlashStyle::Claw => {
            for line in 0..3 {
                for k in 0..rows.min(h - 1) {
                    let x = a.x as i32 + w / 2 - 3 + line * 3 + (h / 2 - k) / 2;
                    let c = mix(theme::BLOOD_LIGHT, theme::BG, fade);
                    put(
                        buf,
                        x,
                        a.y as i32 + k,
                        "/",
                        fg(c).add_modifier(Modifier::BOLD),
                    );
                }
            }
        }
    }
}

fn render_hand(app: &App, f: &mut Frame, hand: Rect, cv: &CombatView) {
    let t = app.fx.t;
    let c = &cv.c;
    let side_w = 24u16;
    let cards_area = Rect {
        width: hand.width.saturating_sub(side_w + 1),
        ..hand
    };
    let side = Rect {
        x: hand.right().saturating_sub(side_w),
        width: side_w,
        ..hand
    };
    let n = c.hand.len() as u16;
    let player_turn = cv.phase == Phase::Player;
    #[allow(clippy::manual_checked_ops)] // the empty-hand branch draws a message, not just a guard
    if n == 0 {
        put_center(
            f.buffer_mut(),
            cards_area,
            cards_area.y as i32 + cards_area.height as i32 / 2,
            "Your hand is empty.",
            fg(theme::DIM),
        );
    } else {
        let card_w = (cards_area.width.saturating_sub(2) / n).clamp(11, 22);
        let total = card_w * n;
        let x0 = cards_area.x + cards_area.width.saturating_sub(total) / 2;
        let strength = c.p.st.get(St::Strength);
        let weak = c.p.st.get(St::Weak) > 0;
        let draw_age = t - cv.turn_t0;
        for (i, card) in c.hand.iter().enumerate() {
            let sel = i == app.sel && player_turn;
            let p = ((draw_age - i as f32 * 0.06) / 0.3).clamp(0.0, 1.0);
            let slide = ((1.0 - p) * (hand.height as f32 + 1.0)) as u16;
            let y = hand.y + if sel { 0 } else { 1 } + slide;
            let r = Rect {
                x: x0 + card_w * i as u16,
                y,
                width: card_w,
                height: hand.height - 1,
            }
            .intersection(hand);
            if r.height < 3 {
                continue;
            }
            let playable = player_turn && c.unplayable_reason(i).is_none();
            draw_card(f, r, card, sel, playable, strength, weak, t);
            if i < 9 {
                put(
                    f.buffer_mut(),
                    r.right() as i32 - 5,
                    r.bottom() as i32 - 1,
                    &format!("[{}]", i + 1),
                    fg(theme::DIM),
                );
            }
            app.hit(
                Rect {
                    x: x0 + card_w * i as u16,
                    y: hand.y,
                    width: card_w,
                    height: hand.height,
                },
                Hit::Card(i),
            );
        }
    }

    // Energy, piles and end turn.
    let has_play = (0..c.hand.len()).any(|i| c.unplayable_reason(i).is_none());
    let ec = if c.energy > 0 {
        mix(theme::GOLD, theme::BONE, flicker(t, 2.0) * 0.3)
    } else {
        theme::DIM
    };
    let er = Rect { height: 3, ..side };
    let inner = panel(f, er, "Energy", ec, false);
    let mut orbs = String::new();
    for k in 0..c.max_energy.max(c.energy) {
        orbs.push_str(if k < c.energy { "◉ " } else { "○ " });
    }
    put_center(
        f.buffer_mut(),
        inner,
        inner.y as i32,
        &format!("{}  {}/{}", orbs.trim_end(), c.energy, c.max_energy),
        Style::new().fg(ec).add_modifier(Modifier::BOLD),
    );
    let py = side.y as i32 + 4;
    let buf = f.buffer_mut();
    put(
        buf,
        side.x as i32 + 1,
        py,
        &format!("Draw pile   {:>3}", c.draw.len()),
        fg(theme::ASH),
    );
    put(
        buf,
        side.x as i32 + 1,
        py + 1,
        &format!("Discard     {:>3}", c.discard.len()),
        fg(theme::ASH),
    );
    put(
        buf,
        side.x as i32 + 1,
        py + 2,
        &format!("Exhausted   {:>3}", c.exhausted.len()),
        fg(theme::DIM),
    );
    let br = Rect {
        x: side.x,
        y: side.bottom().saturating_sub(3),
        width: side.width,
        height: 3,
    };
    if player_turn {
        let urge = !has_play;
        let col = if urge {
            mix(theme::BLOOD, theme::GOLD, 0.5 + 0.5 * (t * 4.0).sin())
        } else {
            theme::BLOOD_LIGHT
        };
        button(f, br, "END TURN  [E]", col, urge);
        app.hit(br, Hit::EndTurn);
    } else {
        button(f, br, "· enemy turn ·", theme::DIM, false);
    }
}

// ---------------------------------------------------------------------- reward

fn render_reward(app: &App, f: &mut Frame, rv: &RewardView) {
    let area = f.area();
    let t = app.fx.t;
    let [top, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(10),
        Constraint::Length(1),
    ])
    .areas(area);
    status_bar(app, f, top, None);
    let buf = f.buffer_mut();
    embers(buf, body, t, 40, theme::GOLD);
    let mut y = body.y as i32 + 2;
    put_center(
        buf,
        body,
        y,
        "✦  S P O I L S   O F   T H E   D E A D  ✦",
        Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
    );
    y += 2;
    put_center(
        buf,
        body,
        y,
        &format!("● +{} gold", rv.gold),
        fg(theme::GOLD),
    );
    y += 1;
    if rv.healed > 0 {
        put_center(
            buf,
            body,
            y,
            &format!("♥ The Cracked Chalice restores {} HP", rv.healed),
            fg(theme::HEAL),
        );
        y += 1;
    }
    if let Some(r) = rv.relic {
        put_center(
            buf,
            body,
            y,
            &format!("◆ Relic claimed: {} — {}", r.name(), r.desc()),
            Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
        );
        y += 1;
    }
    y += 1;
    put_center(
        buf,
        body,
        y,
        "Choose a card to add to your deck",
        fg(theme::BONE),
    );
    y += 2;
    let n = rv.cards.len() as i32;
    let cw = 26i32;
    let ch = 13i32;
    let gap = 4;
    let total = n * cw + (n - 1) * gap;
    let x0 = body.x as i32 + (body.width as i32 - total) / 2;
    let class = app.run.as_ref().unwrap().class;
    let _ = class;
    for (i, card) in rv.cards.iter().enumerate() {
        let sel = app.sel == i;
        let appear = ((app.screen_age() - i as f32 * 0.12) / 0.35).clamp(0.0, 1.0);
        let yy = y + if sel { 0 } else { 1 } + ((1.0 - appear) * 6.0) as i32;
        let r = Rect {
            x: (x0 + i as i32 * (cw + gap)).max(0) as u16,
            y: yy.max(0) as u16,
            width: cw as u16,
            height: ch as u16,
        };
        if appear > 0.0 {
            draw_card(f, r, card, sel, true, 0, false, t);
        }
        app.hit(r, Hit::Card(i));
    }
    let skip_sel = app.sel == rv.cards.len();
    let br = Rect {
        x: area.x + (area.width - 20) / 2,
        y: (y + ch + 2).max(0) as u16,
        width: 20,
        height: 3,
    };
    button(f, br, "Skip", theme::ASH, skip_sel);
    app.hit(br, Hit::Item(rv.cards.len()));
    footer(f, foot, "←→ choose · Enter take · Esc skip · D deck");
}

// ---------------------------------------------------------------------- choice rooms

fn render_choice(app: &App, f: &mut Frame, cv: &ChoiceView) {
    let area = f.area();
    let t = app.fx.t;
    let [top, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(10),
        Constraint::Length(1),
    ])
    .areas(area);
    status_bar(app, f, top, None);
    embers(f.buffer_mut(), body, t, 30, cv.art_color);
    let has_card = cv.opts.iter().any(|o| o.card.is_some());
    let pw = if has_card {
        110.min(body.width - 2)
    } else {
        90.min(body.width - 2)
    };
    let content =
        cv.art.len() as u16 + 9 + (cv.opts.len() as u16).max(if has_card { 12 } else { 0 });
    let pr = centered(body, pw, content.min(body.height.saturating_sub(2)));
    let inner = panel(f, pr, &cv.title, scale(cv.art_color, 0.7), true);
    let buf = f.buffer_mut();
    let aw = art_width(cv.art);
    let ax = inner.x as i32 + (inner.width as i32 - aw) / 2;
    let ay = inner.y as i32 + 1;
    let ac = cv.art_color;
    draw_art(buf, ax, ay, cv.art, inner, |cx, cy, ch| {
        let fl = flicker(t, cx as f32 * 0.4 + cy as f32);
        let c = if "()~$+@".contains(ch) {
            mix(ac, theme::GOLD, fl * 0.6)
        } else {
            scale(ac, 0.75 + fl * 0.15)
        };
        Some(fg(c))
    });
    let mut y = ay + cv.art.len() as i32 + 1;
    // Typewriter text
    let shown = ((t - cv.t0) * 90.0).max(0.0) as usize;
    let text: String = cv.text.chars().take(shown).collect();
    let tr = Rect {
        x: inner.x + 3,
        y: y as u16,
        width: inner.width.saturating_sub(6),
        height: 5,
    };
    paragraph(
        f,
        tr,
        &text,
        Style::new().fg(theme::BONE).add_modifier(Modifier::ITALIC),
        Alignment::Center,
    );
    y += 6;
    let list_w = if has_card {
        inner.width.saturating_sub(34)
    } else {
        inner.width.saturating_sub(4)
    };
    let buf = f.buffer_mut();
    for (i, o) in cv.opts.iter().enumerate() {
        let sel = app.sel == i;
        let yy = y + i as i32;
        let prefix = if sel { "» " } else { "  " };
        let st = if !o.enabled {
            fg(theme::DIM)
        } else if sel {
            Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD)
        } else {
            fg(theme::BONE)
        };
        let label = truncate(&format!("{prefix}{}. {}", i + 1, o.label), list_w as usize);
        if sel {
            let bar: String = " ".repeat(list_w as usize);
            put(
                buf,
                inner.x as i32 + 2,
                yy,
                &bar,
                Style::new().bg(theme::PANEL_HI),
            );
        }
        put(
            buf,
            inner.x as i32 + 2,
            yy,
            &label,
            if sel { st.bg(theme::PANEL_HI) } else { st },
        );
        app.hit(
            Rect {
                x: inner.x + 2,
                y: yy.max(0) as u16,
                width: list_w,
                height: 1,
            },
            Hit::Item(i),
        );
    }
    if has_card && let Some(card) = cv.opts.get(app.sel).and_then(|o| o.card) {
        let top = (y - 1).max(0) as u16;
        let r = Rect {
            x: inner.right().saturating_sub(30),
            y: top,
            width: 26,
            height: 12.min(inner.bottom().saturating_sub(top)),
        };
        draw_card(f, r, &card, true, true, 0, false, t);
    }
    footer(f, foot, "↑↓ choose · Enter confirm · D deck · ? codex");
}

// ---------------------------------------------------------------------- deck pick

fn render_deckpick(app: &App, f: &mut Frame, dp: &DeckPick) {
    let area = f.area();
    let t = app.fx.t;
    let run = app.run.as_ref().unwrap();
    let [top, head, grid, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(4),
        Constraint::Min(10),
        Constraint::Length(1),
    ])
    .areas(area);
    status_bar(app, f, top, None);
    let (title, col) = match dp.purpose {
        Purpose::Upgrade => ("✦  CHOOSE A CARD TO UPGRADE  ✦", theme::HEAL),
        Purpose::Remove => ("✝  CHOOSE A CARD TO BURY FOREVER  ✝", theme::BLOOD_LIGHT),
    };
    let buf = f.buffer_mut();
    put_center(
        buf,
        head,
        head.y as i32 + 1,
        title,
        Style::new().fg(col).add_modifier(Modifier::BOLD),
    );
    let elig = dp.eligible(&run.deck);
    if let Some(&idx) = elig.get(app.sel) {
        let card = run.deck[idx];
        let s = match dp.purpose {
            Purpose::Upgrade => {
                let up = Card { up: true, ..card };
                format!(
                    "{} → {}: {}",
                    card.name(),
                    up.name(),
                    up.def().desc(0, false)
                )
            }
            Purpose::Remove if dp.cost > 0 => format!("Bury {} for {} gold.", card.name(), dp.cost),
            Purpose::Remove => format!("Bury {}.", card.name()),
        };
        put_center(
            buf,
            head,
            head.y as i32 + 2,
            &truncate(&s, head.width as usize - 4),
            fg(theme::ASH),
        );
    }
    let cw = 22u16;
    let ch = 11u16;
    let cols = ((grid.width - 2) / (cw + 1)).max(1) as usize;
    app.deck_cols.set(cols);
    let rows_vis = (grid.height / ch).max(1) as usize;
    let sel_row = app.sel / cols;
    let first_row = sel_row.saturating_sub(rows_vis - 1);
    let gx = grid.x + (grid.width - (cols as u16 * (cw + 1))) / 2;
    for (k, &idx) in elig.iter().enumerate() {
        let row = k / cols;
        if row < first_row || row >= first_row + rows_vis {
            continue;
        }
        let r = Rect {
            x: gx + (k % cols) as u16 * (cw + 1),
            y: grid.y + (row - first_row) as u16 * ch,
            width: cw,
            height: ch,
        };
        draw_card(f, r, &run.deck[idx], k == app.sel, true, 0, false, t);
        app.hit(r, Hit::Card(k));
    }
    footer(f, foot, "Arrows choose · Enter confirm · Esc cancel");
}

// ---------------------------------------------------------------------- end

fn render_end(app: &App, f: &mut Frame, es: &EndStats) {
    let area = f.area();
    let t = app.fx.t;
    let buf = f.buffer_mut();
    let (col, title, sub) = if es.victory {
        (
            theme::GOLD,
            "T H E   U N H O L Y   F A L L S",
            "The crypt falls silent. For the first time in an age, dawn reaches the catacombs.",
        )
    } else {
        (
            theme::BLOOD_LIGHT,
            "Y O U   H A V E   P E R I S H E D",
            "Your bones join the countless others. The crypt is patient; it will wait for the next pilgrim.",
        )
    };
    embers(
        buf,
        area,
        t,
        70,
        if es.victory {
            theme::GOLD
        } else {
            theme::BLOOD
        },
    );
    let cy = area.y as i32 + area.height as i32 / 2 - 9;
    let a = if es.victory { art::SKULL } else { art::GRAVE };
    let aw = art_width(a);
    draw_art(
        buf,
        area.x as i32 + (area.width as i32 - aw) / 2,
        cy - 2,
        a,
        area,
        |_, _, _| Some(fg(scale(col, 0.7))),
    );
    let ty = cy + a.len() as i32;
    let glow = mix(col, theme::BONE, flicker(t, 1.0) * 0.4);
    put_center(
        buf,
        area,
        ty,
        title,
        Style::new().fg(glow).add_modifier(Modifier::BOLD),
    );
    let info = es.class.info();
    let r = Rect {
        x: area.x + 6,
        y: (ty + 2) as u16,
        width: area.width - 12,
        height: 3,
    };
    paragraph(
        f,
        r,
        sub,
        Style::new().fg(theme::ASH).add_modifier(Modifier::ITALIC),
        Alignment::Center,
    );
    let buf = f.buffer_mut();
    let lines = [
        format!("{} — {}", info.name, es.cause),
        format!("Floor reached   {}/{}", es.floor.min(FLOORS), FLOORS),
        format!("Foes slain      {}", es.kills),
        format!("Deck size       {}", es.deck),
        format!("Time            {}", fmt_time(es.secs)),
    ];
    for (i, l) in lines.iter().enumerate() {
        let st = if i == 0 {
            Style::new().fg(info.color).add_modifier(Modifier::BOLD)
        } else {
            fg(theme::BONE)
        };
        put_center(buf, area, ty + 6 + i as i32, l, st);
    }
    if app.screen_age() > 1.0 {
        let blink = (t * 2.0).sin() > -0.3;
        if blink {
            put_center(buf, area, ty + 13, "Press Enter to return", fg(theme::ASH));
        }
    }
    app.hit(area, Hit::Item(0));
}

// ---------------------------------------------------------------------- overlays

fn render_pause(app: &App, f: &mut Frame) {
    let area = f.area();
    let r = centered(area, 46, 14);
    let inner = panel(f, r, "Rest a Moment", theme::GOLD, true);
    let buf = f.buffer_mut();
    for (i, item) in app.pause_items().iter().enumerate() {
        let sel = app.osel == i;
        let y = inner.y as i32 + 1 + i as i32 * 2;
        let label = if sel {
            format!("»  {item}  «")
        } else {
            item.to_string()
        };
        let st = if sel {
            Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD)
        } else {
            fg(theme::BONE)
        };
        put_center(buf, inner, y, &label, st);
        app.ohit(
            Rect {
                x: inner.x,
                y: y as u16,
                width: inner.width,
                height: 1,
            },
            Hit::Item(i),
        );
    }
    let note = if app.tutorial.is_some() {
        "Leaving returns you to the title. Your saved run is untouched."
    } else if matches!(app.screen, Screen::Map) {
        "Your run is saved at the start of each room."
    } else {
        "Quitting now resumes at the start of this room."
    };
    paragraph(
        f,
        Rect {
            x: inner.x + 1,
            y: inner.bottom() - 2,
            width: inner.width - 2,
            height: 2,
        },
        note,
        fg(theme::ASH),
        Alignment::Center,
    );
}

fn scroll_lines(app: &App, f: &mut Frame, title: &str, lines: Vec<Line>) {
    let area = f.area();
    let r = centered(
        area,
        area.width.saturating_sub(8).min(100),
        area.height.saturating_sub(4),
    );
    let inner = panel(f, r, title, theme::GOLD, true);
    let body = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        height: inner.height.saturating_sub(1),
        ..inner
    };
    let bw = body.width.max(1) as i32;
    let total: u16 = lines
        .iter()
        .map(|l| (line_width(&l.spans) + bw - 1).max(bw) / bw)
        .sum::<i32>() as u16;
    let p = Paragraph::new(lines).wrap(Wrap { trim: false });
    let max_scroll = total.saturating_sub(body.height);
    let scroll = (app.scroll as u16).min(max_scroll);
    p.scroll((scroll, 0)).render(body, f.buffer_mut());
    put_center(
        f.buffer_mut(),
        inner,
        inner.bottom() as i32 - 1,
        "↑↓ / wheel scroll · Esc close",
        fg(theme::DIM),
    );
    app.ohit(area, Hit::Item(0));
}

fn card_line(c: &Card) -> Line<'static> {
    let d = c.def();
    Line::from(vec![
        Span::styled(
            format!(" {} ", d.cost),
            Style::new().fg(theme::BG).bg(theme::GOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{:<20}", c.name()),
            Style::new()
                .fg(if c.up { theme::HEAL } else { theme::BONE })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{:<7}", d.kind.label()), fg(d.kind.color())),
        Span::styled(d.desc(0, false), fg(theme::ASH)),
    ])
}

fn render_deck_overlay(app: &App, f: &mut Frame) {
    let mut lines: Vec<Line> = Vec::new();
    let section = |lines: &mut Vec<Line>, title: String, cards: &[Card], sort: bool| {
        lines.push(Line::from(Span::styled(
            title,
            Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
        )));
        let mut cs = cards.to_vec();
        if sort {
            cs.sort_by_key(|c| c.name());
        }
        if cs.is_empty() {
            lines.push(Line::from(Span::styled("  (empty)", fg(theme::DIM))));
        }
        for c in &cs {
            lines.push(card_line(c));
        }
        lines.push(Line::raw(""));
    };
    if let Screen::Combat(cv) = &app.screen {
        section(
            &mut lines,
            format!(
                "Draw pile ({}) — shown in no particular order",
                cv.c.draw.len()
            ),
            &cv.c.draw,
            true,
        );
        section(
            &mut lines,
            format!("Discard pile ({})", cv.c.discard.len()),
            &cv.c.discard,
            false,
        );
        section(
            &mut lines,
            format!("Exhausted ({})", cv.c.exhausted.len()),
            &cv.c.exhausted,
            false,
        );
    } else if let Some(run) = &app.run {
        section(
            &mut lines,
            format!("Your deck ({} cards)", run.deck.len()),
            &run.deck,
            true,
        );
    }
    scroll_lines(app, f, "Deck", lines);
}

fn render_help(app: &App, f: &mut Frame) {
    let h = |s: &str| {
        Line::from(Span::styled(
            s.to_string(),
            Style::new().fg(theme::GOLD).add_modifier(Modifier::BOLD),
        ))
    };
    let p = |s: &str| Line::from(Span::styled(s.to_string(), fg(theme::BONE)));
    let mut lines = vec![
        h("THE DESCENT"),
        p(
            "Descend ten floors of the crypt and destroy Morvath, the Unholy Abbot. On each floor, choose one door. \
           Combat earns gold and cards, Elites guard relics, Shrines heal or upgrade, the Merchant sells power, \
           and Unknown rooms hold strange bargains. Your run is saved before each room.",
        ),
        Line::raw(""),
        h("COMBAT"),
        p(
            "Each turn you gain Energy and draw cards. Play cards by spending Energy, then End Turn. Enemies show \
           their Intent above their heads — read it and prepare. Block is lost at the start of your next turn. \
           When your draw pile empties, your discard pile is reshuffled.",
        ),
        Line::raw(""),
        h("CONTROLS"),
        p(
            "Combat:  ←→ or hover select a card · 1-9 or click to play · ↑↓/Tab or click an enemy to target · E end turn",
        ),
        p(
            "Menus:   arrows / mouse to select · Enter or click to confirm · Esc back / menu · D view deck · ? this codex",
        ),
        Line::raw(""),
        h("CLASSES"),
    ];
    for c in Class::ALL {
        let i = c.info();
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} — ", i.name),
                Style::new().fg(i.color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}: {}", i.passive_name, i.passive),
                fg(theme::BONE),
            ),
        ]));
    }
    lines.push(p(
        "Combo (Assassin): bonus effect if you already played a card this turn.",
    ));
    lines.push(Line::raw(""));
    lines.push(h("STATUSES"));
    for s in St::ALL {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<6} {:<13}", s.short(), s.name()),
                Style::new().fg(s.color()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(s.help(), fg(theme::BONE)),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(h("INTENTS"));
    lines.push(p("» ATTACK n   ◆ DEFEND   ▲ EMPOWER (gains Strength)   ✶ HEX (debuffs you)   ☠ CURSE (adds Wounds to your discard)"));
    scroll_lines(app, f, "Codex of the Crypt", lines);
}
