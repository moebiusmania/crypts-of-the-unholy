use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::art;
use crate::model::{Fighter, St};
use crate::rng::Rng;
use crate::theme;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum EnemyKind {
    Ghoul,
    Skeleton,
    PlagueRat,
    Cultist,
    BoneArcher,
    Wraith,
    Gargoyle,
    DeathKnight,
    Abomination,
    Morvath,
}

impl EnemyKind {
    pub fn name(self) -> &'static str {
        match self {
            EnemyKind::Ghoul => "Ghoul",
            EnemyKind::Skeleton => "Skeleton",
            EnemyKind::PlagueRat => "Plague Rat",
            EnemyKind::Cultist => "Ash Cultist",
            EnemyKind::BoneArcher => "Bone Archer",
            EnemyKind::Wraith => "Wraith",
            EnemyKind::Gargoyle => "Gargoyle",
            EnemyKind::DeathKnight => "Death Knight",
            EnemyKind::Abomination => "Abomination",
            EnemyKind::Morvath => "Morvath, the Unholy",
        }
    }

    pub fn color(self) -> Color {
        match self {
            EnemyKind::Ghoul => Color::Rgb(150, 160, 120),
            EnemyKind::Skeleton => theme::BONE,
            EnemyKind::PlagueRat => Color::Rgb(140, 120, 90),
            EnemyKind::Cultist => Color::Rgb(200, 90, 60),
            EnemyKind::BoneArcher => Color::Rgb(200, 190, 160),
            EnemyKind::Wraith => Color::Rgb(130, 170, 210),
            EnemyKind::Gargoyle => Color::Rgb(140, 140, 150),
            EnemyKind::DeathKnight => Color::Rgb(120, 110, 150),
            EnemyKind::Abomination => Color::Rgb(160, 150, 90),
            EnemyKind::Morvath => Color::Rgb(200, 60, 80),
        }
    }

    pub fn art(self) -> &'static [&'static str] {
        match self {
            EnemyKind::Ghoul => art::GHOUL,
            EnemyKind::Skeleton => art::SKELETON,
            EnemyKind::PlagueRat => art::RAT,
            EnemyKind::Cultist => art::CULTIST,
            EnemyKind::BoneArcher => art::ARCHER,
            EnemyKind::Wraith => art::WRAITH,
            EnemyKind::Gargoyle => art::GARGOYLE,
            EnemyKind::DeathKnight => art::DEATH_KNIGHT,
            EnemyKind::Abomination => art::ABOMINATION,
            EnemyKind::Morvath => art::MORVATH,
        }
    }

    fn base_hp(self) -> (i32, i32) {
        match self {
            EnemyKind::Ghoul => (24, 29),
            EnemyKind::Skeleton => (20, 24),
            EnemyKind::PlagueRat => (9, 13),
            EnemyKind::Cultist => (30, 35),
            EnemyKind::BoneArcher => (18, 22),
            EnemyKind::Wraith => (34, 39),
            EnemyKind::Gargoyle => (56, 62),
            EnemyKind::DeathKnight => (70, 76),
            EnemyKind::Abomination => (82, 88),
            EnemyKind::Morvath => (150, 151),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Intent {
    Attack(i32),
    Multi(i32, i32),
    Defend(i32),
    AttackDefend(i32, i32),
    Buff(i32),
    BuffDefend(i32, i32),
    Debuff(St, i32),
    AttackDebuff(i32, St, i32),
    Curse(i32),
}

#[derive(Clone, Debug)]
pub struct Enemy {
    pub kind: EnemyKind,
    pub f: Fighter,
    pub intent: Intent,
    pub step: usize,
    pub phase: u8,
    pub dead: bool,
}

pub fn spawn(kind: EnemyKind, floor: usize, rng: &mut Rng) -> Enemy {
    let (lo, hi) = kind.base_hp();
    let mut hp = rng.range(lo, hi);
    if kind != EnemyKind::Morvath {
        hp = (hp as f32 * (1.0 + 0.05 * floor as f32)) as i32;
    }
    let step = match kind {
        EnemyKind::Ghoul | EnemyKind::Skeleton | EnemyKind::BoneArcher | EnemyKind::Wraith => rng.below(3),
        _ => 0,
    };
    let mut e = Enemy {
        kind,
        f: Fighter { hp, max_hp: hp, block: 0, st: Default::default() },
        intent: Intent::Attack(0),
        step,
        phase: 0,
        dead: false,
    };
    match kind {
        EnemyKind::Gargoyle => e.f.st.set(St::Thorns, 3),
        EnemyKind::Wraith => e.f.st.set(St::Dodge, 1),
        _ => {}
    }
    pick_intent(&mut e, floor, rng);
    e
}

pub fn pick_intent(e: &mut Enemy, floor: usize, rng: &mut Rng) {
    use Intent::*;
    let fb = (floor / 3) as i32;
    let s = e.step;
    e.intent = match e.kind {
        EnemyKind::Ghoul => match s % 3 {
            0 | 1 => Attack(7 + fb),
            _ => AttackDebuff(5 + fb, St::Weak, 1),
        },
        EnemyKind::Skeleton => {
            if s % 2 == 0 { AttackDefend(5 + fb, 6) } else { Attack(9 + fb) }
        }
        EnemyKind::PlagueRat => {
            if rng.chance(0.5) { Attack(3 + fb) } else { AttackDebuff(2 + fb, St::Poison, 2) }
        }
        EnemyKind::Cultist => {
            if s == 0 { Buff(3) } else { Attack(6 + fb) }
        }
        EnemyKind::BoneArcher => match s % 3 {
            0 => Multi(3 + fb / 2, 3),
            1 => Debuff(St::Vulnerable, 2),
            _ => Attack(8 + fb),
        },
        EnemyKind::Wraith => match s % 3 {
            0 => Attack(10 + fb),
            1 => Debuff(St::Weak, 2),
            _ => Multi(5 + fb / 2, 2),
        },
        EnemyKind::Gargoyle => match s % 3 {
            0 => Defend(14),
            1 => Attack(15 + fb),
            _ => AttackDefend(9 + fb, 9),
        },
        EnemyKind::DeathKnight => match s % 4 {
            0 => Attack(13 + fb),
            1 => BuffDefend(3, 10),
            2 => Multi(7 + fb / 2, 2),
            _ => Curse(2),
        },
        EnemyKind::Abomination => match s % 3 {
            0 => AttackDebuff(10 + fb, St::Poison, 3),
            1 => Multi(4 + fb / 2, 4),
            _ => Attack(18 + fb),
        },
        EnemyKind::Morvath => {
            if e.phase == 0 {
                match s % 4 {
                    0 => Curse(1),
                    1 => Multi(6, 3),
                    2 => Attack(16),
                    _ => BuffDefend(2, 12),
                }
            } else {
                match s % 4 {
                    0 => Multi(6, 3),
                    1 => AttackDebuff(12, St::Vulnerable, 2),
                    2 => Attack(18),
                    _ => Curse(1),
                }
            }
        }
    };
}

pub fn encounter(floor: usize, elite: bool, boss: bool, rng: &mut Rng) -> Vec<EnemyKind> {
    use EnemyKind::*;
    if boss {
        return vec![Morvath];
    }
    if elite {
        let opts: [&[EnemyKind]; 3] = [&[Gargoyle], &[DeathKnight], &[Abomination]];
        return rng.pick(&opts).to_vec();
    }
    if floor < 3 {
        let opts: [&[EnemyKind]; 4] = [&[Ghoul], &[Skeleton, PlagueRat], &[PlagueRat, PlagueRat, PlagueRat], &[Cultist]];
        rng.pick(&opts).to_vec()
    } else {
        let opts: [&[EnemyKind]; 5] = [
            &[Ghoul, Skeleton],
            &[Wraith],
            &[BoneArcher, Cultist],
            &[PlagueRat, Ghoul, PlagueRat],
            &[Skeleton, BoneArcher],
        ];
        rng.pick(&opts).to_vec()
    }
}
