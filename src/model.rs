use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::theme;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Class {
    BloodKnight,
    Assassin,
    Warlock,
}

pub struct ClassInfo {
    pub name: &'static str,
    pub epithet: &'static str,
    pub hp: i32,
    pub energy: i32,
    pub draw: i32,
    pub color: Color,
    pub blurb: &'static str,
    pub passive_name: &'static str,
    pub passive: &'static str,
}

impl Class {
    pub const ALL: [Class; 3] = [Class::BloodKnight, Class::Assassin, Class::Warlock];

    pub fn info(self) -> ClassInfo {
        match self {
            Class::BloodKnight => ClassInfo {
                name: "Blood Knight",
                epithet: "Oathsworn of the Crimson Plate",
                hp: 80,
                energy: 3,
                draw: 5,
                color: theme::BLOOD,
                blurb: "Clad in black iron and wielding a greatsword forged from a cathedral bell. \
                        Pays for power with his own blood.",
                passive_name: "Blood Armor",
                passive: "Whenever you lose HP from your own cards, gain twice that much Block.",
            },
            Class::Assassin => ClassInfo {
                name: "Assassin",
                epithet: "Knife of the Hollow Choir",
                hp: 62,
                energy: 3,
                draw: 6,
                color: theme::POISON,
                blurb: "Light, quick and venomous. Strings cheap cards into Combos and lets poison \
                        finish what the daggers began.",
                passive_name: "Shadow Cloak",
                passive: "Draw 1 extra card each turn. Begin each combat with 1 Dodge.",
            },
            Class::Warlock => ClassInfo {
                name: "Warlock",
                epithet: "Bearer of the Gravewood Staff",
                hp: 66,
                energy: 3,
                draw: 5,
                color: theme::VIOLET,
                blurb: "Binds foes with Doom. When a foe's Doom equals or exceeds its HP at the end \
                        of your turn, its soul is claimed.",
                passive_name: "Soul Harvest",
                passive: "Whenever an enemy dies, heal 3 HP.",
            },
        }
    }
}

/// Statuses that can be stacked on any fighter.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum St {
    Strength,
    Weak,
    Vulnerable,
    Poison,
    Doom,
    Dodge,
    Thorns,
    Plating,
    VenomBlades,
    DoomAura,
}

pub const ST_COUNT: usize = 10;

impl St {
    pub const ALL: [St; ST_COUNT] = [
        St::Strength,
        St::Weak,
        St::Vulnerable,
        St::Poison,
        St::Doom,
        St::Dodge,
        St::Thorns,
        St::Plating,
        St::VenomBlades,
        St::DoomAura,
    ];

    pub fn name(self) -> &'static str {
        match self {
            St::Strength => "Strength",
            St::Weak => "Weak",
            St::Vulnerable => "Vulnerable",
            St::Poison => "Poison",
            St::Doom => "Doom",
            St::Dodge => "Dodge",
            St::Thorns => "Thorns",
            St::Plating => "Iron Will",
            St::VenomBlades => "Venom Blades",
            St::DoomAura => "Dark Pact",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            St::Strength => "STR",
            St::Weak => "WEAK",
            St::Vulnerable => "VULN",
            St::Poison => "PSN",
            St::Doom => "DOOM",
            St::Dodge => "DODGE",
            St::Thorns => "THRN",
            St::Plating => "WILL",
            St::VenomBlades => "VENOM",
            St::DoomAura => "PACT",
        }
    }

    pub fn color(self) -> Color {
        match self {
            St::Strength => theme::EMBER,
            St::Weak => theme::ASH,
            St::Vulnerable => theme::BLOOD_LIGHT,
            St::Poison => theme::POISON,
            St::Doom => theme::VIOLET,
            St::Dodge => theme::STEEL,
            St::Thorns => theme::GOLD,
            St::Plating => theme::STEEL,
            St::VenomBlades => theme::POISON,
            St::DoomAura => theme::VIOLET,
        }
    }

    pub fn help(self) -> &'static str {
        match self {
            St::Strength => "Attacks deal +1 damage per stack.",
            St::Weak => "Attacks deal 25% less damage. Decreases each turn.",
            St::Vulnerable => "Takes 50% more attack damage. Decreases each turn.",
            St::Poison => "Loses HP equal to Poison at start of turn, then Poison decreases by 1.",
            St::Doom => "At the end of your turn, if Doom >= HP, the enemy dies.",
            St::Dodge => "Completely avoid the next attack hit.",
            St::Thorns => "When hit by an attack, the attacker loses that much HP.",
            St::Plating => "Gain this much Block at the end of your turn.",
            St::VenomBlades => "Each attack hit applies this much Poison.",
            St::DoomAura => "At the start of your turn, apply this much Doom to ALL enemies.",
        }
    }
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Statuses(pub [i32; ST_COUNT]);

