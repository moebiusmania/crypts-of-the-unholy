use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::model::{Class, St};
use crate::theme;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum CardId {
    // Blood Knight
    Cleave,
    IronGuard,
    BloodOffering,
    CrimsonArc,
    ExecutionersSwing,
    SanguineStrike,
    BulwarkOfBone,
    ShieldBash,
    Wrath,
    Hemorrhage,
    Unbreakable,
    RecklessFury,
    Martyrdom,
    ThornedMail,
    // Assassin
    Stab,
    ShadowVeil,
    Envenom,
    QuickSlice,
    TwinFangs,
    DeadlyVenom,
    ToxicCloud,
    Rupture,
    SmokeBomb,
    Flurry,
    Backstab,
    Shadowstep,
    Assassinate,
    VenomBlades,
    // Warlock
    ShadowBolt,
    DarkWard,
    Hex,
    SiphonLife,
    Doombringer,
    RainOfAsh,
    SoulRend,
    CurseOfFrailty,
    BloodTithe,
    EldritchBlast,
    PactOfShadows,
    DrainEssence,
    VoidShield,
    DarkInsight,
    // Curses
    Wound,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Attack,
    Skill,
    Power,
    Curse,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Attack => "Attack",
            Kind::Skill => "Skill",
            Kind::Power => "Power",
            Kind::Curse => "Curse",
        }
    }
    pub fn color(self) -> Color {
        match self {
            Kind::Attack => theme::BLOOD_LIGHT,
            Kind::Skill => theme::STEEL,
            Kind::Power => theme::VIOLET,
            Kind::Curse => theme::ASH,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rarity {
    Starter,
    Common,
    Uncommon,
    Rare,
    Curse,
}

impl Rarity {
    pub fn color(self) -> Color {
        match self {
            Rarity::Starter | Rarity::Common => theme::ASH,
            Rarity::Uncommon => theme::STEEL,
            Rarity::Rare => theme::GOLD,
            Rarity::Curse => theme::DIM,
        }
    }
    pub fn price(self) -> i32 {
        match self {
            Rarity::Rare => 95,
            Rarity::Uncommon => 65,
            _ => 45,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Eff {
    Dmg(i32),
    DmgAll(i32),
    Multi(i32, i32),
    Block(i32),
    Draw(i32),
    Energy(i32),
    LoseHp(i32),
    Heal(i32),
    Apply(St, i32),
    ApplyAll(St, i32),
    Gain(St, i32),
    ComboDmg(i32),
    ComboDraw(i32),
    DmgFromBlock,
    DmgFromDoom,
    DoublePoison,
    Execute(i32),
    Drain(i32),
}

impl Eff {
    pub fn needs_target(self) -> bool {
        matches!(
            self,
            Eff::Dmg(_)
                | Eff::Multi(..)
                | Eff::Apply(..)
                | Eff::ComboDmg(_)
                | Eff::DmgFromBlock
                | Eff::DmgFromDoom
                | Eff::DoublePoison
                | Eff::Execute(_)
                | Eff::Drain(_)
        )
    }
}

pub struct CardDef {
    pub name: &'static str,
    pub cost: i32,
    pub kind: Kind,
    pub rarity: Rarity,
    pub effs: Vec<Eff>,
    pub exhaust: bool,
}

impl CardDef {
    pub fn needs_target(&self) -> bool {
        self.effs.iter().any(|e| e.needs_target())
    }

    pub fn self_damage(&self) -> i32 {
        self.effs
            .iter()
            .map(|e| if let Eff::LoseHp(n) = e { *n } else { 0 })
            .sum()
    }

    /// Card text. `strength`/`weak` adjust displayed attack numbers in combat.
    pub fn desc(&self, strength: i32, weak: bool) -> String {
        let atk = |n: i32| {
            let v = (n + strength).max(0);
            if weak { v * 3 / 4 } else { v }
        };
        let mut parts: Vec<String> = Vec::new();
        for e in &self.effs {
            parts.push(match *e {
                Eff::Dmg(n) => format!("Deal {} damage.", atk(n)),
                Eff::DmgAll(n) => format!("Deal {} damage to ALL enemies.", atk(n)),
                Eff::Multi(n, k) => format!("Deal {} damage {} times.", atk(n), k),
                Eff::Block(n) => format!("Gain {n} Block."),
                Eff::Draw(1) => "Draw 1 card.".into(),
                Eff::Draw(n) => format!("Draw {n} cards."),
                Eff::Energy(n) => format!("Gain {n} Energy."),
                Eff::LoseHp(n) => format!("Lose {n} HP."),
                Eff::Heal(n) => format!("Heal {n} HP."),
                Eff::Apply(s, n) => format!("Apply {n} {}.", s.name()),
                Eff::ApplyAll(s, n) => format!("Apply {n} {} to ALL enemies.", s.name()),
                Eff::Gain(s, n) => match s {
                    St::Plating => format!("At the end of each turn, gain {n} Block."),
                    St::VenomBlades => format!("Your attack hits apply {n} Poison."),
                    St::DoomAura => {
                        format!("At the start of each turn, apply {n} Doom to ALL enemies.")
                    }
                    _ => format!("Gain {n} {}.", s.name()),
                },
                Eff::ComboDmg(n) => format!("Combo: deal {} more.", atk(n)),
                Eff::ComboDraw(1) => "Combo: draw 1 card.".into(),
                Eff::ComboDraw(n) => format!("Combo: draw {n} cards."),
                Eff::DmgFromBlock => "Deal damage equal to your Block.".into(),
                Eff::DmgFromDoom => "Deal damage equal to the target's Doom.".into(),
                Eff::DoublePoison => "Double the target's Poison.".into(),
                Eff::Execute(n) => {
                    format!("Deal {} damage. Double vs. foes below half HP.", atk(n))
                }
                Eff::Drain(n) => format!("Deal {} damage. Heal unblocked damage.", atk(n)),
            });
        }
        if self.kind == Kind::Curse {
            parts.push("Unplayable. Clogs your hand.".into());
        }
        if self.exhaust {
            parts.push("Exhaust.".into());
        }
        parts.join(" ")
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Card {
    pub id: CardId,
    pub up: bool,
}

impl Card {
    pub fn new(id: CardId) -> Self {
        Card { id, up: false }
    }
    pub fn def(&self) -> CardDef {
        def(self.id, self.up)
    }
    pub fn name(&self) -> String {
        let d = self.def();
        if self.up {
            format!("{}+", d.name)
        } else {
            d.name.to_string()
        }
    }
    pub fn upgradable(&self) -> bool {
        !self.up && self.def().kind != Kind::Curse
    }
}

pub fn def(id: CardId, up: bool) -> CardDef {
    use CardId::*;
    use Eff::*;
    use Kind::*;
    use Rarity::*;
    let u = |a: i32, b: i32| if up { b } else { a };
    let c = |name, cost, kind, rarity, effs: Vec<Eff>| CardDef {
        name,
        cost,
        kind,
        rarity,
        effs,
        exhaust: false,
    };
    let x = |mut d: CardDef| {
        d.exhaust = true;
        d
    };
    match id {
        Cleave => c("Cleave", 1, Attack, Starter, vec![Dmg(u(7, 10))]),
        IronGuard => c("Iron Guard", 1, Skill, Starter, vec![Block(u(6, 9))]),
        BloodOffering => c(
            "Blood Offering",
            0,
            Skill,
            Starter,
            if up {
                vec![LoseHp(3), Energy(2), Draw(1)]
            } else {
                vec![LoseHp(3), Energy(2)]
            },
        ),
        CrimsonArc => c(
            "Crimson Arc",
            2,
            Attack,
            Starter,
            vec![LoseHp(2), DmgAll(u(10, 14))],
        ),
        ExecutionersSwing => c(
            "Executioner's Swing",
            2,
            Attack,
            Uncommon,
            vec![Execute(u(14, 18))],
        ),
        SanguineStrike => c(
            "Sanguine Strike",
            1,
            Attack,
            Common,
            vec![Dmg(u(8, 11)), Heal(u(3, 4))],
        ),
        BulwarkOfBone => c("Bulwark of Bone", 2, Skill, Common, vec![Block(u(14, 19))]),
        ShieldBash => c("Shield Bash", u(1, 0), Attack, Common, vec![DmgFromBlock]),
        Wrath => c(
            "Wrath",
            1,
            Power,
            Uncommon,
            vec![LoseHp(3), Gain(St::Strength, u(2, 3))],
        ),
        Hemorrhage => c(
            "Hemorrhage",
            1,
            Attack,
            Common,
            vec![Dmg(u(6, 8)), Apply(St::Vulnerable, u(2, 3))],
        ),
        Unbreakable => c(
            "Unbreakable",
            1,
            Power,
            Uncommon,
            vec![Gain(St::Plating, u(4, 6))],
        ),
        RecklessFury => c(
            "Reckless Fury",
            0,
            Attack,
            Common,
            vec![LoseHp(2), Multi(u(4, 6), 2)],
        ),
        Martyrdom => c(
            "Martyrdom",
            2,
            Attack,
            Rare,
            vec![LoseHp(6), Dmg(u(28, 36))],
        ),
        ThornedMail => c(
            "Thorned Mail",
            1,
            Power,
            Rare,
            vec![Gain(St::Thorns, u(3, 5)), Block(u(5, 8))],
        ),

        Stab => c("Stab", 1, Attack, Starter, vec![Dmg(u(6, 9))]),
        ShadowVeil => c("Shadow Veil", 1, Skill, Starter, vec![Block(u(5, 8))]),
        Envenom => c(
            "Envenom",
            1,
            Skill,
            Starter,
            vec![Apply(St::Poison, u(4, 6))],
        ),
        QuickSlice => c(
            "Quick Slice",
            0,
            Attack,
            Starter,
            vec![Dmg(u(3, 5)), ComboDraw(1)],
        ),
        TwinFangs => c(
            "Twin Fangs",
            1,
            Attack,
            Common,
            vec![Multi(u(4, 5), 2), ComboDmg(u(4, 6))],
        ),
        DeadlyVenom => c(
            "Deadly Venom",
            1,
            Skill,
            Common,
            vec![Apply(St::Poison, u(7, 10))],
        ),
        ToxicCloud => c(
            "Toxic Cloud",
            2,
            Skill,
            Uncommon,
            vec![ApplyAll(St::Poison, u(5, 7))],
        ),
        Rupture => x(c("Rupture", u(1, 0), Skill, Uncommon, vec![DoublePoison])),
        SmokeBomb => c(
            "Smoke Bomb",
            u(1, 0),
            Skill,
            Common,
            vec![Gain(St::Dodge, 1), Draw(1)],
        ),
        Flurry => c("Flurry", 1, Attack, Common, vec![Multi(u(2, 3), 4)]),
        Backstab => x(c("Backstab", 0, Attack, Uncommon, vec![Dmg(u(11, 15))])),
        Shadowstep => x(c("Shadowstep", 0, Skill, Uncommon, vec![Draw(u(2, 3))])),
        Assassinate => c(
            "Assassinate",
            2,
            Attack,
            Rare,
            vec![Dmg(u(14, 18)), ComboDmg(u(14, 18))],
        ),
        VenomBlades => c(
            "Venom Blades",
            1,
            Power,
            Rare,
            vec![Gain(St::VenomBlades, u(1, 2))],
        ),

        ShadowBolt => c("Shadow Bolt", 1, Attack, Starter, vec![Dmg(u(7, 10))]),
        DarkWard => c("Dark Ward", 1, Skill, Starter, vec![Block(u(5, 8))]),
        Hex => c(
            "Hex",
            1,
            Skill,
            Starter,
            vec![Apply(St::Weak, u(2, 3)), Apply(St::Doom, u(6, 9))],
        ),
        SiphonLife => c("Siphon Life", 1, Attack, Starter, vec![Drain(u(6, 9))]),
        Doombringer => c(
            "Doombringer",
            1,
            Skill,
            Common,
            vec![Apply(St::Doom, u(12, 17))],
        ),
        RainOfAsh => c(
            "Rain of Ash",
            2,
            Attack,
            Uncommon,
            vec![DmgAll(u(8, 11)), ApplyAll(St::Doom, u(4, 6))],
        ),
        SoulRend => c("Soul Rend", u(2, 1), Attack, Rare, vec![DmgFromDoom]),
        CurseOfFrailty => c(
            "Curse of Frailty",
            0,
            Skill,
            Common,
            vec![Apply(St::Vulnerable, u(2, 3))],
        ),
        BloodTithe => c(
            "Blood Tithe",
            0,
            Skill,
            Common,
            vec![LoseHp(3), Energy(u(1, 2)), Draw(1)],
        ),
        EldritchBlast => c("Eldritch Blast", 2, Attack, Common, vec![Dmg(u(18, 24))]),
        PactOfShadows => c(
            "Pact of Shadows",
            1,
            Power,
            Uncommon,
            vec![Gain(St::DoomAura, u(3, 5))],
        ),
        DrainEssence => c("Drain Essence", 2, Attack, Uncommon, vec![Drain(u(11, 15))]),
        VoidShield => c(
            "Void Shield",
            1,
            Skill,
            Common,
            vec![Block(u(8, 11)), ApplyAll(St::Weak, 1)],
        ),
        DarkInsight => c("Dark Insight", u(1, 0), Skill, Common, vec![Draw(3)]),

        Wound => c("Wound", 0, Kind::Curse, Rarity::Curse, vec![]),
    }
}

pub fn starter_deck(class: Class) -> Vec<Card> {
    use CardId::*;
    let ids: &[CardId] = match class {
        Class::BloodKnight => &[
            Cleave,
            Cleave,
            Cleave,
            IronGuard,
            IronGuard,
            IronGuard,
            BloodOffering,
            CrimsonArc,
        ],
        Class::Assassin => &[
            Stab, Stab, Stab, ShadowVeil, ShadowVeil, Envenom, QuickSlice, QuickSlice,
        ],
        Class::Warlock => &[
            ShadowBolt, ShadowBolt, ShadowBolt, DarkWard, DarkWard, DarkWard, Hex, SiphonLife,
        ],
    };
    ids.iter().map(|&id| Card::new(id)).collect()
}

/// Cards that can appear as rewards or in shops for a class.
pub fn class_pool(class: Class) -> Vec<CardId> {
    use CardId::*;
    match class {
        Class::BloodKnight => vec![
            ExecutionersSwing,
            SanguineStrike,
            BulwarkOfBone,
            ShieldBash,
            Wrath,
            Hemorrhage,
            Unbreakable,
            RecklessFury,
            Martyrdom,
            ThornedMail,
        ],
        Class::Assassin => vec![
            TwinFangs,
            DeadlyVenom,
            ToxicCloud,
            Rupture,
            SmokeBomb,
            Flurry,
            Backstab,
            Shadowstep,
            Assassinate,
            VenomBlades,
        ],
        Class::Warlock => vec![
            Doombringer,
            RainOfAsh,
            SoulRend,
            CurseOfFrailty,
            BloodTithe,
            EldritchBlast,
            PactOfShadows,
            DrainEssence,
            VoidShield,
            DarkInsight,
        ],
    }
}
