use ratatui::style::Color;

use crate::cards::Card;
use crate::combat::{Ev, Who};
use crate::model::Class;
use crate::theme;

pub struct Float {
    pub who: Who,
    pub text: String,
    pub color: Color,
    pub t0: f32,
    pub lane: i32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SlashStyle {
    Heavy,
    Twin,
    Arcane,
    Claw,
}

/// Transient visual effects. Positions are resolved at render time from `Who` anchors.
#[derive(Default)]
pub struct Fx {
    pub t: f32,
    pub floats: Vec<Float>,
    pub hits: Vec<(Who, f32)>,
    pub slashes: Vec<(Who, f32, SlashStyle)>,
    pub deaths: Vec<(usize, f32)>,
    pub banner: Option<(String, f32, Color)>,
    pub played: Option<(Card, f32)>,
    pub toast: Option<(String, f32)>,
    pub shake: f32,
}

impl Fx {
    pub fn update(&mut self, dt: f32) {
        self.t += dt;
        let t = self.t;
        self.floats.retain(|f| t - f.t0 < 1.3);
        self.hits.retain(|h| t - h.1 < 0.5);
        self.slashes.retain(|s| t - s.1 < 0.5);
        if self.banner.as_ref().is_some_and(|b| t - b.1 > 1.4) {
            self.banner = None;
        }
        if self.played.as_ref().is_some_and(|p| t - p.1 > 0.9) {
            self.played = None;
        }
        if self.toast.as_ref().is_some_and(|p| t - p.1 > 2.5) {
            self.toast = None;
        }
    }

    pub fn clear_combat(&mut self) {
        self.floats.clear();
        self.hits.clear();
        self.slashes.clear();
        self.deaths.clear();
        self.banner = None;
        self.played = None;
    }

    pub fn toast(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), self.t));
    }

    pub fn banner(&mut self, s: impl Into<String>, c: Color) {
        self.banner = Some((s.into(), self.t, c));
    }

    pub fn hit_age(&self, w: Who) -> Option<f32> {
        self.hits.iter().rev().find(|h| h.0 == w).map(|h| self.t - h.1).filter(|a| *a >= 0.0)
    }

    pub fn death_age(&self, i: usize) -> Option<f32> {
        self.deaths.iter().find(|d| d.0 == i).map(|d| self.t - d.1)
    }

    fn float(&mut self, who: Who, text: String, color: Color) {
        // Stagger texts on the same target so multi-hits read in sequence.
        let last = self.floats.iter().filter(|f| f.who == who).map(|f| f.t0).fold(f32::MIN, f32::max);
        let t0 = if last > self.t - 0.15 { last + 0.15 } else { self.t };
        let lane = self.floats.iter().filter(|f| f.who == who && self.t - f.t0 < 0.6).count() as i32 % 3;
        self.floats.push(Float { who, text, color, t0, lane });
    }

    pub fn ingest(&mut self, ev: Ev, class: Class) {
        match ev {
            Ev::Damage(w, lost, blocked) => {
                if blocked > 0 {
                    self.float(w, format!("◆{blocked}"), theme::STEEL);
                }
                if lost > 0 {
                    self.float(w, format!("-{lost}"), theme::BLOOD_LIGHT);
                    self.hits.push((w, self.t));
                    if lost >= 14 {
                        self.shake = self.t;
                    }
                }
            }
            Ev::Dodge(w) => self.float(w, "DODGED".into(), theme::STEEL),
            Ev::Block(w, n) => self.float(w, format!("+{n} Block"), theme::STEEL),
            Ev::Heal(w, n) => self.float(w, format!("+{n} HP"), theme::HEAL),
            Ev::Status(w, s, n) => self.float(w, format!("+{n} {}", s.name()), s.color()),
            Ev::Died(Who::Enemy(i)) => self.deaths.push((i, self.t)),
            Ev::Died(Who::Player) => self.shake = self.t,
            Ev::Played(c) => self.played = Some((c, self.t)),
            Ev::Hit(from, to) => {
                let style = match from {
                    Who::Enemy(_) => SlashStyle::Claw,
                    Who::Player => match class {
                        Class::BloodKnight => SlashStyle::Heavy,
                        Class::Assassin => SlashStyle::Twin,
                        Class::Warlock => SlashStyle::Arcane,
                    },
                };
                self.slashes.push((to, self.t, style));
            }
            Ev::Banner(s) => self.banner(s, theme::BONE),
            Ev::Text(w, s) => self.float(w, s, theme::GOLD),
        }
    }
}