impl Statuses {
    pub fn get(&self, s: St) -> i32 {
        self.0[s as usize]
    }
    pub fn add(&mut self, s: St, n: i32) {
        let v = &mut self.0[s as usize];
        *v = v.saturating_add(n).max(0);
    }
    pub fn set(&mut self, s: St, n: i32) {
        self.0[s as usize] = n.max(0);
    }
    pub fn tick_down(&mut self, s: St) {
        self.add(s, -1);
    }
    pub fn active(&self) -> impl Iterator<Item = (St, i32)> + '_ {
        St::ALL.iter().filter_map(|&s| {
            let v = self.get(s);
            (v > 0).then_some((s, v))
        })
    }
}

#[derive(Clone, Default, Debug)]
pub struct Fighter {
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub st: Statuses,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Relic {
    CrackedChalice,
    BlackCandle,
    BoneIdol,
    RustedNail,
    GraveDust,
    EmberOfHate,
    ThornedRosary,
    MournersVeil,
    OssuaryKey,
    EyeOfTheAbyss,
}

impl Relic {
    pub const ALL: [Relic; 10] = [
        Relic::CrackedChalice,
        Relic::BlackCandle,
        Relic::BoneIdol,
        Relic::RustedNail,
        Relic::GraveDust,
        Relic::EmberOfHate,
        Relic::ThornedRosary,
        Relic::MournersVeil,
        Relic::OssuaryKey,
        Relic::EyeOfTheAbyss,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Relic::CrackedChalice => "Cracked Chalice",
            Relic::BlackCandle => "Black Candle",
            Relic::BoneIdol => "Bone Idol",
            Relic::RustedNail => "Rusted Coffin Nail",
            Relic::GraveDust => "Grave Dust",
            Relic::EmberOfHate => "Ember of Hate",
            Relic::ThornedRosary => "Thorned Rosary",
            Relic::MournersVeil => "Mourner's Veil",
            Relic::OssuaryKey => "Ossuary Key",
            Relic::EyeOfTheAbyss => "Eye of the Abyss",
        }
    }

    pub fn desc(self) -> &'static str {
        match self {
            Relic::CrackedChalice => "Heal 5 HP after each combat.",
            Relic::BlackCandle => "Draw 2 extra cards on your first turn.",
            Relic::BoneIdol => "Gain 10 Max HP.",
            Relic::RustedNail => "Begin each combat with 1 Strength.",
            Relic::GraveDust => "Begin each combat by applying 1 Weak to ALL enemies.",
            Relic::EmberOfHate => "Gain 1 extra Energy on your first turn.",
            Relic::ThornedRosary => "Begin each combat with 3 Thorns.",
            Relic::MournersVeil => "Begin each combat with 10 Block.",
            Relic::OssuaryKey => "Combats yield 50% more gold.",
            Relic::EyeOfTheAbyss => "Whenever an enemy dies, draw 1 card.",
        }
    }
}
